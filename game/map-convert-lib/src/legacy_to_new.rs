use std::{
    collections::HashMap,
    future::Future,
    io::Cursor,
    num::{NonZeroU8, NonZeroU32},
    path::Path,
    pin::Pin,
    sync::{Arc, Mutex},
};

use anyhow::anyhow;
use base::{
    benchmark::Benchmark,
    hash::{Hash, generate_hash_for},
};
use base_io::io::IoFileSys;
use legacy_map::datafile::{
    CDatafileWrapper, LegacyMapToNewOutput, LegacyMapToNewRes, MapFileImageReadOptions,
    MapFileLayersReadOptions, MapFileOpenOptions, MapFileSoundReadOptions,
};
use oxipng::optimize_from_memory;
use rayon::iter::{IntoParallelIterator, IntoParallelRefMutIterator, ParallelIterator};
use vorbis_rs::VorbisEncoderBuilder;

pub fn legacy_to_new(
    path: &Path,
    io: &IoFileSys,
    thread_pool: &Arc<rayon::ThreadPool>,
    optimize: bool,
) -> anyhow::Result<LegacyMapToNewOutput> {
    let fs = io.fs.clone();
    let map_name = path.to_path_buf();
    let map_file = io
        .rt
        .spawn(async move {
            let path = map_name.as_ref();
            let map = fs.read_file(path).await?;
            Ok(map)
        })
        .get()?;

    legacy_to_new_from_buf(
        map_file,
        path.file_stem()
            .ok_or(anyhow!("wrong file name"))?
            .to_str()
            .ok_or(anyhow!("file name not utf8"))?,
        io,
        thread_pool,
        optimize,
    )
}

pub async fn legacy_to_new_from_buf_async(
    map_file: Vec<u8>,
    name: &str,
    load_image: impl Fn(&Path) -> Pin<Box<dyn Future<Output = anyhow::Result<Vec<u8>>> + Send>>,
    thread_pool: &Arc<rayon::ThreadPool>,
    optimize: bool,
) -> anyhow::Result<LegacyMapToNewOutput> {
    let mut map_legacy = CDatafileWrapper::new();
    let load_options = MapFileOpenOptions::default();
    let res = map_legacy.open(&map_file, name, thread_pool.as_ref(), &load_options);
    match res {
        Ok(data_start) => {
            CDatafileWrapper::read_map_layers(
                &map_legacy.data_file,
                &mut map_legacy.layers,
                data_start,
                &MapFileLayersReadOptions::default(),
            );

            let imgs = CDatafileWrapper::read_image_data(
                &map_legacy.data_file,
                &map_legacy.images,
                data_start,
                &MapFileImageReadOptions {
                    do_benchmark: false,
                },
            );
            for (i, img) in imgs.into_iter().enumerate() {
                if let Some((_, _, img)) = img {
                    map_legacy.images[i].internal_img = Some(img);
                }
            }

            let snds = CDatafileWrapper::read_sound_data(
                &map_legacy.data_file,
                &map_legacy.sounds,
                data_start,
                &MapFileSoundReadOptions {
                    do_benchmark: false,
                },
            );
            for (i, snd) in snds.into_iter().enumerate() {
                if let Some((_, snd)) = snd {
                    map_legacy.sounds[i].data = Some(snd);
                }
            }
        }
        Err(err) => {
            return Err(anyhow!("map not loaded {err}"));
        }
    }
    map_legacy.init_layers(thread_pool);

    let read_files = map_legacy.read_files.clone();
    let mut images: Vec<Vec<u8>> = Default::default();
    for read_file_path in read_files.keys() {
        let read_file_path = read_file_path.to_string();
        let file = load_image(read_file_path.as_ref()).await?;
        images.push(file)
    }
    let benchmark = Benchmark::new(true);

    benchmark.bench("encoding images to png");
    let mut map_output = map_legacy.into_map(thread_pool, &images, Default::default(), true)?;
    benchmark.bench("converting map");

    if optimize {
        thread_pool.install(|| {
            let hashes: Mutex<HashMap<Hash, Hash>> = Default::default();
            map_output.resources.images = std::mem::take(&mut map_output.resources.images)
                .into_par_iter()
                .map(|(old_hash, mut i)| {
                    i.buf = optimize_from_memory(&i.buf, &oxipng::Options::default())?;
                    let hash = generate_hash_for(&i.buf);
                    hashes.lock().unwrap().insert(old_hash, hash);
                    anyhow::Ok((hash, i))
                })
                .collect::<anyhow::Result<HashMap<Hash, LegacyMapToNewRes>>>()?;

            map_output
                .map
                .resources
                .images
                .par_iter_mut()
                .chain(map_output.map.resources.image_arrays.par_iter_mut())
                .for_each(|img| {
                    // update hashes
                    img.meta.blake3_hash =
                        *hashes.lock().unwrap().get(&img.meta.blake3_hash).unwrap();
                });

            anyhow::Ok(())
        })?;
    }

    thread_pool.install(|| {
        let hashes: Mutex<HashMap<Hash, Hash>> = Default::default();
        map_output.resources.sounds = std::mem::take(&mut map_output.resources.sounds)
            .into_iter()
            .map(|(old_hash, mut res)| {
                // transcode from opus to vorbis
                let (raw, header) = ogg_opus::decode::<_, 48000>(Cursor::new(&res.buf))?;
                let mut transcoded_ogg = vec![];
                let mut encoder = VorbisEncoderBuilder::new_with_serial(
                    NonZeroU32::new(48000).unwrap(),
                    NonZeroU8::new(2).unwrap(),
                    &mut transcoded_ogg,
                    0,
                )
                .build()?;

                let (channel1, channel2): (Vec<_>, Vec<_>) = raw
                    .chunks_exact(header.channels as usize)
                    .map(|freq| {
                        if freq.len() == 1 {
                            (
                                (freq[0] as f64 / i16::MAX as f64) as f32,
                                (freq[0] as f64 / i16::MAX as f64) as f32,
                            )
                        } else {
                            (
                                (freq[0] as f64 / i16::MAX as f64) as f32,
                                (freq[1] as f64 / i16::MAX as f64) as f32,
                            )
                        }
                    })
                    .unzip();
                encoder.encode_audio_block([channel1, channel2])?;
                encoder.finish()?;

                res.ty = "ogg".into();

                let hash = generate_hash_for(&transcoded_ogg);
                res.buf = transcoded_ogg;
                hashes.lock().unwrap().insert(old_hash, hash);
                anyhow::Ok((hash, res))
            })
            .collect::<anyhow::Result<_>>()?;

        map_output
            .map
            .resources
            .sounds
            .par_iter_mut()
            .for_each(|res| {
                // update hash after conversion
                res.meta.blake3_hash = *hashes.lock().unwrap().get(&res.meta.blake3_hash).unwrap();
                res.meta.ty = "ogg".try_into().unwrap();
            });

        anyhow::Ok(())
    })?;

    Ok(map_output)
}

pub fn legacy_to_new_from_buf(
    map_file: Vec<u8>,
    name: &str,
    io: &IoFileSys,
    thread_pool: &Arc<rayon::ThreadPool>,
    optimize: bool,
) -> anyhow::Result<LegacyMapToNewOutput> {
    let tp = thread_pool.clone();
    let name = name.to_string();
    let name = name.to_string();
    let fs = io.fs.clone();
    io.rt
        .spawn(async move {
            legacy_to_new_from_buf_async(
                map_file,
                &name,
                |path| {
                    let path = path.to_path_buf();
                    let fs = fs.clone();
                    Box::pin(async move { Ok(fs.read_file(&path).await?) })
                },
                &tp,
                optimize,
            )
            .await
        })
        .get()
}
