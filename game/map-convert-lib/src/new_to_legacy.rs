use anyhow::anyhow;
use base::{benchmark::Benchmark, hash::fmt_hash};
use base_io::io::IoFileSys;
use legacy_map::datafile::CDatafileWrapper;
use map::{file::MapFileReader, map::Map};
use std::{future::Future, io::Cursor, path::Path, pin::Pin, sync::Arc};
use vorbis_rs::VorbisDecoder;

// the map is prepared to be written to disk. the map format is not used in the code base
#[derive(Debug)]
pub struct NewMapToLegacyOutput {
    pub map: Vec<u8>,
}

pub async fn new_to_legacy_from_buf_async(
    file: &[u8],
    load_resources: impl FnOnce(
        &Map,
    ) -> Pin<
        Box<dyn Future<Output = anyhow::Result<(Vec<Vec<u8>>, Vec<Vec<u8>>, Vec<Vec<u8>>)>> + Send>,
    >,
    thread_pool: &Arc<rayon::ThreadPool>,
) -> anyhow::Result<NewMapToLegacyOutput> {
    let map = Map::read(&MapFileReader::new(file.to_vec())?, thread_pool)
        .map_err(|err| anyhow!("loading map from file failed: {err}"))?;

    let (images, image_arrays, mut sounds) = load_resources(&map).await?;

    for (sound, sound_def) in sounds.iter_mut().zip(map.resources.sounds.iter()) {
        if sound_def.meta.ty.as_str() == "ogg" {
            let mut new_sound = Vec::new();
            {
                let mut decoder = VorbisDecoder::new(Cursor::new(&*sound))?;
                let channels = decoder.channels();
                // prepare vorbis stream as i64 samples
                while let Some(decoded) = decoder.decode_audio_block()? {
                    let raw = decoded.samples();

                    let (channel1, channel2) = if channels.get() == 1 {
                        (raw[0].to_vec(), raw[0].to_vec())
                    } else {
                        (raw[0].to_vec(), raw[1].to_vec())
                    };

                    new_sound.extend(channel1.into_iter().zip(channel2.into_iter()).flat_map(
                        |(freq1, freq2)| {
                            [
                                (freq1 as f64 * i16::MAX as f64)
                                    .clamp(i16::MIN as f64, i16::MAX as f64)
                                    as i16,
                                (freq2 as f64 * i16::MAX as f64)
                                    .clamp(i16::MIN as f64, i16::MAX as f64)
                                    as i16,
                            ]
                        },
                    ));
                }
            }

            // encode opus
            *sound = ogg_opus::encode::<48000, 2>(&new_sound)?;
        }
    }

    let benchmark = Benchmark::new(true);
    let map_legacy = CDatafileWrapper::from_map(map, &images, &image_arrays, &sounds);
    benchmark.bench("converting to legacy");
    Ok(NewMapToLegacyOutput { map: map_legacy })
}

pub fn new_to_legacy_from_buf(
    file: &[u8],
    io: &IoFileSys,
    thread_pool: &Arc<rayon::ThreadPool>,
) -> anyhow::Result<NewMapToLegacyOutput> {
    let tp = thread_pool.clone();
    let fs = io.fs.clone();
    let file = file.to_vec();

    io.rt
        .spawn(async move {
            new_to_legacy_from_buf_async(
                &file,
                |map| {
                    let resources = map.resources.clone();
                    Box::pin(async move {
                        let mut images: Vec<Vec<u8>> = Default::default();
                        for image in &resources.images {
                            let img_file = fs
                                .read_file(
                                    format!(
                                        "map/resources/images/{}_{}.{}",
                                        image.name.as_str(),
                                        fmt_hash(&image.meta.blake3_hash),
                                        image.meta.ty.as_str()
                                    )
                                    .as_ref(),
                                )
                                .await
                                .map_err(|err| anyhow!("loading images failed: {err}"))?;
                            images.push(img_file);
                        }

                        let mut image_arrays: Vec<Vec<u8>> = Default::default();
                        for image_array in &resources.image_arrays {
                            let img_file = fs
                                .read_file(
                                    format!(
                                        "map/resources/images/{}_{}.{}",
                                        image_array.name.as_str(),
                                        fmt_hash(&image_array.meta.blake3_hash),
                                        image_array.meta.ty.as_str()
                                    )
                                    .as_ref(),
                                )
                                .await
                                .map_err(|err| anyhow!("loading images failed: {err}"))?;
                            image_arrays.push(img_file);
                        }

                        let mut sounds: Vec<Vec<u8>> = Default::default();
                        for sound in &resources.sounds {
                            let img_file = fs
                                .read_file(
                                    format!(
                                        "map/resources/sounds/{}_{}.{}",
                                        sound.name.as_str(),
                                        fmt_hash(&sound.meta.blake3_hash),
                                        sound.meta.ty.as_str()
                                    )
                                    .as_ref(),
                                )
                                .await
                                .map_err(|err| anyhow!("loading sound failed: {err}"))?;
                            sounds.push(img_file);
                        }
                        Ok((images, image_arrays, sounds))
                    })
                },
                &tp,
            )
            .await
        })
        .get_storage()
}

/// this function will only be supported as long as the map format is equally convertable to the old format
pub fn new_to_legacy(
    path: &Path,
    io: &IoFileSys,
    thread_pool: &Arc<rayon::ThreadPool>,
) -> anyhow::Result<NewMapToLegacyOutput> {
    let fs = io.fs.clone();
    let map_name2 = path.to_path_buf();
    let map = io
        .rt
        .spawn(async move {
            let path = map_name2.as_ref();
            let map = fs
                .read_file(path)
                .await
                .map_err(|err| anyhow!("loading map file failed: {err}"))?;

            Ok(map)
        })
        .get_storage()
        .map_err(|err| anyhow!("loading map failed: {err}"))?;
    new_to_legacy_from_buf(&map, io, thread_pool)
}
