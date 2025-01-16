use std::{
    borrow::Cow,
    collections::HashMap,
    ffi::{CStr, CString},
    io::{Read, Write},
    mem::size_of,
    time::Duration,
};

use anyhow::anyhow;
use base::{
    benchmark::Benchmark, hash::Hash, join_all, linked_hash_map_view::FxLinkedHashMap,
    reduced_ascii_str::ReducedAsciiString,
};
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use hashlink::LinkedHashMap;
use hiarc::Hiarc;
use image_utils::{
    png::{load_png_image_as_rgba, resize_rgba, save_png_image, PngValidatorOptions},
    utils,
};
use math::math::{
    f2fx, fx2f,
    vector::{ffixed, fvec2, fvec3, ivec2, ivec4, nffixed, nfvec4, uffixed, ufvec2, vec1_base},
};
use rayon::{
    prelude::{IndexedParallelIterator, IntoParallelRefMutIterator, ParallelIterator},
    slice::ParallelSliceMut,
};

use map::{
    map::{
        self as mapnew,
        animations::{
            AnimPointColor, AnimPointCurveType, AnimPointPos, AnimPointSound, Animations,
            ColorAnimation, PosAnimation, SoundAnimation,
        },
        config::Config,
        groups::{
            layers::{
                design::{
                    MapLayerQuadsAttrs, MapLayerSound, MapLayerSoundAttrs, Quad, Sound, SoundShape,
                },
                physics::{
                    MapLayerPhysics, MapLayerTilePhysicsBase, MapLayerTilePhysicsSwitch,
                    MapLayerTilePhysicsTele, MapLayerTilePhysicsTune, MapLayerTilePhysicsTuneZone,
                },
                tiles::{
                    MapTileLayerAttr, SpeedupTile, SwitchTile, TeleTile, TileBase, TileFlags,
                    TuneTile,
                },
            },
            MapGroup, MapGroupAttr, MapGroupAttrClipping, MapGroupPhysicsAttr,
        },
        metadata::Metadata,
        resources::{MapResourceMetaData, MapResourceRef, Resources},
        Map,
    },
    types::NonZeroU16MinusOne,
};

use crate::mapdef_06::{
    read_i32_le, read_u32_le, CEnvPoint, CMapItemEnvelope, CMapItemEnvelopeVer, CMapItemGroup,
    CMapItemImage, CMapItemInfo, CMapItemInfoSettings, CMapItemLayer, CMapItemLayerQuads,
    CMapItemLayerSounds, CMapItemLayerSoundsVer, CMapItemLayerTilemap, CMapItemSound,
    CMapItemVersion, CQuad, CSoundShape, CSoundSource, CSpeedupTile, CSwitchTile, CTeleTile, CTile,
    CTuneTile, CurveType, LayerFlag, MapImage, MapInfo, MapItemTypes, MapLayer, MapLayerQuad,
    MapLayerTile, MapLayerTypes, MapSound, MapTileLayerDetail, ReadFromSliceWriteToVec,
    SoundShapeTy, TilesLayerFlag,
};

enum UuidOffset {
    Uuid = 0x8000,
}

#[repr(C)]
pub struct CUuid {
    data: [u8; 16],
}

#[repr(C)]
pub struct CItemEx {
    uuid: [i32; std::mem::size_of::<CUuid>() / 4],
}
/*
TODO: impl
static CItemEx FromUuid(CUuid Uuid)
{
    CItemEx Result;
    for(i = 0; i < (int)sizeof(CUuid) / 4: i32, i++)
        Result.m_aUuid[i] = bytes_be_to_int(&Uuid.m_aData[i * 4]);
    return Result;
}

CUuid ToUuid() const
{
    CUuid Result;
    for(i = 0; i < (int)sizeof(CUuid) / 4: i32, i++)
        int_to_bytes_be(&Result.m_aData[i * 4], m_aUuid[i]);
    return Result;
} */

#[derive(Debug, Hiarc, Copy, Clone, Default)]
#[repr(C)]
struct CDatafileItemType {
    item_type: i32,
    start: i32,
    num: i32,
}

impl CDatafileItemType {
    pub fn read_from_slice(data: &[u8]) -> Self {
        let (item_type, rest) = data.split_at(size_of::<i32>());
        let i_type = read_i32_le(item_type);

        let (start, rest) = rest.split_at(size_of::<i32>());
        let s = read_i32_le(start);

        let (num, _rest) = rest.split_at(size_of::<i32>());
        let n = read_i32_le(num);

        Self {
            item_type: i_type,
            start: s,
            num: n,
        }
    }

    pub fn write_to_vec(&self, w: &mut Vec<u8>) {
        w.extend(self.item_type.to_le_bytes());
        w.extend(self.start.to_le_bytes());
        w.extend(self.num.to_le_bytes());
    }
}

#[repr(C)]
struct CDatafileItem {
    type_and_id: i32,
    size: i32,
}

impl CDatafileItem {
    pub fn read_from_slice(data: &[u8]) -> Self {
        let (type_and_id, rest) = data.split_at(size_of::<i32>());
        let t_and_id = read_i32_le(type_and_id);

        let (size, _rest) = rest.split_at(size_of::<i32>());
        let s = read_i32_le(size);

        Self {
            type_and_id: t_and_id,
            size: s,
        }
    }

    pub fn write_to_vec(&self, w: &mut Vec<u8>) {
        w.extend(self.type_and_id.to_le_bytes());
        w.extend(self.size.to_le_bytes());
    }
}

#[repr(C)]
struct CDatafileItemAndData<'a> {
    header: CDatafileItem,
    data: &'a [u8],
}

impl<'a> CDatafileItemAndData<'a> {
    pub fn read_from_slice(data: &'a [u8], item_size: usize) -> Self {
        let header = CDatafileItem::read_from_slice(data);

        let (_, rest) = data.split_at(size_of::<CDatafileItem>());

        let (rest, _) = rest.split_at(item_size);

        Self { header, data: rest }
    }
}

#[derive(Debug, Hiarc, Default, Clone, Copy)]
#[repr(C)]
struct CDatafileHeader {
    id: [i8; 4],
    version: u32,
    size: u32,
    swap_len: u32,
    num_item_types: u32,
    num_items: u32,
    num_raw_data: u32,
    item_size: u32,
    data_size: u32,
}

impl CDatafileHeader {
    pub fn read_from_slice(data: &[u8]) -> Self {
        let mut rest = data;
        let mut id: [u8; 4] = Default::default();
        id.iter_mut().for_each(|b| {
            let (id, rest2) = rest.split_at(size_of::<i8>());
            *b = id[0];
            rest = rest2;
        });

        let (version, rest) = rest.split_at(size_of::<u32>());
        let ver = read_u32_le(version);

        let (size, rest) = rest.split_at(size_of::<u32>());
        let siz = read_u32_le(size);

        let (swaplen, rest) = rest.split_at(size_of::<u32>());
        let swapln = read_u32_le(swaplen);

        let (num_item_types, rest) = rest.split_at(size_of::<u32>());
        let item_types_num = read_u32_le(num_item_types);

        let (num_item, rest) = rest.split_at(size_of::<u32>());
        let item_num = read_u32_le(num_item);

        let (num_raw_data, rest) = rest.split_at(size_of::<u32>());
        let raw_data_num = read_u32_le(num_raw_data);

        let (item_size, rest) = rest.split_at(size_of::<u32>());
        let i_size = read_u32_le(item_size);

        let (data_size, _rest) = rest.split_at(size_of::<u32>());
        let d_size = read_u32_le(data_size);

        Self {
            id: [id[0] as i8, id[1] as i8, id[2] as i8, id[3] as i8],
            version: ver,
            size: siz,
            swap_len: swapln,
            num_item_types: item_types_num,
            num_items: item_num,
            num_raw_data: raw_data_num,
            item_size: i_size,
            data_size: d_size,
        }
    }

    pub fn write_to_vec(&self, w: &mut Vec<u8>) {
        w.extend(self.id.iter().flat_map(|v| v.to_le_bytes()));
        w.extend(self.version.to_le_bytes());
        w.extend(self.size.to_le_bytes());
        w.extend(self.swap_len.to_le_bytes());
        w.extend(self.num_item_types.to_le_bytes());
        w.extend(self.num_items.to_le_bytes());
        w.extend(self.num_raw_data.to_le_bytes());
        w.extend(self.item_size.to_le_bytes());
        w.extend(self.data_size.to_le_bytes());
    }
}

#[derive(Debug, Hiarc, Clone, Default)]
#[repr(C)]
struct CDatafileInfo {
    item_types: Vec<CDatafileItemType>,
    item_offsets: Vec<i32>,
    data_offsets: Vec<i32>,
    data_sizes: Vec<i32>,
}

#[derive(Debug, Hiarc, Clone, Default)]
#[repr(C)]
pub struct CDatafile {
    /*IOHANDLE m_File;
    SHA256_DIGEST m_Sha256;
    unsigned m_Crc;*/
    info: CDatafileInfo,
    header: CDatafileHeader,
}

#[derive(Debug, Hiarc, Clone)]
pub enum ReadFile {
    // contains the image index
    Image(usize, Vec<u8>),
}

#[derive(Debug, Hiarc)]
pub struct CDatafileWrapper {
    pub data_file: CDatafile,
    pub name: String,

    versions: Vec<CMapItemVersion>,
    infos: Vec<MapInfo>,
    pub images: Vec<MapImage>,
    envelopes: Vec<(String, CMapItemEnvelope)>,
    groups: Vec<CMapItemGroup>,
    pub layers: Vec<MapLayer>,
    env_points: Vec<Vec<CEnvPoint>>,
    pub sounds: Vec<MapSound>,

    game_layer_index: usize,
    game_group_index: usize,
    //m_pGameGroupEx: *mut CMapItemGroupEx,
    tele_layer_index: usize,
    speed_layer_index: usize,
    front_layer_index: usize,
    switch_layer_index: usize,
    tune_layer_index: usize,

    // files to read, if the user of this object
    // wants to have support for images etc.
    pub read_files: LinkedHashMap<String, ReadFile>,
}

#[derive(Default)]
pub struct MapFileOpenOptions {
    pub do_benchmark: bool,
    pub dont_load_map_item: [bool; MapItemTypes::Count as usize],
}

#[derive(Default)]
pub struct MapFileLayersReadOptions {
    pub do_benchmark: bool,
    pub dont_load_design_layers: bool,
}

#[derive(Default)]
pub struct MapFileImageReadOptions {
    pub do_benchmark: bool,
}

#[derive(Default)]
pub struct MapFileSoundReadOptions {
    pub do_benchmark: bool,
}

#[derive(Debug)]
pub struct LegacyMapToNewRes {
    pub buf: Vec<u8>,
    pub ty: String,
    pub name: String,
}

#[derive(Debug)]
pub struct LegacyMapToNewResources {
    /// blake3 hash
    pub images: HashMap<Hash, LegacyMapToNewRes>,
    /// blake3 hash
    pub sounds: HashMap<Hash, LegacyMapToNewRes>,
}

#[derive(Debug)]
pub struct LegacyMapToNewOutput {
    pub map: Map,
    pub resources: LegacyMapToNewResources,
}

impl Default for CDatafileWrapper {
    fn default() -> Self {
        Self::new()
    }
}

impl CDatafileWrapper {
    pub fn new() -> CDatafileWrapper {
        CDatafileWrapper {
            data_file: Default::default(),
            name: String::new(),
            versions: Vec::new(),
            infos: Vec::new(),
            images: Vec::new(),
            envelopes: Vec::new(),
            groups: Vec::new(),
            layers: Vec::new(),
            env_points: Vec::new(),
            sounds: Vec::new(),

            game_layer_index: usize::MAX,
            game_group_index: usize::MAX,
            //m_pGameGroupEx: std::ptr::null_mut(),
            tele_layer_index: usize::MAX,
            speed_layer_index: usize::MAX,
            front_layer_index: usize::MAX,
            switch_layer_index: usize::MAX,
            tune_layer_index: usize::MAX,

            read_files: Default::default(),
        }
    }

    /// Returns a tuple of various information about the file:
    /// - the a slice of the data containers of the file vec
    pub fn open<'a>(
        &mut self,
        data_param: &'a Vec<u8>,
        file_name: &str,
        thread_pool: &rayon::ThreadPool,
        options: &MapFileOpenOptions,
    ) -> anyhow::Result<&'a [u8]> {
        let do_benchmark = options.do_benchmark;
        self.name = file_name.to_string();
        //log_trace("datafile", "loading. filename='%s'", pFilename);

        // take the CRC of the file and store it
        /*unsigned Crc = 0;
        SHA256_DIGEST Sha256;
        {
            enum
            {
                BUFFER_SIZE = 64 * 1024
            };

            SHA256_CTX Sha256Ctxt;
            sha256_init(&Sha256Ctxt);
            unsigned char aBuffer[BUFFER_SIZE];

            while(true)
            {
                unsigned Bytes = io_read(File, aBuffer, BUFFER_SIZE);
                if(Bytes == 0)
                    break;
                Crc = crc32(Crc, aBuffer, Bytes);
                sha256_update(&Sha256Ctxt, aBuffer, Bytes);
            }
            Sha256 = sha256_finish(&Sha256Ctxt);

            io_seek(File, 0, IOSEEK_START);
        }*/
        let mut data_file: CDatafile = CDatafile::default();
        let mut read_data = data_param.as_slice();

        let mut items: Vec<CDatafileItemAndData> = Vec::new();
        let data_start: &[u8];

        let benchmark = Benchmark::new(do_benchmark);
        if !{
            // TODO: change this header
            let header_size = std::mem::size_of::<CDatafileHeader>();
            if header_size <= read_data.len() {
                data_file.header = CDatafileHeader::read_from_slice(read_data);
                (_, read_data) = read_data.split_at(header_size);
            } else {
                return Err(anyhow!("size is smaller than the header size"));
            }
            if (data_file.header.id[0] != 'A' as i8
                || data_file.header.id[1] != 'T' as i8
                || data_file.header.id[2] != 'A' as i8
                || data_file.header.id[3] != 'D' as i8)
                && (data_file.header.id[0] != 'D' as i8
                    || data_file.header.id[1] != 'A' as i8
                    || data_file.header.id[2] != 'T' as i8
                    || data_file.header.id[3] != 'A' as i8)
            {
                /*dbg_msg(
                    "datafile",
                    "wrong signature. %x %x %x %x",
                    Header.m_aID[0],
                    Header.m_aID[1],
                    Header.m_aID[2],
                    Header.m_aID[3],
                );*/
                return Err(anyhow!("header is wrong"));
            }

            // data_file.m_Header.m_Version != 3 &&
            if data_file.header.version != 4 {
                // TODO dbg_msg("datafile", "wrong version. version=%x", Header.m_Version);

                return Err(anyhow!(
                    "file versions other than 4 are currently not supported"
                ));
            }

            // read in the rest except the data
            let mut read_size_total: u32 = 0;
            read_size_total +=
                data_file.header.num_item_types * std::mem::size_of::<CDatafileItemType>() as u32;
            read_size_total += (data_file.header.num_items + data_file.header.num_raw_data)
                * std::mem::size_of::<u32>() as u32;
            if data_file.header.version == 4 {
                read_size_total +=
                    data_file.header.num_raw_data * std::mem::size_of::<u32>() as u32;
                // v4 has uncompressed data sizes as well
            }
            read_size_total += data_file.header.item_size;

            /*TODO:(*pTmpDataFile).m_File = File;
            (*pTmpDataFile).m_Sha256 = Sha256;
            (*pTmpDataFile).m_Crc = Crc;*/

            if read_data.len() < (read_size_total as usize) {
                //TODO dbg_msg("datafile", "couldn't load the whole thing, wanted=%d got=%d", Size, ReadSize);

                return Err(anyhow!("file is too small, can't read all items"));
            }

            /*
            if(DEBUG)
            {
                dbg_msg("datafile", "allocsize=%d", AllocSize);
                dbg_msg("datafile", "readsize=%d", ReadSize);
                dbg_msg("datafile", "swaplen=%d", Header.m_Swaplen);
                dbg_msg("datafile", "item_size=%d", (*self.m_pDataFile).m_Header.m_ItemSize);
            } */

            let size_of_item = size_of::<CDatafileItemType>();
            for _i in 0..data_file.header.num_item_types {
                data_file
                    .info
                    .item_types
                    .push(CDatafileItemType::read_from_slice(read_data));
                (_, read_data) = read_data.split_at(size_of_item);
            }

            let size_of_item = size_of::<i32>();
            for _i in 0..data_file.header.num_items {
                data_file.info.item_offsets.push(read_i32_le(read_data));
                (_, read_data) = read_data.split_at(size_of_item);
            }

            let size_of_item = size_of::<i32>();
            for _i in 0..data_file.header.num_raw_data {
                data_file.info.data_offsets.push(read_i32_le(read_data));
                (_, read_data) = read_data.split_at(size_of_item);
            }

            let size_of_item = size_of::<i32>();
            for _i in 0..data_file.header.num_raw_data {
                data_file.info.data_sizes.push(read_i32_le(read_data));
                (_, read_data) = read_data.split_at(size_of_item);
            }

            if data_file.header.version == 4 {
                let (itemsstart, rest) = read_data.split_at(data_file.header.item_size as usize);
                read_data = rest;

                for i in 0..data_file.header.num_items as usize {
                    let offset = data_file.info.item_offsets[i] as usize;
                    assert!(
                        itemsstart.len() >= offset,
                        "read data too small: {}/{} - {} - {} - {} - {:?}",
                        itemsstart.len(),
                        read_data.len(),
                        data_file.header.item_size,
                        offset,
                        data_file.header.num_items,
                        data_file.info.item_offsets
                    );
                    let (_, item_data) = itemsstart.split_at(offset);

                    let item_size =
                        Self::get_item_size(&data_file.header, &data_file.info, i as i32) as usize;
                    assert!(
                        item_size <= item_data.len(),
                        "item data is too small {} vs. {}, {} len:{}-{:?}, {}",
                        item_size,
                        item_data.len(),
                        i,
                        data_file.info.item_offsets.len(),
                        data_file.info.item_offsets,
                        data_file.header.item_size
                    );
                    items.push(CDatafileItemAndData::read_from_slice(item_data, item_size));
                }
            } else {
                panic!("not supported");
            }

            let (datas, _) = read_data.split_at(data_file.header.data_size as usize);
            data_start = datas;

            true
        } {
            return Err(anyhow!("File could not be opened"));
        }
        benchmark.bench("loading the map header, items and data");

        // read items
        thread_pool.install(|| {
            join_all!(
                || {
                    if !options.dont_load_map_item[MapItemTypes::Version as usize] {
                        // MAPITEMTYPE_VERSION
                        let mut start = i32::default();
                        let mut num = i32::default();
                        let item_size = size_of::<CMapItemVersion>();
                        Self::get_type(
                            &data_file,
                            MapItemTypes::Version as i32,
                            &mut start,
                            &mut num,
                        );
                        for i in 0..num as usize {
                            let data = &items[start as usize + i].data[0..item_size];
                            self.versions.push(CMapItemVersion::read_from_slice(data))
                        }
                        benchmark.bench_multi("loading the map version");
                    }
                },
                || {
                    if !options.dont_load_map_item[MapItemTypes::Info as usize] {
                        // MAPITEMTYPE_INFO
                        let mut start = i32::default();
                        let mut num = i32::default();
                        let item_size = size_of::<CMapItemInfoSettings>();
                        Self::get_type(&data_file, MapItemTypes::Info as i32, &mut start, &mut num);
                        for i in 0..num as usize {
                            let data = &items[start as usize + i].data;
                            let item_size = if data.len() >= item_size {
                                item_size
                            } else {
                                size_of::<CMapItemInfo>()
                            };
                            let data = &data[0..item_size];
                            let def = CMapItemInfoSettings::read_from_slice(data);

                            self.infos.push(MapInfo {
                                author: Self::read_char_array::<32>(
                                    &data_file,
                                    def.info.author,
                                    data_start,
                                ),
                                map_version: Self::read_char_array::<16>(
                                    &data_file,
                                    def.info.map_version,
                                    data_start,
                                ),
                                credits: Self::read_char_array::<128>(
                                    &data_file,
                                    def.info.credits,
                                    data_start,
                                ),
                                license: Self::read_char_array::<32>(
                                    &data_file,
                                    def.info.license,
                                    data_start,
                                ),
                                settings: Self::read_char_array_array(
                                    &data_file,
                                    def.settings,
                                    data_start,
                                ),
                                def,
                            });
                        }
                        benchmark.bench_multi("loading the map info");
                    }
                },
                || {
                    if !options.dont_load_map_item[MapItemTypes::Image as usize] {
                        //MAPITEMTYPE_IMAGE
                        let mut start = i32::default();
                        let mut num = i32::default();
                        let item_size = size_of::<CMapItemImage>();
                        Self::get_type(
                            &data_file,
                            MapItemTypes::Image as i32,
                            &mut start,
                            &mut num,
                        );
                        self.images.resize_with(num as usize, MapImage::default);
                        self.images.par_iter_mut().enumerate().for_each(|(i, img)| {
                            let data = &items[start as usize + i].data[0..item_size];
                            img.item_data = CMapItemImage::read_from_slice(data);

                            // read the image name
                            img.img_name =
                                Self::read_string(&data_file, img.item_data.image_name, data_start);
                        });
                        self.images.iter().enumerate().for_each(|(index, img)| {
                            if img.item_data.external != 0 {
                                // add the external image to the read files
                                let r = self.read_files.insert(
                                    format!("legacy/mapres/{}.png", img.img_name),
                                    ReadFile::Image(index, Vec::new()),
                                );
                                assert!(
                                    r.is_none(),
                                    "image with that name was already found {}",
                                    img.img_name
                                )
                            }
                        });
                        benchmark.bench_multi("loading the map images");
                    }
                },
                || {
                    if !options.dont_load_map_item[MapItemTypes::Envelope as usize] {
                        //MAPITEMTYPE_ENVELOPE
                        let mut start = i32::default();
                        let mut num = i32::default();
                        let item_size_full_size = size_of::<CMapItemEnvelope>();
                        let item_size_without_sync = CMapItemEnvelope::size_without_sync();
                        Self::get_type(
                            &data_file,
                            MapItemTypes::Envelope as i32,
                            &mut start,
                            &mut num,
                        );
                        for i in 0..num as usize {
                            let data_len = items[start as usize + i].data.len();
                            let data = if data_len >= item_size_full_size {
                                &items[start as usize + i].data[0..item_size_full_size]
                            } else {
                                &items[start as usize + i].data[0..item_size_without_sync]
                            };
                            let env = CMapItemEnvelope::read_from_slice(data);
                            let env_name = if env.name[0] != -1 {
                                Self::read_str_from_ints(&env.name)
                            } else {
                                String::new()
                            };
                            self.envelopes.push((env_name, env));
                        }
                        benchmark.bench_multi("loading the map envelopes");
                    }
                },
                || {
                    if !options.dont_load_map_item[MapItemTypes::Group as usize] {
                        //MAPITEMTYPE_GROUP
                        let mut start = i32::default();
                        let mut num = i32::default();
                        let item_size_full = size_of::<CMapItemGroup>();
                        let item_size_no_name = CMapItemGroup::size_of_without_name();
                        Self::get_type(
                            &data_file,
                            MapItemTypes::Group as i32,
                            &mut start,
                            &mut num,
                        );
                        for i in 0..num as usize {
                            let data_len = items[start as usize + i].data.len();
                            let data = if data_len >= item_size_full {
                                &items[start as usize + i].data[0..item_size_full]
                            } else {
                                &items[start as usize + i].data[0..item_size_no_name]
                            };
                            self.groups.push(CMapItemGroup::read_from_slice(data))
                        }
                        benchmark.bench_multi("loading the map groups");
                    }
                },
                || {
                    if !options.dont_load_map_item[MapItemTypes::Layer as usize] {
                        //MAPITEMTYPE_LAYER
                        let mut start = i32::default();
                        let mut num = i32::default();
                        let item_size = size_of::<CMapItemLayer>();
                        Self::get_type(
                            &data_file,
                            MapItemTypes::Layer as i32,
                            &mut start,
                            &mut num,
                        );
                        self.layers = vec![MapLayer::Unknown(Default::default()); num as usize];
                        self.layers
                            .par_iter_mut()
                            .enumerate()
                            .for_each(|(i, map_layer)| {
                                let data = &items[start as usize + i].data[0..item_size];
                                let layer = CMapItemLayer::read_from_slice(data);

                                if layer.item_layer == MapLayerTypes::Tiles as i32 {
                                    let item_size_non_ddrace =
                                        CMapItemLayerTilemap::size_of_without_ddrace();
                                    let item_size_non_ddrace_no_name =
                                        CMapItemLayerTilemap::size_of_without_name();
                                    let item_size_full = size_of::<CMapItemLayerTilemap>();
                                    let data_len = items[start as usize + i].data.len();
                                    let data = if data_len >= item_size_full {
                                        &items[start as usize + i].data[0..item_size_full]
                                    } else if data_len >= item_size_non_ddrace {
                                        &items[start as usize + i].data[0..item_size_non_ddrace]
                                    } else {
                                        &items[start as usize + i].data
                                            [0..item_size_non_ddrace_no_name]
                                    };
                                    let tile_layer = CMapItemLayerTilemap::read_from_slice(data);

                                    let tile_layer_impl = MapTileLayerDetail::Tile(Vec::new());
                                    *map_layer = MapLayer::Tile(MapLayerTile(
                                        tile_layer,
                                        tile_layer_impl,
                                        Vec::new(),
                                    ));
                                } else if layer.item_layer == MapLayerTypes::Quads as i32 {
                                    let item_size_no_name =
                                        CMapItemLayerQuads::size_of_without_name();
                                    let item_size_full = size_of::<CMapItemLayerQuads>();
                                    let data_len = items[start as usize + i].data.len();
                                    let data = if data_len >= item_size_full {
                                        &items[start as usize + i].data[0..item_size_full]
                                    } else {
                                        &items[start as usize + i].data[0..item_size_no_name]
                                    };
                                    let quad_layer_info = CMapItemLayerQuads::read_from_slice(data);

                                    *map_layer =
                                        MapLayer::Quads(MapLayerQuad(quad_layer_info, Vec::new()));
                                } else if layer.item_layer == MapLayerTypes::Sounds as i32 {
                                    let item_size_full = size_of::<CMapItemLayerSounds>();
                                    let item_size_no_name =
                                        CMapItemLayerSounds::size_of_without_name();
                                    let data_len = items[start as usize + i].data.len();
                                    let data = if data_len >= item_size_full {
                                        &items[start as usize + i].data[0..item_size_full]
                                    } else {
                                        &items[start as usize + i].data[0..item_size_no_name]
                                    };
                                    let sound_layer = CMapItemLayerSounds::read_from_slice(data);
                                    *map_layer = MapLayer::Sound {
                                        def: sound_layer,
                                        sounds: Vec::new(),
                                    };
                                } else {
                                    *map_layer = MapLayer::Unknown(layer);
                                }
                            });
                        benchmark.bench_multi("loading the map layers");
                    }
                },
                || {
                    if !options.dont_load_map_item[MapItemTypes::Envpoints as usize] {
                        //MAPITEMTYPE_ENVPOINTS
                        let mut start = i32::default();
                        let mut num = i32::default();
                        let item_size = size_of::<CEnvPoint>();
                        Self::get_type(
                            &data_file,
                            MapItemTypes::Envpoints as i32,
                            &mut start,
                            &mut num,
                        );
                        for i in 0..num as usize {
                            let item_count = items[start as usize + i].data.len() / item_size;
                            let mut env_points: Vec<CEnvPoint> = Vec::new();
                            for n in 0..item_count {
                                let item_off = n * item_size;
                                let data =
                                    &items[start as usize + i].data[item_off..item_off + item_size];
                                env_points.push(CEnvPoint::read_from_slice(data));
                            }
                            self.env_points.push(env_points);
                        }
                        benchmark.bench_multi("loading the map env-points");
                    }
                },
                || {
                    if !options.dont_load_map_item[MapItemTypes::Sound as usize] {
                        //MAPITEMTYPE_SOUND
                        let mut start = i32::default();
                        let mut num = i32::default();
                        let item_size = size_of::<CMapItemSound>();
                        Self::get_type(
                            &data_file,
                            MapItemTypes::Sound as i32,
                            &mut start,
                            &mut num,
                        );
                        for i in 0..num as usize {
                            let data = &items[start as usize + i].data[0..item_size];
                            let sound = CMapItemSound::read_from_slice(data);
                            let sound_name =
                                Self::read_string(&data_file, sound.sound_name, data_start);
                            self.sounds.push(MapSound {
                                name: sound_name,
                                def: sound,
                                data: None,
                            });
                        }
                        benchmark.bench_multi("loading the map sounds");
                    }
                }
            );
        });

        self.data_file = data_file; //pTmpDataFile;

        Ok(data_start)
    }

    pub fn read_map_layers(
        data_file: &CDatafile,
        layers: &mut Vec<MapLayer>,
        data_start: &[u8],
        options: &MapFileLayersReadOptions,
    ) {
        let benchmark = Benchmark::new(options.do_benchmark);

        layers
            .par_iter_mut()
            .enumerate()
            .for_each(|(_i, map_layer)| {
                if let MapLayer::Tile(tile_layer) = map_layer {
                    let mut tiles_data_index = tile_layer.0.data;

                    let mut is_entity_layer = false;

                    if (tile_layer.0.flags & TilesLayerFlag::Game as i32) != 0 {
                        is_entity_layer = true;
                    }

                    if (tile_layer.0.flags & TilesLayerFlag::Front as i32) != 0 {
                        is_entity_layer = true;
                        tiles_data_index = tile_layer.0.front;
                    }

                    let mut tile_layer_impl = MapTileLayerDetail::Tile(Vec::new());
                    if (tile_layer.0.flags & TilesLayerFlag::Tele as i32) != 0 {
                        tile_layer_impl = MapTileLayerDetail::Tele(Self::read_tiles(
                            data_file,
                            tile_layer.0.tele,
                            tile_layer.0.width as usize,
                            tile_layer.0.height as usize,
                            data_start,
                            "tele",
                        ));
                        is_entity_layer = true;
                    } else if (tile_layer.0.flags & TilesLayerFlag::Speedup as i32) != 0 {
                        tile_layer_impl = MapTileLayerDetail::Speedup(Self::read_tiles(
                            data_file,
                            tile_layer.0.speedup,
                            tile_layer.0.width as usize,
                            tile_layer.0.height as usize,
                            data_start,
                            "speedup",
                        ));
                        is_entity_layer = true;
                    } else if (tile_layer.0.flags & TilesLayerFlag::Switch as i32) != 0 {
                        tile_layer_impl = MapTileLayerDetail::Switch(Self::read_tiles(
                            data_file,
                            tile_layer.0.switch,
                            tile_layer.0.width as usize,
                            tile_layer.0.height as usize,
                            data_start,
                            "switch",
                        ));
                        is_entity_layer = true;
                    } else if (tile_layer.0.flags & TilesLayerFlag::Tune as i32) != 0 {
                        tile_layer_impl = MapTileLayerDetail::Tune(Self::read_tiles(
                            data_file,
                            tile_layer.0.tune,
                            tile_layer.0.width as usize,
                            tile_layer.0.height as usize,
                            data_start,
                            "tune",
                        ));
                        is_entity_layer = true;
                    }

                    let tiles = if is_entity_layer || !options.dont_load_design_layers {
                        Self::read_tiles(
                            data_file,
                            tiles_data_index,
                            tile_layer.0.width as usize,
                            tile_layer.0.height as usize,
                            data_start,
                            "physics",
                        )
                    } else {
                        Vec::new()
                    };

                    *map_layer =
                        MapLayer::Tile(MapLayerTile(tile_layer.0.clone(), tile_layer_impl, tiles));
                } else if let MapLayer::Quads(quad_layer) = map_layer {
                    let quads = Self::read_quads(
                        data_file,
                        quad_layer.0.data,
                        quad_layer.0.num_quads as usize,
                        data_start,
                    );
                    *map_layer = MapLayer::Quads(MapLayerQuad(quad_layer.0.clone(), quads));
                } else if let MapLayer::Sound { def, .. } = map_layer {
                    let sounds = Self::read_sounds(
                        data_file,
                        def.data,
                        def.num_sources as usize,
                        data_start,
                    );
                    *map_layer = MapLayer::Sound { def: *def, sounds };
                }
            });

        benchmark.bench("loading the map layers tiles");
    }

    pub fn read_image_data(
        data_file: &CDatafile,
        images: &[MapImage],
        data_start: &[u8],
        options: &MapFileImageReadOptions,
    ) -> Vec<Option<(u32, u32, Vec<u8>)>> {
        let mut res: Vec<Option<(u32, u32, Vec<u8>)>> = Vec::new();
        res.resize(images.len(), Default::default());

        let benchmark = Benchmark::new(options.do_benchmark);

        res.par_iter_mut().enumerate().for_each(|(i, img)| {
            let img_data = &images[i];
            if img_data.item_data.external == 0 {
                // read the image data
                *img = Some((
                    img_data.item_data.width as u32,
                    img_data.item_data.height as u32,
                    Self::decompress_data(
                        data_file,
                        img_data.item_data.image_data as usize,
                        data_start,
                    ),
                ));
            }
        });

        benchmark.bench("loading the map internal images");
        res
    }

    pub fn read_sound_data(
        data_file: &CDatafile,
        sounds: &[MapSound],
        data_start: &[u8],
        options: &MapFileSoundReadOptions,
    ) -> Vec<Option<(u32, Vec<u8>)>> {
        let mut res: Vec<Option<(u32, Vec<u8>)>> = Vec::new();
        res.resize(sounds.len(), Default::default());

        let benchmark = Benchmark::new(options.do_benchmark);

        res.par_iter_mut().enumerate().for_each(|(i, img)| {
            let snd_data = &sounds[i].def;
            if snd_data.external == 0 {
                // read the image data
                *img = Some((
                    snd_data.sound_data_size as u32,
                    Self::decompress_data(data_file, snd_data.sound_data as usize, data_start),
                ));
            }
        });

        benchmark.bench("loading the map internal sounds");
        res
    }

    fn read_tiles<T>(
        data_file: &CDatafile,
        data_index: i32,
        width: usize,
        height: usize,
        data_start: &[u8],
        layer_name: &str,
    ) -> Vec<T>
    where
        T: ReadFromSliceWriteToVec + Default + Clone + Send + Sync,
    {
        if data_index != -1 {
            let tile_size = size_of::<T>();
            let uncompressed_data =
                Self::decompress_data(data_file, data_index as usize, data_start);
            let tiles_sliced = uncompressed_data.as_slice();
            let mut tiles = vec![Default::default(); width * height];
            assert!(
                tiles_sliced.len() >= width * height * tile_size,
                "read layer data too small for {layer_name}"
            );
            tiles
                .par_chunks_exact_mut(width)
                .enumerate()
                .for_each(|(y, tiles_width)| {
                    for (x, tiles_width) in tiles_width.iter_mut().enumerate() {
                        let tile_index = y * width + x;
                        let tile_sliced = &tiles_sliced
                            [(tile_index * tile_size)..(tile_index * tile_size) + tile_size];

                        *tiles_width = T::read_from_slice(tile_sliced);
                    }
                });
            return tiles;
        }
        Vec::new()
    }

    fn read_quads(
        data_file: &CDatafile,
        data_index: i32,
        num_quads: usize,
        data_start: &[u8],
    ) -> Vec<CQuad> {
        if data_index != -1 {
            let quad_size = size_of::<CQuad>();
            let uncompressed_data =
                Self::decompress_data(data_file, data_index as usize, data_start);
            let quads_sliced = uncompressed_data.as_slice();
            let mut quads = vec![Default::default(); num_quads];
            quads.par_iter_mut().enumerate().for_each(|(index, quad)| {
                let quad_sliced = &quads_sliced[index * quad_size..(index * quad_size) + quad_size];
                *quad = CQuad::read_from_slice(quad_sliced);
            });
            return quads;
        }
        Vec::new()
    }

    fn read_sounds(
        data_file: &CDatafile,
        data_index: i32,
        num_sounds: usize,
        data_start: &[u8],
    ) -> Vec<CSoundSource> {
        if data_index != -1 {
            let sound_size = size_of::<CSoundSource>();
            let uncompressed_data =
                Self::decompress_data(data_file, data_index as usize, data_start);
            let sounds_sliced = uncompressed_data.as_slice();
            let mut sounds = vec![Default::default(); num_sounds];
            sounds
                .par_iter_mut()
                .enumerate()
                .for_each(|(index, sound)| {
                    let sound_sliced =
                        &sounds_sliced[index * sound_size..(index * sound_size) + sound_size];
                    *sound = CSoundSource::read_from_slice(sound_sliced);
                });
            return sounds;
        }
        Vec::new()
    }

    fn decompress_data(data_file: &CDatafile, index: usize, data_start: &[u8]) -> Vec<u8> {
        // v4 has compressed data
        let uncompressed_size = data_file.info.data_sizes[index];

        // read the compressed data
        let data_split = Self::get_data_slice(data_file, index, data_start);
        let tmp = data_split;

        // decompress the data, TODO: check for errors
        let mut d = ZlibDecoder::new(tmp);

        let mut data = Vec::with_capacity(uncompressed_size as usize);
        d.read_to_end(&mut data).unwrap();
        data
    }

    fn compress_data(data: &[u8]) -> Vec<u8> {
        let mut res: Vec<u8> = Default::default();

        let mut e = ZlibEncoder::new(&mut res, Compression::default());

        e.write_all(data).unwrap();
        drop(e);

        res
    }

    fn get_internal_item_type(external_type: i32) -> i32 {
        if external_type < UuidOffset::Uuid as i32 {
            return external_type;
        }
        /* TODO! CUuid Uuid = g_UuidManager.GetUuid(ExternalType);
        Start, Num: i32,
        GetType(ITEMTYPE_EX, &Start, &Num);
        for(i = Start; i < Start + Num: i32, i++)
        {
            if(GetItemSize(i) < (int)sizeof(CItemEx))
            {
                continue;
            }
            ID: i32,
            if(Uuid == ((const CItemEx *)GetItem(i, 0, &ID))->ToUuid())
            {
                return ID;
            }
        }*/
        -1
    }

    fn get_type(data_file: &CDatafile, item_type: i32, start_index: &mut i32, num: &mut i32) {
        *start_index = 0;
        *num = 0;

        let real_type = Self::get_internal_item_type(item_type);
        for i in 0..data_file.header.num_item_types as usize {
            if data_file.info.item_types[i].item_type == real_type {
                *start_index = data_file.info.item_types[i].start;
                *num = data_file.info.item_types[i].num;
                return;
            }
        }
    }

    pub fn num_groups(&self) -> i32 {
        self.groups.len() as i32
    }

    fn get_item_size(header: &CDatafileHeader, info: &CDatafileInfo, index: i32) -> i32 {
        if index == header.num_items as i32 - 1 {
            return header.item_size as i32
                - info.item_offsets[index as usize]
                - std::mem::size_of::<CDatafileItem>() as i32;
        }
        info.item_offsets[index as usize + 1]
            - info.item_offsets[index as usize]
            - std::mem::size_of::<CDatafileItem>() as i32
    }

    fn get_data_slice<'a>(data_file: &CDatafile, index: usize, data_start: &'a [u8]) -> &'a [u8] {
        let data_start_off = data_file.info.data_offsets[index] as usize;
        let (_, offset_data) = data_start.split_at(data_start_off);
        let (data_split, _) =
            offset_data
                .split_at(Self::get_data_size(&data_file.header, &data_file.info, index) as usize);
        data_split
    }

    fn get_data_size(header: &CDatafileHeader, info: &CDatafileInfo, index: usize) -> i32 {
        if index as i32 == header.num_raw_data as i32 - 1 {
            return header.data_size as i32 - info.data_offsets[index];
        }
        info.data_offsets[index + 1] - info.data_offsets[index]
    }

    fn init_tilemap_skip(&mut self, thread_pool: &rayon::ThreadPool) {
        for g in 0..self.num_groups() as usize {
            let group = &self.groups[g];
            for l in 0..group.num_layers as usize {
                let layer = &mut self.layers[group.start_layer as usize + l];

                if let MapLayer::Tile(MapLayerTile(tile_layer, _, tiles)) = layer {
                    let tile_list = tiles;
                    thread_pool.install(|| {
                        tile_list
                            .par_chunks_mut(tile_layer.width as usize)
                            .for_each(|tiles_chunk| {
                                let mut x = 0;
                                while x < tile_layer.width {
                                    let mut skipped_x: i32 = 1;
                                    while x + skipped_x < tile_layer.width && skipped_x < 255 {
                                        if tiles_chunk[x as usize + skipped_x as usize].index > 0 {
                                            break;
                                        }

                                        skipped_x += 1;
                                    }

                                    tiles_chunk[x as usize].skip = (skipped_x - 1) as u8;
                                    x += skipped_x;
                                }
                            });
                    });
                }
            }
        }
    }

    pub fn init_layers(&mut self, thread_pool: &rayon::ThreadPool) {
        for g in 0..self.num_groups() as usize {
            let group = &mut self.groups[g];
            //let pGroupEx = self.GetGroupExUnsafe(g);
            for l in 0..group.num_layers as usize {
                let layer_index = group.start_layer as usize + l;
                let layer = &mut self.layers[layer_index];

                if let MapLayer::Tile(MapLayerTile(tile_layer, _, _)) = layer {
                    if (tile_layer.flags & TilesLayerFlag::Game as i32) != 0 {
                        self.game_layer_index = layer_index;
                        self.game_group_index = g;
                        //self.m_pGameGroupEx = pGroupEx;

                        // make sure the game group has standard settings
                        group.offset_x = 0;
                        group.offset_y = 0;
                        group.parallax_x = 100;
                        group.parallax_y = 100;

                        if group.version >= 2 {
                            group.use_clipping = 0;
                            group.clip_x = 0;
                            group.clip_y = 0;
                            group.clip_w = 0;
                            group.clip_h = 0;
                        }

                        /*if !pGroupEx.is_null() {
                            (*pGroupEx).m_ParallaxZoom = 100;
                        }*/

                        //break;
                    }
                    if (tile_layer.flags & TilesLayerFlag::Tele as i32) != 0 {
                        self.tele_layer_index = layer_index;
                    }
                    if (tile_layer.flags & TilesLayerFlag::Speedup as i32) != 0 {
                        self.speed_layer_index = layer_index;
                    }
                    if (tile_layer.flags & TilesLayerFlag::Front as i32) != 0 {
                        self.front_layer_index = layer_index;
                    }
                    if (tile_layer.flags & TilesLayerFlag::Switch as i32) != 0 {
                        self.switch_layer_index = layer_index;
                    }
                    if (tile_layer.flags & TilesLayerFlag::Tune as i32) != 0 {
                        self.tune_layer_index = layer_index;
                    }
                }
            }
        }

        self.init_tilemap_skip(thread_pool);
    }

    fn read_string(data_file: &CDatafile, index: i32, data_start: &[u8]) -> String {
        let data_name = Self::decompress_data(data_file, index as usize, data_start);
        let name_cstr = CStr::from_bytes_with_nul(data_name.as_slice()).unwrap_or_else(|_| {
            panic!("data name was not valid utf8 with null-termination {data_name:?}")
        });
        name_cstr.to_str().unwrap().to_string()
    }

    fn read_char_array<const N: usize>(
        data_file: &CDatafile,
        index: i32,
        data_start: &[u8],
    ) -> String {
        if index == -1 {
            return "".into();
        }
        let mut data_name = Self::decompress_data(data_file, index as usize, data_start);
        data_name.truncate(N);

        let c_str = std::ffi::CStr::from_bytes_until_nul(&data_name).unwrap();
        c_str.to_string_lossy().to_string()
    }

    fn read_char_array_array(data_file: &CDatafile, index: i32, data_start: &[u8]) -> Vec<String> {
        if index == -1 {
            return Vec::new();
        }
        let data_name = Self::decompress_data(data_file, index as usize, data_start);
        let mut offset = 0;
        let mut res = Vec::new();
        while offset < data_name.len() {
            let c_str = std::ffi::CStr::from_bytes_until_nul(&data_name[offset..]).unwrap();
            offset += c_str.to_bytes_with_nul().len();
            res.push(c_str.to_string_lossy().to_string());
        }
        res
    }

    pub fn is_game_layer(&self, layer_index: usize) -> bool {
        self.game_layer_index == layer_index
    }

    pub fn is_tele_layer(&self, layer_index: usize) -> bool {
        self.tele_layer_index == layer_index
    }

    pub fn is_speedup_layer(&self, layer_index: usize) -> bool {
        self.speed_layer_index == layer_index
    }

    pub fn is_front_layer(&self, layer_index: usize) -> bool {
        self.front_layer_index == layer_index
    }

    pub fn is_switch_layer(&self, layer_index: usize) -> bool {
        self.switch_layer_index == layer_index
    }

    pub fn is_tune_layer(&self, layer_index: usize) -> bool {
        self.tune_layer_index == layer_index
    }

    pub fn get_game_layer(&self) -> &MapLayerTile {
        let layer = &self.layers[self.game_layer_index];
        if let MapLayer::Tile(layer) = layer {
            return layer;
        }
        panic!("layer does not exists");
    }

    pub fn get_game_group(&self) -> &CMapItemGroup {
        self.get_group(self.game_group_index)
    }

    pub fn get_layer(&self, index: usize) -> &MapLayer {
        &self.layers[index]
    }

    pub fn get_group(&self, index: usize) -> &CMapItemGroup {
        &self.groups[index]
    }

    pub fn env_count(&self) -> usize {
        self.envelopes.len()
    }

    pub fn get_env(&self, index: usize) -> &CMapItemEnvelope {
        &self.envelopes[index].1
    }

    pub fn env_point_count(&self) -> usize {
        self.env_points.len()
    }

    pub fn get_env_points(&self) -> &[Vec<CEnvPoint>] {
        self.env_points.as_slice()
    }

    fn str_to_ints(int_arr: &mut [i32], c_str: &[u8]) {
        let mut index = 0;
        let mut num_int = 0;
        while num_int < int_arr.len() {
            let mut conv_char: [u8; 4] = [0, 0, 0, 0];
            for conv_char in conv_char.iter_mut() {
                if c_str.is_empty() || index >= c_str.len() || c_str[index] == 0 {
                    break;
                }
                *conv_char = c_str[index];
                index += 1;
            }
            int_arr[num_int] = (((conv_char[0] as i32 + 128) & 0xff) << 24)
                | (((conv_char[1] as i32 + 128) & 0xff) << 16)
                | (((conv_char[2] as i32 + 128) & 0xff) << 8)
                | ((conv_char[3] as i32 + 128) & 0xff);
            num_int += 1;
        }

        // null terminate
        int_arr[int_arr.len() - 1] &= -256i32;
    }

    fn ints_to_str(int_arr: &[i32], c_str: &mut [u8]) {
        let mut num_int = 0;
        let mut index = 0;
        while num_int < int_arr.len() {
            let bytes = int_arr[num_int].to_be_bytes();
            c_str[index] = bytes[0].wrapping_sub(128);
            c_str[index + 1] = bytes[1].wrapping_sub(128);
            c_str[index + 2] = bytes[2].wrapping_sub(128);
            c_str[index + 3] = bytes[3].wrapping_sub(128);
            index += 4;
            num_int += 1;
        }

        // null terminate
        c_str[index - 1] = 0;
    }

    fn read_str_from_ints(inp: &[i32]) -> String {
        let mut res: [u8; 32] = Default::default();

        Self::ints_to_str(inp, &mut res);

        CStr::from_bytes_until_nul(&res)
            .map_err(|err| anyhow!("reading {inp:?} - {res:?} => err: {err}"))
            .unwrap()
            .to_string_lossy()
            .to_string()
    }

    /// images are external images
    pub fn into_map(
        self,
        thread_pool: &rayon::ThreadPool,
        images: &[Vec<u8>],
        png_validation: PngValidatorOptions,
        dilate: bool,
    ) -> anyhow::Result<LegacyMapToNewOutput> {
        let mut image_resources: HashMap<Hash, LegacyMapToNewRes> = Default::default();
        let mut sound_resources: HashMap<Hash, LegacyMapToNewRes> = Default::default();

        let mut map = Map {
            animations: Animations {
                pos: Default::default(),
                color: Default::default(),
                sound: Default::default(),
            },
            groups: mapnew::groups::MapGroups {
                physics: mapnew::groups::MapGroupPhysics {
                    attr: mapnew::groups::MapGroupPhysicsAttr {
                        width: NonZeroU16MinusOne::new(1).unwrap(),
                        height: NonZeroU16MinusOne::new(1).unwrap(),
                    },
                    layers: Default::default(),
                },
                background: Default::default(),
                foreground: Default::default(),
            },
            resources: Resources {
                images: Default::default(),
                image_arrays: Default::default(),
                sounds: Default::default(),
            },
            config: Config {
                commands: Default::default(),
            },
            meta: Metadata {
                authors: Default::default(),
                licenses: Default::default(),
                version: Default::default(),
                credits: Default::default(),
                memo: Default::default(),
            },
        };

        fn conv_curv_type(curve_type: i32) -> anyhow::Result<AnimPointCurveType> {
            match curve_type {
                i if i == CurveType::Step as i32 => Ok(AnimPointCurveType::Step),
                i if i == CurveType::Linear as i32 => Ok(AnimPointCurveType::Linear),
                i if i == CurveType::Slow as i32 => Ok(AnimPointCurveType::Slow),
                i if i == CurveType::Fast as i32 => Ok(AnimPointCurveType::Fast),
                i if i == CurveType::Smooth as i32 => Ok(AnimPointCurveType::Smooth),
                _ => Err(anyhow!("non supported curve type")),
            }
        }

        // animations
        let mut old_env_assign: HashMap<usize, usize> = Default::default();
        for (index, (env_name, env)) in self.envelopes.into_iter().enumerate() {
            match env.channels {
                1 => {
                    // sound
                    old_env_assign.insert(index, map.animations.sound.len());
                    map.animations.sound.push(SoundAnimation {
                        name: env_name,
                        synchronized: env.synchronized != 0,
                        points: self.env_points.first().map(|e| e.as_slice()).unwrap_or(&[])[env
                            .start_point
                            as usize
                            ..env.start_point as usize + env.num_points as usize]
                            .iter()
                            .map(|e| {
                                anyhow::Ok(AnimPointSound {
                                    curve_type: conv_curv_type(e.curve_type)?,
                                    time: Duration::from_millis(e.time.clamp(0, i32::MAX) as u64),
                                    value: vec1_base {
                                        x: nffixed::from_num(fx2f(e.values[0])),
                                    },
                                })
                            })
                            .collect::<anyhow::Result<Vec<_>>>()?,
                    });
                }
                3 => {
                    // pos (+ rot)
                    old_env_assign.insert(index, map.animations.pos.len());
                    map.animations.pos.push(PosAnimation {
                        name: env_name,
                        synchronized: env.synchronized != 0,
                        points: self.env_points.first().map(|e| e.as_slice()).unwrap_or(&[])[env
                            .start_point
                            as usize
                            ..env.start_point as usize + env.num_points as usize]
                            .iter()
                            .map(|e| {
                                anyhow::Ok(AnimPointPos {
                                    curve_type: conv_curv_type(e.curve_type)?,
                                    time: Duration::from_millis(e.time.clamp(0, i32::MAX) as u64),
                                    value: fvec3 {
                                        x: ffixed::from_num(fx2f(e.values[0]) / 32.0),
                                        y: ffixed::from_num(fx2f(e.values[1]) / 32.0),
                                        z: ffixed::from_num(fx2f(e.values[2])),
                                    },
                                })
                            })
                            .collect::<anyhow::Result<Vec<_>>>()?,
                    });
                }
                4 => {
                    // color
                    old_env_assign.insert(index, map.animations.color.len());
                    map.animations.color.push(ColorAnimation {
                        name: env_name,
                        synchronized: env.synchronized != 0,
                        points: self.env_points.first().map(|e| e.as_slice()).unwrap_or(&[])[env
                            .start_point
                            as usize
                            ..env.start_point as usize + env.num_points as usize]
                            .iter()
                            .map(|e| {
                                anyhow::Ok(AnimPointColor {
                                    curve_type: conv_curv_type(e.curve_type)?,
                                    time: Duration::from_millis(e.time.clamp(0, i32::MAX) as u64),
                                    value: nfvec4 {
                                        x: nffixed::from_num(fx2f(e.values[0]).clamp(0.0, 1.0)),
                                        y: nffixed::from_num(fx2f(e.values[1]).clamp(0.0, 1.0)),
                                        z: nffixed::from_num(fx2f(e.values[2]).clamp(0.0, 1.0)),
                                        w: nffixed::from_num(fx2f(e.values[3]).clamp(0.0, 1.0)),
                                    },
                                })
                            })
                            .collect::<anyhow::Result<Vec<_>>>()?,
                    });
                }
                _ => return Err(anyhow!("this envelope is not supported")),
            }
        }

        // resources
        for MapSound { name, def: _, data } in self.sounds.into_iter() {
            let data = data.ok_or(anyhow!("sound data not loaded"))?;
            let res = MapResourceRef {
                name: ReducedAsciiString::from_str_autoconvert(&name),
                meta: MapResourceMetaData {
                    blake3_hash: Map::generate_hash_for(&data),
                    ty: "opus".try_into().unwrap(),
                },
                hq_meta: None,
            };
            sound_resources.insert(
                res.meta.blake3_hash,
                LegacyMapToNewRes {
                    buf: data,
                    ty: "opus".to_string(),
                    name: res.name.to_string(),
                },
            );
            map.resources.sounds.push(res.clone());
        }

        let mut old_img_assign: HashMap<usize, usize> = Default::default();
        let mut old_img_array_assign: HashMap<usize, usize> = Default::default();
        let mut ext_image_count = 0;
        for (img_index, image) in self.images.into_iter().enumerate() {
            // was the image used in tile layer and/or quad layer?
            let mut in_tile_layer = false;
            let mut in_quad_layer = false;
            for layer in &self.layers {
                match layer {
                    MapLayer::Tile(layer) => {
                        if layer.0.image == img_index as i32 {
                            in_tile_layer = true;
                        }
                    }
                    MapLayer::Quads(layer) => {
                        if layer.0.image == img_index as i32 {
                            in_quad_layer = true;
                        }
                    }
                    _ => {}
                }
            }

            fn check_size_and_dilate<'a>(
                thread_pool: &rayon::ThreadPool,
                img: Cow<'a, [u8]>,
                mut width: u32,
                mut height: u32,
                png_validation: PngValidatorOptions,
                dilate: bool,
                in_tile_layer_only: bool,
            ) -> (Cow<'a, [u8]>, u32, u32) {
                let mut res = img;
                if width > png_validation.max_width.get()
                    || height > png_validation.max_height.get()
                {
                    let width_ratio =
                        (width as f64 / png_validation.max_width.get() as f64).clamp(1.0, f64::MAX);
                    let height_ratio = (height as f64 / png_validation.max_height.get() as f64)
                        .clamp(1.0, f64::MAX);

                    let ratio = width_ratio.max(height_ratio);

                    let new_width = ((width as f64 / ratio) as u32).clamp(1, u32::MAX);
                    let new_height = ((height as f64 / ratio) as u32).clamp(1, u32::MAX);

                    res = resize_rgba(res, width, height, new_width, new_height).into();

                    width = new_width;
                    height = new_height;
                }
                if dilate {
                    if in_tile_layer_only && width % 16 == 0 && height % 16 == 0 {
                        let sub_width = width / 16;
                        let sub_height = height / 16;
                        for y in 0..16 {
                            for x in 0..16 {
                                utils::dilate_image_sub(
                                    thread_pool,
                                    res.to_mut(),
                                    width as usize,
                                    height as usize,
                                    4,
                                    x * sub_width as usize,
                                    y * sub_height as usize,
                                    sub_width as usize,
                                    sub_height as usize,
                                );
                            }
                        }
                    } else {
                        utils::dilate_image(
                            thread_pool,
                            res.to_mut(),
                            width as usize,
                            height as usize,
                            4,
                        );
                    }
                }
                (res, width, height)
            }

            let (hash, png_data) = if let Some(internal_img) = image.internal_img {
                let (internal_img, width, height) = check_size_and_dilate(
                    thread_pool,
                    internal_img.into(),
                    image.item_data.width as u32,
                    image.item_data.height as u32,
                    png_validation,
                    dilate,
                    in_tile_layer && !in_quad_layer,
                );
                let img = save_png_image(&internal_img, width, height)?;
                (Map::generate_hash_for(&img), img)
            } else {
                let img = images
                    .get({
                        let i = ext_image_count;
                        ext_image_count += 1;
                        i
                    })
                    .ok_or_else(|| anyhow!("image with name {} was not loaded", image.img_name))?;
                let mut img_data: Vec<u8> = Vec::new();
                let img = load_png_image_as_rgba(img, |width, height, color_channel_count| {
                    img_data.resize(width * height * color_channel_count, Default::default());
                    &mut img_data
                })?;
                let (img_data, width, height) = check_size_and_dilate(
                    thread_pool,
                    img.data.into(),
                    img.width as u32,
                    img.height as u32,
                    png_validation,
                    dilate,
                    in_tile_layer && !in_quad_layer,
                );
                let img = save_png_image(&img_data, width, height)?;
                (Map::generate_hash_for(&img), img)
            };
            let res_ref = MapResourceRef {
                name: ReducedAsciiString::from_str_autoconvert(&image.img_name),
                meta: MapResourceMetaData {
                    blake3_hash: hash,
                    ty: "png".try_into().unwrap(),
                },
                hq_meta: None,
            };
            image_resources.insert(
                res_ref.meta.blake3_hash,
                LegacyMapToNewRes {
                    buf: png_data,
                    ty: "png".into(),
                    name: res_ref.name.to_string(),
                },
            );
            if in_tile_layer {
                old_img_array_assign.insert(img_index, map.resources.image_arrays.len());
                map.resources.image_arrays.push(res_ref.clone());
            }
            if in_quad_layer {
                old_img_assign.insert(img_index, map.resources.images.len());
                map.resources.images.push(res_ref.clone());
            }
        }

        let mut tune_zones: FxLinkedHashMap<u8, MapLayerTilePhysicsTuneZone> = Default::default();

        // handle settings before layers, since we need the tune zones
        if let Some(settings) = self.infos.first() {
            map.meta = Metadata {
                authors: vec![settings.author.clone()],
                licenses: vec![settings.license.clone()],
                version: settings.map_version.clone(),
                credits: settings.credits.clone(),
                memo: Default::default(),
            };
            map.config = Config {
                commands: settings
                    .settings
                    .iter()
                    .filter_map(|setting| {
                        let setting_trimmed = setting.trim();
                        if setting_trimmed.starts_with("tune_zone")
                            && setting_trimmed
                                .chars()
                                .nth("tune_zone".chars().count())
                                .is_some_and(|c| c.is_whitespace())
                        {
                            let (_, cmd) = setting
                                .trim()
                                .split_once(' ')
                                .map(|(s1, s2)| (s1.to_string(), s2.to_string()))
                                .unwrap_or_else(|| (setting.clone(), "".to_string()));

                            let (index, cmd) = cmd
                                .trim()
                                .split_once(' ')
                                .map(|(s1, s2)| (s1.to_string(), s2.to_string()))
                                .unwrap_or_else(|| (cmd.clone(), "".to_string()));

                            if let Ok(index) = index.trim().parse::<u8>() {
                                let tune_zone = tune_zones.entry(index).or_insert_with(|| {
                                    MapLayerTilePhysicsTuneZone {
                                        name: "".into(),
                                        tunes: Default::default(),
                                    }
                                });
                                let (tune_param, tune_val) = cmd
                                    .trim()
                                    .split_once(' ')
                                    .map(|(s1, s2)| (s1.to_string(), s2.to_string()))
                                    .unwrap_or_else(|| (cmd.clone(), "".to_string()));
                                tune_zone.tunes.insert(tune_param, tune_val);
                            }

                            None
                        } else if setting_trimmed.starts_with("tune")
                            && setting_trimmed
                                .chars()
                                .nth("tune".chars().count())
                                .is_some_and(|c| c.is_whitespace())
                        {
                            let (_, cmd) = setting
                                .trim()
                                .split_once(' ')
                                .map(|(s1, s2)| (s1.to_string(), s2.to_string()))
                                .unwrap_or_else(|| (setting.clone(), "".to_string()));

                            let tune_zone = tune_zones.entry(0).or_insert_with(|| {
                                MapLayerTilePhysicsTuneZone {
                                    name: "".into(),
                                    tunes: Default::default(),
                                }
                            });
                            let (tune_param, tune_val) = cmd
                                .trim()
                                .split_once(' ')
                                .map(|(s1, s2)| (s1.to_string(), s2.to_string()))
                                .unwrap_or_else(|| (cmd.clone(), "".to_string()));
                            tune_zone.tunes.insert(tune_param, tune_val);

                            None
                        } else {
                            Some(
                                setting
                                    .trim()
                                    .split_once(' ')
                                    .map(|(s1, s2)| (s1.to_string(), s2.to_string()))
                                    .unwrap_or_else(|| (setting.clone(), "".to_string())),
                            )
                        }
                    })
                    .collect(),
            }
        }

        // layers
        let mut passed_game_layer = false;
        for group in self.groups.into_iter() {
            let layers = &self.layers[group.start_layer as usize
                ..group.start_layer as usize + group.num_layers as usize];
            let group_def = MapGroup {
                attr: MapGroupAttr {
                    offset: fvec2::new(
                        ffixed::from_num(group.offset_x as f64 / 32.0),
                        ffixed::from_num(group.offset_y as f64 / 32.0),
                    ),
                    parallax: fvec2::new(group.parallax_x.into(), group.parallax_y.into()),
                    clipping: if group.use_clipping > 0 {
                        Some(MapGroupAttrClipping {
                            pos: fvec2::new(
                                ffixed::from_num(group.clip_x as f64 / 32.0),
                                ffixed::from_num(group.clip_y as f64 / 32.0),
                            ),
                            size: ufvec2::new(
                                uffixed::from_num(group.clip_w.clamp(0, i32::MAX) as f64 / 32.0),
                                uffixed::from_num(group.clip_h.clamp(0, i32::MAX) as f64 / 32.0),
                            ),
                        })
                    } else {
                        None
                    },
                },
                layers: Default::default(),
                name: if group.version >= 3 {
                    Self::read_str_from_ints(&group.name)
                } else {
                    String::new()
                },
            };
            let mut groups = if !passed_game_layer {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            groups.push(group_def.clone());
            for layer in layers.iter() {
                if let Some(layer) = match layer {
                    MapLayer::Tile(MapLayerTile(attr, tiles_detail, tiles)) => {
                        let is_game_layer = (attr.flags & TilesLayerFlag::Game as i32) != 0;
                        passed_game_layer |= is_game_layer;
                        // fill physics group
                        if is_game_layer {
                            map.groups.physics.attr = MapGroupPhysicsAttr {
                                width: NonZeroU16MinusOne::new(attr.width as u16)
                                    .ok_or(anyhow!("tile layer width was 0"))?,
                                height: NonZeroU16MinusOne::new(attr.height as u16)
                                    .ok_or(anyhow!("tile layer height was 0"))?,
                            };
                            let g = groups.last_mut().unwrap();
                            // if game layer is first layer in group -> ignore empty groups
                            if g.layers.is_empty() {
                                groups.pop();
                            }
                            groups = if !passed_game_layer {
                                &mut map.groups.background
                            } else {
                                &mut map.groups.foreground
                            };
                            groups.push(group_def.clone());
                        }
                        if attr.flags != 0 {
                            map.groups.physics.layers.push(match tiles_detail {
                                MapTileLayerDetail::Tile(_) => {
                                    let tiles: Vec<_> = tiles
                                        .iter()
                                        .map(|tile| TileBase {
                                            index: tile.index,
                                            flags: TileFlags::from_bits_truncate(tile.flags),
                                        })
                                        .collect();
                                    if is_game_layer {
                                        MapLayerPhysics::Game(MapLayerTilePhysicsBase { tiles })
                                    } else {
                                        MapLayerPhysics::Front(MapLayerTilePhysicsBase { tiles })
                                    }
                                }
                                MapTileLayerDetail::Tele(tiles_detail) => {
                                    MapLayerPhysics::Tele(MapLayerTilePhysicsTele {
                                        base: MapLayerTilePhysicsBase {
                                            tiles: tiles
                                                .iter()
                                                .zip(tiles_detail.iter())
                                                .map(|(tile, tile_detail)| TeleTile {
                                                    base: TileBase {
                                                        index: tile_detail.tile_type,
                                                        flags: TileFlags::from_bits_truncate(
                                                            tile.flags,
                                                        ),
                                                    },
                                                    number: tile_detail.number,
                                                })
                                                .collect(),
                                        },
                                        tele_names: Default::default(),
                                    })
                                }
                                MapTileLayerDetail::Speedup(tiles_detail) => {
                                    MapLayerPhysics::Speedup(MapLayerTilePhysicsBase {
                                        tiles: tiles
                                            .iter()
                                            .zip(tiles_detail.iter())
                                            .map(|(tile, tile_detail)| SpeedupTile {
                                                base: TileBase {
                                                    index: tile_detail.tile_type,
                                                    flags: TileFlags::from_bits_truncate(
                                                        tile.flags,
                                                    ),
                                                },
                                                angle: tile_detail.angle,
                                                force: tile_detail.force,
                                                max_speed: tile_detail.max_speed,
                                            })
                                            .collect(),
                                    })
                                }
                                MapTileLayerDetail::Switch(tiles_detail) => {
                                    MapLayerPhysics::Switch(MapLayerTilePhysicsSwitch {
                                        base: MapLayerTilePhysicsBase {
                                            tiles: tiles
                                                .iter()
                                                .zip(tiles_detail.iter())
                                                .map(|(_, tile_detail)| SwitchTile {
                                                    base: TileBase {
                                                        index: tile_detail.tile_type,
                                                        flags: TileFlags::from_bits_truncate(
                                                            tile_detail.flags,
                                                        ),
                                                    },
                                                    delay: tile_detail.delay,
                                                    number: tile_detail.number,
                                                })
                                                .collect(),
                                        },
                                        switch_names: Default::default(),
                                    })
                                }
                                MapTileLayerDetail::Tune(tiles_detail) => {
                                    MapLayerPhysics::Tune(MapLayerTilePhysicsTune {
                                        base: MapLayerTilePhysicsBase {
                                            tiles: tiles
                                                .iter()
                                                .zip(tiles_detail.iter())
                                                .map(|(tile, tile_detail)| TuneTile {
                                                    base: TileBase {
                                                        index: tile_detail.tile_type,
                                                        flags: TileFlags::from_bits_truncate(
                                                            tile.flags,
                                                        ),
                                                    },
                                                    number: tile_detail.number,
                                                })
                                                .collect(),
                                        },
                                        tune_zones: tune_zones.clone(),
                                    })
                                }
                            });
                            None
                        } else {
                            Some(mapnew::groups::layers::design::MapLayer::Tile(
                                mapnew::groups::layers::design::MapLayerTile {
                                    attr: MapTileLayerAttr {
                                        width: NonZeroU16MinusOne::new(attr.width as u16)
                                            .ok_or(anyhow!("width of tile layer was 0"))?,
                                        height: NonZeroU16MinusOne::new(attr.height as u16)
                                            .ok_or(anyhow!("height of tile layer was 0"))?,
                                        color: nfvec4 {
                                            x: nffixed::from_num(attr.color.x as f32 / 255.0),
                                            y: nffixed::from_num(attr.color.y as f32 / 255.0),
                                            z: nffixed::from_num(attr.color.z as f32 / 255.0),
                                            w: nffixed::from_num(attr.color.w as f32 / 255.0),
                                        },
                                        high_detail: (attr.layer.flags & LayerFlag::Detail as i32)
                                            != 0,
                                        color_anim: old_env_assign
                                            .get(&(attr.color_env as usize))
                                            .copied(),
                                        color_anim_offset: time::Duration::milliseconds(
                                            attr.color_env_offset as i64,
                                        ),
                                        image_array: if attr.image >= 0 {
                                            Some(
                                                *old_img_array_assign
                                                    .get(&(attr.image as usize))
                                                    .ok_or(anyhow!("img index out of bounds"))?,
                                            )
                                        } else {
                                            None
                                        },
                                    },
                                    tiles: tiles
                                        .iter()
                                        .map(|tile| TileBase {
                                            index: tile.index,
                                            flags: TileFlags::from_bits_truncate(tile.flags),
                                        })
                                        .collect(),
                                    name: if attr.version >= 3 {
                                        Self::read_str_from_ints(&attr.name)
                                    } else {
                                        String::new()
                                    },
                                },
                            ))
                        }
                    }
                    MapLayer::Quads(MapLayerQuad(attr, quads)) => {
                        Some(mapnew::groups::layers::design::MapLayer::Quad(
                            mapnew::groups::layers::design::MapLayerQuad {
                                attr: MapLayerQuadsAttrs {
                                    image: if attr.image >= 0 {
                                        Some(
                                            *old_img_assign
                                                .get(&(attr.image as usize))
                                                .ok_or(anyhow!("img index out of bounds"))?,
                                        )
                                    } else {
                                        None
                                    },
                                    high_detail: (attr.layer.flags & LayerFlag::Detail as i32) != 0,
                                },
                                quads: quads
                                    .iter()
                                    .map(|q| Quad {
                                        points: {
                                            let mut r: [fvec2; 5] = Default::default();

                                            for (i, r) in r.iter_mut().enumerate() {
                                                *r = fvec2::new(
                                                    ffixed::from_num(
                                                        q.points[i].x as f64 / 1024.0 / 32.0,
                                                    ),
                                                    ffixed::from_num(
                                                        q.points[i].y as f64 / 1024.0 / 32.0,
                                                    ),
                                                );
                                            }

                                            r
                                        },
                                        colors: {
                                            let mut r: [nfvec4; 4] = Default::default();

                                            for (i, r) in r.iter_mut().enumerate() {
                                                *r = nfvec4::new(
                                                    nffixed::from_num(q.colors[i].x as f32 / 255.0),
                                                    nffixed::from_num(q.colors[i].y as f32 / 255.0),
                                                    nffixed::from_num(q.colors[i].z as f32 / 255.0),
                                                    nffixed::from_num(q.colors[i].w as f32 / 255.0),
                                                );
                                            }

                                            r
                                        },
                                        tex_coords: {
                                            let mut r: [fvec2; 4] = Default::default();

                                            for (i, r) in r.iter_mut().enumerate() {
                                                *r = fvec2::new(
                                                    ffixed::from_num(fx2f(q.tex_coords[i].x)),
                                                    ffixed::from_num(fx2f(q.tex_coords[i].y)),
                                                );
                                            }

                                            r
                                        },
                                        pos_anim: old_env_assign
                                            .get(&(q.pos_env as usize))
                                            .copied(),
                                        pos_anim_offset: time::Duration::milliseconds(
                                            q.pos_env_offset as i64,
                                        ),
                                        color_anim: old_env_assign
                                            .get(&(q.color_env as usize))
                                            .copied(),
                                        color_anim_offset: time::Duration::milliseconds(
                                            q.color_env_offset as i64,
                                        ),
                                    })
                                    .collect(),
                                name: if attr.version >= 2 {
                                    Self::read_str_from_ints(&attr.name)
                                } else {
                                    String::new()
                                },
                            },
                        ))
                    }
                    MapLayer::Sound { def, sounds } => Some(
                        mapnew::groups::layers::design::MapLayer::Sound(MapLayerSound {
                            attr: MapLayerSoundAttrs {
                                sound: if def.sound >= 0 {
                                    Some(def.sound as usize)
                                } else {
                                    None
                                },
                                high_detail: (def.layer.flags & LayerFlag::Detail as i32) != 0,
                            },
                            sounds: sounds
                                .iter()
                                .map(|sound| {
                                    anyhow::Ok(Sound {
                                        pos: fvec2::new(
                                            ffixed::from_num(fx2f(sound.pos.x) as f64 / 32.0),
                                            ffixed::from_num(fx2f(sound.pos.y) as f64 / 32.0),
                                        ),
                                        looped: sound.looped > 0,
                                        panning: sound.panning > 0,
                                        time_delay: Duration::from_secs(sound.time_delay as u64),
                                        falloff: nffixed::from_num(
                                            (sound.falloff as f64 / 255.0).clamp(0.0, 1.0),
                                        ),
                                        pos_anim: old_env_assign
                                            .get(&(sound.pos_env as usize))
                                            .copied(),
                                        pos_anim_offset: time::Duration::milliseconds(
                                            sound.pos_env_offset as i64,
                                        ),
                                        sound_anim: old_env_assign
                                            .get(&(sound.sound_env as usize))
                                            .copied(),
                                        sound_anim_offset: time::Duration::milliseconds(
                                            sound.sound_env_offset as i64,
                                        ),
                                        shape: unsafe {
                                            match sound.shape.ty {
                                                x if x == SoundShapeTy::ShapeCircle as i32 => {
                                                    SoundShape::Circle {
                                                        radius: uffixed::from_num(
                                                            sound.shape.props.circle.radius as f64
                                                                / 32.0,
                                                        ),
                                                    }
                                                }
                                                x if x == SoundShapeTy::ShapeRectangle as i32 => {
                                                    SoundShape::Rect {
                                                        size: ufvec2::new(
                                                            uffixed::from_num(
                                                                sound.shape.props.rect.width as f64
                                                                    / 32.0,
                                                            ),
                                                            uffixed::from_num(
                                                                sound.shape.props.rect.height
                                                                    as f64
                                                                    / 32.0,
                                                            ),
                                                        ),
                                                    }
                                                }
                                                _ => return Err(anyhow!("unknown sound shape")),
                                            }
                                        },
                                    })
                                })
                                .collect::<anyhow::Result<Vec<_>>>()?,
                            name: Self::read_str_from_ints(&def.name),
                        }),
                    ),
                    MapLayer::Unknown(_) => {
                        return Err(anyhow!("for now unknown layers are not supported"))
                    }
                } {
                    groups.last_mut().unwrap().layers.push(layer);
                }
            }
            let g = groups.last_mut().unwrap();
            // ignore empty groups
            if g.layers.is_empty() {
                groups.pop();
            }
        }

        Ok(LegacyMapToNewOutput {
            map,
            resources: LegacyMapToNewResources {
                images: image_resources,
                sounds: sound_resources,
            },
        })
    }

    /// returns a Vec containing the file ready to write to disk
    pub fn from_map(
        map: Map,
        images: &[Vec<u8>],
        image_arrays: &[Vec<u8>],
        sounds: &[Vec<u8>],
    ) -> Vec<u8> {
        let mut res = Self::new();
        res.data_file.header.id[0] = b'D' as i8;
        res.data_file.header.id[1] = b'A' as i8;
        res.data_file.header.id[2] = b'T' as i8;
        res.data_file.header.id[3] = b'A' as i8;
        res.data_file.header.version = 4;

        let mut data_compressed_data: Vec<u8> = Vec::new();
        let mut data_items: Vec<u8> = Vec::new();

        fn conv_curv_type(curve_type: AnimPointCurveType) -> i32 {
            match curve_type {
                AnimPointCurveType::Step => CurveType::Step as i32,
                AnimPointCurveType::Linear => CurveType::Linear as i32,
                AnimPointCurveType::Slow => CurveType::Slow as i32,
                AnimPointCurveType::Fast => CurveType::Fast as i32,
                AnimPointCurveType::Smooth => CurveType::Smooth as i32,
            }
        }

        // version
        {
            let item_index = res.data_file.info.item_offsets.len() as i32;

            res.data_file
                .info
                .item_offsets
                .push(data_items.len() as i32);

            let mut ver_data: Vec<u8> = Vec::new();
            let version = CMapItemVersion { version: 1 };
            version.write_to_vec(&mut ver_data);

            let ver_item = CDatafileItem {
                size: ver_data.len() as i32,
                type_and_id: ((MapItemTypes::Version as i32) << 16),
            };
            ver_item.write_to_vec(&mut data_items);
            data_items.append(&mut ver_data);

            res.data_file.info.item_types.push(CDatafileItemType {
                item_type: MapItemTypes::Version as i32,
                start: item_index,
                num: 1,
            });
        }

        // animations
        let pos_env_index_offset = 0;
        let sound_env_index_offset = pos_env_index_offset + map.animations.pos.len();
        let color_env_index_offset = sound_env_index_offset + map.animations.sound.len();
        {
            let mut env_points: Vec<CEnvPoint> = Vec::new();
            let mut envs: Vec<CMapItemEnvelope> = Vec::new();

            for pos_anim in map.animations.pos.iter() {
                let start_index = env_points.len();
                env_points.extend(pos_anim.points.iter().map(|p| CEnvPoint {
                    time: p.time.as_millis() as i32,
                    curve_type: conv_curv_type(p.curve_type),
                    values: [
                        f2fx(p.value.x.to_num::<f32>() * 32.0),
                        f2fx(p.value.y.to_num::<f32>() * 32.0),
                        f2fx(p.value.z.to_num::<f32>()),
                        0,
                    ],
                }));
                let mut env = CMapItemEnvelope {
                    version: CMapItemEnvelopeVer::CurVersion as i32,
                    channels: 3,
                    start_point: start_index as i32,
                    num_points: (env_points.len() - start_index) as i32,
                    name: Default::default(),
                    synchronized: pos_anim.synchronized as i32,
                };
                Self::str_to_ints(&mut env.name, pos_anim.name.as_bytes());
                envs.push(env);
            }

            for sound_anim in map.animations.sound.iter() {
                let start_index = env_points.len();
                env_points.extend(sound_anim.points.iter().map(|p| CEnvPoint {
                    time: p.time.as_millis() as i32,
                    curve_type: conv_curv_type(p.curve_type),
                    values: [f2fx(p.value.x.to_num()), 0, 0, 0],
                }));
                let mut env = CMapItemEnvelope {
                    version: CMapItemEnvelopeVer::CurVersion as i32,
                    channels: 1,
                    start_point: start_index as i32,
                    num_points: (env_points.len() - start_index) as i32,
                    name: Default::default(),
                    synchronized: sound_anim.synchronized as i32,
                };
                Self::str_to_ints(&mut env.name, sound_anim.name.as_bytes());
                envs.push(env);
            }

            for color_anim in map.animations.color.iter() {
                let start_index = env_points.len();
                env_points.extend(color_anim.points.iter().map(|p| CEnvPoint {
                    time: p.time.as_millis() as i32,
                    curve_type: conv_curv_type(p.curve_type),
                    values: [
                        f2fx(p.value.r().to_num()),
                        f2fx(p.value.g().to_num()),
                        f2fx(p.value.b().to_num()),
                        f2fx(p.value.a().to_num()),
                    ],
                }));
                let mut env = CMapItemEnvelope {
                    version: CMapItemEnvelopeVer::CurVersion as i32,
                    channels: 4,
                    start_point: start_index as i32,
                    num_points: (env_points.len() - start_index) as i32,
                    name: Default::default(),
                    synchronized: color_anim.synchronized as i32,
                };
                Self::str_to_ints(&mut env.name, color_anim.name.as_bytes());
                envs.push(env);
            }

            // next: write all env points to data
            let item_index = res.data_file.info.item_offsets.len() as i32;
            res.data_file
                .info
                .item_offsets
                .push(data_items.len() as i32);

            let mut data_envs: Vec<u8> = Default::default();
            for env_point in &env_points {
                env_point.write_to_vec(&mut data_envs);
            }
            let env_item = CDatafileItem {
                size: data_envs.len() as i32,
                type_and_id: ((MapItemTypes::Envpoints as i32) << 16),
            };
            env_item.write_to_vec(&mut data_items);
            data_items.extend(data_envs);

            res.data_file.info.item_types.push(CDatafileItemType {
                item_type: MapItemTypes::Envpoints as i32,
                start: item_index,
                num: env_points.len() as i32,
            });

            // next: write all env definitions to data
            let item_index = res.data_file.info.item_offsets.len() as i32;
            let env_count = envs.len();
            for (index, env) in envs.into_iter().enumerate() {
                res.data_file
                    .info
                    .item_offsets
                    .push(data_items.len() as i32);

                let mut env_data: Vec<u8> = Vec::new();
                env.write_to_vec(&mut env_data);

                let env_item = CDatafileItem {
                    size: env_data.len() as i32,
                    type_and_id: ((MapItemTypes::Envelope as i32) << 16) | (index as i32),
                };
                env_item.write_to_vec(&mut data_items);
                data_items.append(&mut env_data);
            }
            res.data_file.info.item_types.push(CDatafileItemType {
                item_type: MapItemTypes::Envelope as i32,
                start: item_index,
                num: env_count as i32,
            });
        }

        let images_len = map.resources.images.len();

        // resources
        {
            // images
            let item_index = res.data_file.info.item_offsets.len() as i32;
            let image_count = map.resources.images.len() + map.resources.image_arrays.len();
            for (index, image) in map.resources.images.into_iter().enumerate() {
                res.data_file
                    .info
                    .item_offsets
                    .push(data_items.len() as i32);

                let mut img_data: Vec<u8> = Vec::new();
                let img = load_png_image_as_rgba(
                    images.get(index).unwrap_or_else(|| {
                        panic!("did not find image with name: {}", image.name.as_str())
                    }),
                    |width, height, color_channel_count| {
                        img_data.resize(width * height * color_channel_count, Default::default());
                        &mut img_data
                    },
                )
                .unwrap();

                // add name as data
                let data_offset = data_compressed_data.len() as i32;
                let name_cstr = CString::new(image.name.as_str()).unwrap();
                let uncompressed_size = name_cstr.as_bytes_with_nul().len();
                let compressed_data = Self::compress_data(name_cstr.as_bytes_with_nul());
                data_compressed_data.extend(compressed_data);
                let name_index = res.data_file.info.data_offsets.len();
                res.data_file.info.data_offsets.push(data_offset);
                res.data_file.info.data_sizes.push(uncompressed_size as i32);

                // add image as data
                let data_offset = data_compressed_data.len() as i32;
                let uncompressed_size = img.data.len();
                let compressed_data = Self::compress_data(img.data);
                data_compressed_data.extend(compressed_data);
                let data_index = res.data_file.info.data_offsets.len();
                res.data_file.info.data_offsets.push(data_offset);
                res.data_file.info.data_sizes.push(uncompressed_size as i32);

                let item_data = CMapItemImage {
                    version: 1,
                    width: img.width as i32,
                    height: img.height as i32,
                    external: 0,
                    image_name: name_index as i32,
                    image_data: data_index as i32,
                };

                let mut img_data: Vec<u8> = Vec::new();
                item_data.write_to_vec(&mut img_data);

                let data_item = CDatafileItem {
                    size: img_data.len() as i32,
                    type_and_id: ((MapItemTypes::Image as i32) << 16) | (index as i32),
                };
                data_item.write_to_vec(&mut data_items);
                data_items.append(&mut img_data);
            }

            // images 2d array
            for (index, image) in map.resources.image_arrays.into_iter().enumerate() {
                res.data_file
                    .info
                    .item_offsets
                    .push(data_items.len() as i32);

                let mut img_data: Vec<u8> = Vec::new();
                let img = load_png_image_as_rgba(
                    image_arrays.get(index).unwrap_or_else(|| {
                        panic!("did not find image with name: {}", image.name.as_str())
                    }),
                    |width, height, color_channel_count| {
                        img_data.resize(width * height * color_channel_count, Default::default());
                        &mut img_data
                    },
                )
                .unwrap();

                // add name as data
                let data_offset = data_compressed_data.len() as i32;
                let name_cstr = CString::new(image.name.as_str()).unwrap();
                let uncompressed_size = name_cstr.as_bytes_with_nul().len();
                let compressed_data = Self::compress_data(name_cstr.as_bytes_with_nul());
                data_compressed_data.extend(compressed_data);
                let name_index = res.data_file.info.data_offsets.len();
                res.data_file.info.data_offsets.push(data_offset);
                res.data_file.info.data_sizes.push(uncompressed_size as i32);

                // add image as data
                let data_offset = data_compressed_data.len() as i32;
                let uncompressed_size = img.data.len();
                let compressed_data = Self::compress_data(img.data);
                data_compressed_data.extend(compressed_data);
                let data_index = res.data_file.info.data_offsets.len();
                res.data_file.info.data_offsets.push(data_offset);
                res.data_file.info.data_sizes.push(uncompressed_size as i32);

                let item_data = CMapItemImage {
                    version: 1,
                    width: img.width as i32,
                    height: img.height as i32,
                    external: 0,
                    image_name: name_index as i32,
                    image_data: data_index as i32,
                };

                let mut img_data: Vec<u8> = Vec::new();
                item_data.write_to_vec(&mut img_data);

                let data_item = CDatafileItem {
                    size: img_data.len() as i32,
                    type_and_id: ((MapItemTypes::Image as i32) << 16) | (index as i32),
                };
                data_item.write_to_vec(&mut data_items);
                data_items.append(&mut img_data);
            }
            res.data_file.info.item_types.push(CDatafileItemType {
                item_type: MapItemTypes::Image as i32,
                start: item_index,
                num: image_count as i32,
            });

            // sound
            let item_index = res.data_file.info.item_offsets.len() as i32;
            let sound_count = map.resources.sounds.len();
            for (index, sound) in map.resources.sounds.into_iter().enumerate() {
                res.data_file
                    .info
                    .item_offsets
                    .push(data_items.len() as i32);

                // add name as data
                let data_offset = data_compressed_data.len() as i32;
                let name_cstr = CString::new(sound.name.as_str()).unwrap();
                let uncompressed_size = name_cstr.as_bytes_with_nul().len();
                let compressed_data = Self::compress_data(name_cstr.as_bytes_with_nul());
                data_compressed_data.extend(compressed_data);
                let name_index = res.data_file.info.data_offsets.len();
                res.data_file.info.data_offsets.push(data_offset);
                res.data_file.info.data_sizes.push(uncompressed_size as i32);

                // add image as data
                let sound_data_size = sounds[index].len();
                let data_offset = data_compressed_data.len() as i32;
                let uncompressed_size = sounds[index].len();
                let compressed_data = Self::compress_data(&sounds[index]);
                data_compressed_data.extend(compressed_data);
                let data_index = res.data_file.info.data_offsets.len();
                res.data_file.info.data_offsets.push(data_offset);
                res.data_file.info.data_sizes.push(uncompressed_size as i32);

                let item_data = CMapItemSound {
                    version: 1,
                    external: 0,
                    sound_name: name_index as i32,
                    sound_data: data_index as i32,
                    sound_data_size: sound_data_size as i32,
                };

                let mut img_data: Vec<u8> = Vec::new();
                item_data.write_to_vec(&mut img_data);

                let data_item = CDatafileItem {
                    size: img_data.len() as i32,
                    type_and_id: ((MapItemTypes::Sound as i32) << 16) | (index as i32),
                };
                data_item.write_to_vec(&mut data_items);
                data_items.append(&mut img_data);
            }
            res.data_file.info.item_types.push(CDatafileItemType {
                item_type: MapItemTypes::Sound as i32,
                start: item_index,
                num: sound_count as i32,
            });
        }

        let mut global_map_settings: Vec<[u8; 256]> = Default::default();
        // layers
        {
            let mut group_list: Vec<CMapItemGroup> = Vec::new();

            let layer_item_index = res.data_file.info.item_offsets.len() as i32;
            let mut layer_count = 0;

            let write_groups = |data_compressed_data: &mut Vec<u8>,
                                data_items: &mut Vec<u8>,
                                layer_count: &mut i32,
                                res: &mut CDatafileWrapper,
                                group_list: &mut Vec<CMapItemGroup>,
                                groups: Vec<MapGroup>| {
                for group in groups.into_iter() {
                    let mut group_item = CMapItemGroup {
                        version: 3,
                        offset_x: (group.attr.offset.x.to_num::<f64>() * 32.0).round() as i32,
                        offset_y: (group.attr.offset.y.to_num::<f64>() * 32.0).round() as i32,
                        parallax_x: group.attr.parallax.x.to_num::<f64>().round() as i32,
                        parallax_y: group.attr.parallax.y.to_num::<f64>().round() as i32,
                        start_layer: { *layer_count },
                        num_layers: group.layers.len() as i32,
                        use_clipping: group.attr.clipping.as_ref().is_some() as i32,
                        clip_x: group
                            .attr
                            .clipping
                            .map(|c| (c.pos.x.to_num::<f64>() * 32.0).round() as i32)
                            .unwrap_or(0),
                        clip_y: group
                            .attr
                            .clipping
                            .map(|c| (c.pos.y.to_num::<f64>() * 32.0).round() as i32)
                            .unwrap_or(0),
                        clip_w: group
                            .attr
                            .clipping
                            .map(|c| (c.size.x.to_num::<f64>() * 32.0).round() as i32)
                            .unwrap_or(0),
                        clip_h: group
                            .attr
                            .clipping
                            .map(|c| (c.size.y.to_num::<f64>() * 32.0).round() as i32)
                            .unwrap_or(0),
                        name: Default::default(),
                    };
                    Self::str_to_ints(&mut group_item.name, group.name.as_bytes());

                    group_list.push(group_item);

                    for layer in group.layers.iter() {
                        res.data_file
                            .info
                            .item_offsets
                            .push(data_items.len() as i32);

                        let mut data_layer: Vec<u8> = Default::default();
                        match layer {
                            mapnew::groups::layers::design::MapLayer::Abritrary(_) => {
                                panic!("abritrary is not supported.")
                            }
                            mapnew::groups::layers::design::MapLayer::Tile(layer) => {
                                let tiles: Vec<CTile> = layer
                                    .tiles
                                    .iter()
                                    .map(|t| CTile {
                                        index: t.index,
                                        flags: t.flags.bits(),
                                        skip: 0,
                                        reserved: 0,
                                    })
                                    .collect();
                                let mut tiles_data: Vec<u8> = Vec::new();
                                tiles.into_iter().for_each(|t| {
                                    t.write_to_vec(&mut tiles_data);
                                });
                                let data_offset = data_compressed_data.len() as i32;
                                let uncompressed_size = tiles_data.len();
                                let compressed_data = Self::compress_data(&tiles_data);
                                data_compressed_data.extend(compressed_data);
                                let data_index = res.data_file.info.data_offsets.len();
                                res.data_file.info.data_offsets.push(data_offset);
                                res.data_file.info.data_sizes.push(uncompressed_size as i32);

                                let mut layer_item = CMapItemLayerTilemap {
                                    layer: CMapItemLayer {
                                        version: 0,
                                        item_layer: MapLayerTypes::Tiles as i32,
                                        flags: if layer.attr.high_detail {
                                            LayerFlag::Detail as i32
                                        } else {
                                            0
                                        },
                                    },
                                    version: 3,
                                    width: layer.attr.width.get() as i32,
                                    height: layer.attr.height.get() as i32,
                                    flags: 0,
                                    color: ivec4::new(
                                        (layer.attr.color.x.to_num::<f32>() * 255.0) as i32,
                                        (layer.attr.color.y.to_num::<f32>() * 255.0) as i32,
                                        (layer.attr.color.z.to_num::<f32>() * 255.0) as i32,
                                        (layer.attr.color.w.to_num::<f32>() * 255.0) as i32,
                                    ),
                                    color_env: if let Some(l) = layer.attr.color_anim {
                                        (color_env_index_offset + l) as i32
                                    } else {
                                        -1
                                    },
                                    color_env_offset: layer
                                        .attr
                                        .color_anim_offset
                                        .whole_milliseconds()
                                        as i32,
                                    image: layer
                                        .attr
                                        .image_array
                                        .map(|i| (i + images_len) as i32)
                                        .unwrap_or(-1),
                                    data: data_index as i32,
                                    name: Default::default(),
                                    tele: -1,
                                    speedup: -1,
                                    front: -1,
                                    switch: -1,
                                    tune: -1,
                                };
                                Self::str_to_ints(&mut layer_item.name, layer.name.as_bytes());
                                layer_item.write_to_vec(&mut data_layer);
                            }
                            mapnew::groups::layers::design::MapLayer::Quad(layer) => {
                                let quads: Vec<CQuad> = layer
                                    .quads
                                    .iter()
                                    .map(|q| CQuad {
                                        points: {
                                            let mut r: [ivec2; 5] = Default::default();

                                            for (i, r) in r.iter_mut().enumerate() {
                                                *r = ivec2::new(
                                                    (q.points[i].x * ffixed::from_num(1024 * 32))
                                                        .to_num::<i32>(),
                                                    (q.points[i].y * ffixed::from_num(1024 * 32))
                                                        .to_num::<i32>(),
                                                );
                                            }

                                            r
                                        },
                                        colors: {
                                            let mut r: [ivec4; 4] = Default::default();

                                            for (i, r) in r.iter_mut().enumerate() {
                                                *r = ivec4::new(
                                                    (q.colors[i].x.to_num::<f32>() * 255.0) as i32,
                                                    (q.colors[i].y.to_num::<f32>() * 255.0) as i32,
                                                    (q.colors[i].z.to_num::<f32>() * 255.0) as i32,
                                                    (q.colors[i].w.to_num::<f32>() * 255.0) as i32,
                                                );
                                            }

                                            r
                                        },
                                        tex_coords: {
                                            let mut r: [ivec2; 4] = Default::default();

                                            for (i, r) in r.iter_mut().enumerate() {
                                                *r = ivec2::new(
                                                    f2fx(q.tex_coords[i].x.to_num::<f32>()),
                                                    f2fx(q.tex_coords[i].y.to_num::<f32>()),
                                                );
                                            }

                                            r
                                        },
                                        pos_env: if let Some(l) = q.pos_anim {
                                            (pos_env_index_offset + l) as i32
                                        } else {
                                            -1
                                        },
                                        pos_env_offset: q.pos_anim_offset.whole_milliseconds()
                                            as i32,
                                        color_env: if let Some(l) = q.color_anim {
                                            (color_env_index_offset + l) as i32
                                        } else {
                                            -1
                                        },
                                        color_env_offset: q.color_anim_offset.whole_milliseconds()
                                            as i32,
                                    })
                                    .collect();
                                let mut quads_data: Vec<u8> = Vec::new();
                                quads.into_iter().for_each(|t| {
                                    t.write_to_vec(&mut quads_data);
                                });
                                let data_offset = data_compressed_data.len() as i32;
                                let uncompressed_size = quads_data.len();
                                let compressed_data = Self::compress_data(&quads_data);
                                data_compressed_data.extend(compressed_data);
                                let data_index = res.data_file.info.data_offsets.len();
                                res.data_file.info.data_offsets.push(data_offset);
                                res.data_file.info.data_sizes.push(uncompressed_size as i32);

                                let mut layer_item = CMapItemLayerQuads {
                                    layer: CMapItemLayer {
                                        version: 0,
                                        item_layer: MapLayerTypes::Quads as i32,
                                        flags: if layer.attr.high_detail {
                                            LayerFlag::Detail as i32
                                        } else {
                                            0
                                        },
                                    },
                                    version: 2,
                                    num_quads: layer.quads.len() as i32,
                                    data: data_index as i32,
                                    image: layer.attr.image.map(|i| i as i32).unwrap_or(-1),
                                    name: Default::default(),
                                };
                                Self::str_to_ints(&mut layer_item.name, layer.name.as_bytes());
                                layer_item.write_to_vec(&mut data_layer);
                            }
                            mapnew::groups::layers::design::MapLayer::Sound(layer) => {
                                let sounds: Vec<CSoundSource> = layer
                                    .sounds
                                    .iter()
                                    .map(|s| CSoundSource {
                                        pos: ivec2::new(
                                            f2fx(s.pos.x.to_num::<f32>() * 32.0),
                                            f2fx(s.pos.y.to_num::<f32>() * 32.0),
                                        ),
                                        looped: s.looped as i32,
                                        panning: s.panning as i32,
                                        time_delay: s.time_delay.as_secs() as i32,
                                        falloff: (s.falloff.to_num::<f32>() * 255.0) as i32,
                                        pos_env: if let Some(l) = s.pos_anim {
                                            (pos_env_index_offset + l) as i32
                                        } else {
                                            -1
                                        },
                                        pos_env_offset: s.pos_anim_offset.whole_milliseconds()
                                            as i32,
                                        sound_env: if let Some(l) = s.sound_anim {
                                            (sound_env_index_offset + l) as i32
                                        } else {
                                            -1
                                        },
                                        sound_env_offset: s.sound_anim_offset.whole_milliseconds()
                                            as i32,
                                        shape: {
                                            let mut res = CSoundShape::default();
                                            match s.shape {
                                                SoundShape::Rect { size } => {
                                                    res.ty = SoundShapeTy::ShapeRectangle as i32;
                                                    res.props.rect.width =
                                                        (size.x.to_num::<f64>() * 32.0).round()
                                                            as i32;
                                                    res.props.rect.height =
                                                        (size.y.to_num::<f64>() * 32.0).round()
                                                            as i32;
                                                }
                                                SoundShape::Circle { radius } => {
                                                    res.ty = SoundShapeTy::ShapeCircle as i32;
                                                    res.props.circle.radius =
                                                        (radius.to_num::<f64>() * 32.0).round()
                                                            as i32;
                                                }
                                            }
                                            res
                                        },
                                    })
                                    .collect();
                                let mut sounds_data: Vec<u8> = Vec::new();
                                sounds.into_iter().for_each(|t| {
                                    t.write_to_vec(&mut sounds_data);
                                });
                                let data_offset = data_compressed_data.len() as i32;
                                let uncompressed_size = sounds_data.len();
                                let compressed_data = Self::compress_data(&sounds_data);
                                data_compressed_data.extend(compressed_data);
                                let data_index = res.data_file.info.data_offsets.len();
                                res.data_file.info.data_offsets.push(data_offset);
                                res.data_file.info.data_sizes.push(uncompressed_size as i32);

                                let mut layer_item = CMapItemLayerSounds {
                                    layer: CMapItemLayer {
                                        version: 0,
                                        item_layer: MapLayerTypes::Sounds as i32,
                                        flags: if layer.attr.high_detail {
                                            LayerFlag::Detail as i32
                                        } else {
                                            0
                                        },
                                    },
                                    version: CMapItemLayerSoundsVer::CurVersion as i32,
                                    num_sources: layer.sounds.len() as i32,
                                    data: data_index as i32,
                                    sound: layer.attr.sound.map(|i| i as i32).unwrap_or(-1),
                                    name: Default::default(),
                                };
                                Self::str_to_ints(&mut layer_item.name, layer.name.as_bytes());
                                layer_item.write_to_vec(&mut data_layer);
                            }
                        }

                        let data_item = CDatafileItem {
                            size: data_layer.len() as i32,
                            type_and_id: ((MapItemTypes::Layer as i32) << 16) | { *layer_count },
                        };
                        data_item.write_to_vec(data_items);
                        data_items.extend(data_layer);

                        *layer_count += 1;
                    }
                }
            };
            write_groups(
                &mut data_compressed_data,
                &mut data_items,
                &mut layer_count,
                &mut res,
                &mut group_list,
                map.groups.background,
            );
            // write physics group
            let group = map.groups.physics;
            let group_item = CMapItemGroup {
                version: 3,
                offset_x: 0,
                offset_y: 0,
                parallax_x: 100,
                parallax_y: 100,
                start_layer: layer_count,
                num_layers: group.layers.len() as i32,
                use_clipping: 0,
                clip_x: 0,
                clip_y: 0,
                clip_w: 0,
                clip_h: 0,
                name: Default::default(),
            };
            group_list.push(group_item);
            for layer in group.layers {
                let mut data_layer: Vec<u8> = Default::default();
                res.data_file
                    .info
                    .item_offsets
                    .push(data_items.len() as i32);

                let tiles_base_iter: Box<dyn Iterator<Item = &TileBase>> = match &layer {
                    MapLayerPhysics::Arbitrary(_) => {
                        panic!("arbitrary tiles not supported")
                    }
                    MapLayerPhysics::Game(layer) => Box::new(layer.tiles.iter()),
                    MapLayerPhysics::Front(layer) => Box::new(layer.tiles.iter()),
                    MapLayerPhysics::Tele(layer) => {
                        Box::new(layer.base.tiles.iter().map(|t| &t.base))
                    }
                    MapLayerPhysics::Speedup(layer) => {
                        Box::new(layer.tiles.iter().map(|t| &t.base))
                    }
                    MapLayerPhysics::Switch(layer) => {
                        Box::new(layer.base.tiles.iter().map(|t| &t.base))
                    }
                    MapLayerPhysics::Tune(layer) => {
                        Box::new(layer.base.tiles.iter().map(|t| &t.base))
                    }
                };
                let tiles: Vec<CTile> = tiles_base_iter
                    .map(|t| CTile {
                        index: t.index,
                        flags: t.flags.bits(),
                        skip: 0,
                        reserved: 0,
                    })
                    .collect();
                let mut tiles_data: Vec<u8> = Vec::new();
                tiles.into_iter().for_each(|t| {
                    t.write_to_vec(&mut tiles_data);
                });
                let data_offset = data_compressed_data.len() as i32;
                let uncompressed_size = tiles_data.len();
                let compressed_data = Self::compress_data(&tiles_data);
                data_compressed_data.extend(compressed_data);
                let data_index = res.data_file.info.data_offsets.len();
                res.data_file.info.data_offsets.push(data_offset);
                res.data_file.info.data_sizes.push(uncompressed_size as i32);

                // DDrace layers
                let mut tele = -1;
                let mut speedup = -1;
                let mut front = -1;
                let mut switch = -1;
                let mut tune = -1;
                match &layer {
                    MapLayerPhysics::Arbitrary(_) => {
                        panic!("arbitrary tiles not supported")
                    }
                    MapLayerPhysics::Game(_) => {
                        // nothing to do
                    }
                    MapLayerPhysics::Front(_) => {
                        // simply use the previous data index
                        front = data_index as i32;
                    }
                    MapLayerPhysics::Tele(layer) => {
                        let tiles: Vec<CTeleTile> = layer
                            .base
                            .tiles
                            .iter()
                            .map(|t| CTeleTile {
                                number: t.number,
                                tile_type: t.base.index,
                            })
                            .collect();

                        let mut tiles_data: Vec<u8> = Vec::new();
                        tiles.into_iter().for_each(|t| {
                            t.write_to_vec(&mut tiles_data);
                        });
                        let data_offset = data_compressed_data.len() as i32;
                        let uncompressed_size = tiles_data.len();
                        let compressed_data = Self::compress_data(&tiles_data);
                        data_compressed_data.extend(compressed_data);
                        tele = res.data_file.info.data_offsets.len() as i32;
                        res.data_file.info.data_offsets.push(data_offset);
                        res.data_file.info.data_sizes.push(uncompressed_size as i32);
                    }
                    MapLayerPhysics::Speedup(layer) => {
                        let tiles: Vec<CSpeedupTile> = layer
                            .tiles
                            .iter()
                            .map(|t| CSpeedupTile {
                                force: t.force,
                                max_speed: t.max_speed,
                                tile_type: t.base.index,
                                angle: t.angle,
                            })
                            .collect();

                        let mut tiles_data: Vec<u8> = Vec::new();
                        tiles.into_iter().for_each(|t| {
                            t.write_to_vec(&mut tiles_data);
                        });
                        let data_offset = data_compressed_data.len() as i32;
                        let uncompressed_size = tiles_data.len();
                        let compressed_data = Self::compress_data(&tiles_data);
                        data_compressed_data.extend(compressed_data);
                        speedup = res.data_file.info.data_offsets.len() as i32;
                        res.data_file.info.data_offsets.push(data_offset);
                        res.data_file.info.data_sizes.push(uncompressed_size as i32);
                    }
                    MapLayerPhysics::Switch(layer) => {
                        let tiles: Vec<CSwitchTile> = layer
                            .base
                            .tiles
                            .iter()
                            .map(|t| CSwitchTile {
                                number: t.number,
                                tile_type: t.base.index,
                                flags: t.base.flags.bits(),
                                delay: t.delay,
                            })
                            .collect();

                        let mut tiles_data: Vec<u8> = Vec::new();
                        tiles.into_iter().for_each(|t| {
                            t.write_to_vec(&mut tiles_data);
                        });
                        let data_offset = data_compressed_data.len() as i32;
                        let uncompressed_size = tiles_data.len();
                        let compressed_data = Self::compress_data(&tiles_data);
                        data_compressed_data.extend(compressed_data);
                        switch = res.data_file.info.data_offsets.len() as i32;
                        res.data_file.info.data_offsets.push(data_offset);
                        res.data_file.info.data_sizes.push(uncompressed_size as i32);
                    }
                    MapLayerPhysics::Tune(layer) => {
                        let tiles: Vec<CTuneTile> = layer
                            .base
                            .tiles
                            .iter()
                            .map(|t| CTuneTile {
                                number: t.number,
                                tile_type: t.base.index,
                            })
                            .collect();

                        let mut tiles_data: Vec<u8> = Vec::new();
                        tiles.into_iter().for_each(|t| {
                            t.write_to_vec(&mut tiles_data);
                        });
                        let data_offset = data_compressed_data.len() as i32;
                        let uncompressed_size = tiles_data.len();
                        let compressed_data = Self::compress_data(&tiles_data);
                        data_compressed_data.extend(compressed_data);
                        tune = res.data_file.info.data_offsets.len() as i32;
                        res.data_file.info.data_offsets.push(data_offset);
                        res.data_file.info.data_sizes.push(uncompressed_size as i32);

                        for (index, args) in &layer.tune_zones {
                            for (tune_name, tune_val) in &args.tunes {
                                let mut setting: [u8; 256] = vec![0; 256].try_into().unwrap();
                                let cmd = if *index == 0 {
                                    format!("tune \"{}\" \"{}\"", tune_name, tune_val)
                                } else {
                                    format!("tune_zone {index} \"{}\" \"{}\"", tune_name, tune_val)
                                };
                                let src = cmd.as_bytes();
                                setting[0..src.len().min(256)]
                                    .copy_from_slice(&src[0..src.len().min(256)]);
                                *setting.last_mut().unwrap() = 0;
                                global_map_settings.push(setting);
                            }
                        }
                    }
                }

                let mut layer_item = CMapItemLayerTilemap {
                    layer: CMapItemLayer {
                        version: 0,
                        item_layer: MapLayerTypes::Tiles as i32,
                        flags: 0,
                    },
                    version: 3,
                    width: group.attr.width.get() as i32,
                    height: group.attr.height.get() as i32,
                    flags: match &layer {
                        MapLayerPhysics::Arbitrary(_) => {
                            panic!("arbitrary tile layer not supported")
                        }
                        MapLayerPhysics::Game(_) => TilesLayerFlag::Game as i32,
                        MapLayerPhysics::Front(_) => TilesLayerFlag::Front as i32,
                        MapLayerPhysics::Tele(_) => TilesLayerFlag::Tele as i32,
                        MapLayerPhysics::Speedup(_) => TilesLayerFlag::Speedup as i32,
                        MapLayerPhysics::Switch(_) => TilesLayerFlag::Switch as i32,
                        MapLayerPhysics::Tune(_) => TilesLayerFlag::Tune as i32,
                    },
                    color: ivec4::new(255, 255, 255, 255),
                    color_env: -1,
                    color_env_offset: 0,
                    image: -1,
                    data: data_index as i32,
                    name: Default::default(),
                    tele,
                    speedup,
                    front,
                    switch,
                    tune,
                };
                Self::str_to_ints(&mut layer_item.name, "".as_bytes());
                layer_item.write_to_vec(&mut data_layer);

                let data_item = CDatafileItem {
                    size: data_layer.len() as i32,
                    type_and_id: ((MapItemTypes::Layer as i32) << 16) | layer_count,
                };
                data_item.write_to_vec(&mut data_items);
                data_items.extend(data_layer);

                layer_count += 1;
            }
            write_groups(
                &mut data_compressed_data,
                &mut data_items,
                &mut layer_count,
                &mut res,
                &mut group_list,
                map.groups.foreground,
            );

            // write groups
            let item_index = res.data_file.info.item_offsets.len() as i32;

            let group_count = group_list.len();
            for (i, group) in group_list.into_iter().enumerate() {
                res.data_file
                    .info
                    .item_offsets
                    .push(data_items.len() as i32);
                let mut data_groups: Vec<u8> = Default::default();
                group.write_to_vec(&mut data_groups);
                let data_item = CDatafileItem {
                    size: data_groups.len() as i32,
                    type_and_id: ((MapItemTypes::Group as i32) << 16) | (i as i32),
                };
                data_item.write_to_vec(&mut data_items);
                data_items.extend(data_groups);
            }

            res.data_file.info.item_types.push(CDatafileItemType {
                item_type: MapItemTypes::Group as i32,
                start: item_index,
                num: group_count as i32,
            });

            // write layers
            res.data_file.info.item_types.push(CDatafileItemType {
                item_type: MapItemTypes::Layer as i32,
                start: layer_item_index,
                num: layer_count,
            });
        }

        // map settings
        {
            let item_index = res.data_file.info.item_offsets.len() as i32;

            res.data_file
                .info
                .item_offsets
                .push(data_items.len() as i32);

            let mut info_data: Vec<u8> = Vec::new();
            let info_settings = CMapItemInfoSettings {
                info: CMapItemInfo {
                    version: 0,
                    author: {
                        let mut author: [u8; 32] = Default::default();
                        let src = map.meta.authors.first().cloned().unwrap_or_default();
                        let src = src.as_bytes();
                        author[0..src.len().min(32)].copy_from_slice(&src[0..src.len().min(32)]);
                        *author.last_mut().unwrap() = 0;
                        if author[0] != 0 {
                            let data_offset = data_compressed_data.len() as i32;
                            let uncompressed_size = author.len();
                            let compressed_data = Self::compress_data(&author);
                            data_compressed_data.extend(compressed_data);
                            let data_index = res.data_file.info.data_offsets.len();
                            res.data_file.info.data_offsets.push(data_offset);
                            res.data_file.info.data_sizes.push(uncompressed_size as i32);
                            data_index as i32
                        } else {
                            -1
                        }
                    },
                    map_version: {
                        let mut map_version: [u8; 16] = Default::default();
                        let src = map.meta.version.as_bytes();
                        map_version[0..src.len().min(16)]
                            .copy_from_slice(&src[0..src.len().min(16)]);
                        *map_version.last_mut().unwrap() = 0;
                        if map_version[0] != 0 {
                            let data_offset = data_compressed_data.len() as i32;
                            let uncompressed_size = map_version.len();
                            let compressed_data = Self::compress_data(&map_version);
                            data_compressed_data.extend(compressed_data);
                            let data_index = res.data_file.info.data_offsets.len();
                            res.data_file.info.data_offsets.push(data_offset);
                            res.data_file.info.data_sizes.push(uncompressed_size as i32);
                            data_index as i32
                        } else {
                            -1
                        }
                    },
                    credits: {
                        let mut credits: [u8; 128] = vec![0; 128].try_into().unwrap();
                        let src = map.meta.credits.as_bytes();
                        credits[0..src.len().min(128)].copy_from_slice(&src[0..src.len().min(128)]);
                        *credits.last_mut().unwrap() = 0;
                        if credits[0] != 0 {
                            let data_offset = data_compressed_data.len() as i32;
                            let uncompressed_size = credits.len();
                            let compressed_data = Self::compress_data(&credits);
                            data_compressed_data.extend(compressed_data);
                            let data_index = res.data_file.info.data_offsets.len();
                            res.data_file.info.data_offsets.push(data_offset);
                            res.data_file.info.data_sizes.push(uncompressed_size as i32);
                            data_index as i32
                        } else {
                            -1
                        }
                    },
                    license: {
                        let mut license: [u8; 32] = Default::default();
                        let src = map.meta.licenses.first().cloned().unwrap_or_default();
                        let src = src.as_bytes();
                        license[0..src.len().min(32)].copy_from_slice(&src[0..src.len().min(32)]);
                        *license.last_mut().unwrap() = 0;
                        if license[0] != 0 {
                            let data_offset = data_compressed_data.len() as i32;
                            let uncompressed_size = license.len();
                            let compressed_data = Self::compress_data(&license);
                            data_compressed_data.extend(compressed_data);
                            let data_index = res.data_file.info.data_offsets.len();
                            res.data_file.info.data_offsets.push(data_offset);
                            res.data_file.info.data_sizes.push(uncompressed_size as i32);
                            data_index as i32
                        } else {
                            -1
                        }
                    },
                },
                settings: {
                    for (cmd, args) in &map.config.commands {
                        let mut setting: [u8; 256] = vec![0; 256].try_into().unwrap();
                        let cmd = format!("{cmd} {args}");
                        let src = cmd.as_bytes();
                        setting[0..src.len().min(256)].copy_from_slice(&src[0..src.len().min(256)]);
                        *setting.last_mut().unwrap() = 0;
                        global_map_settings.push(setting);
                    }
                    let data_offset = data_compressed_data.len() as i32;
                    let uncompressed_size =
                        global_map_settings.len() * std::mem::size_of::<[u8; 256]>();
                    let compressed_data = Self::compress_data(
                        &global_map_settings
                            .into_iter()
                            .map(|v| v.to_vec())
                            .collect::<Vec<_>>()
                            .into_iter()
                            .flatten()
                            .collect::<Vec<_>>(),
                    );
                    data_compressed_data.extend(compressed_data);
                    let data_index = res.data_file.info.data_offsets.len();
                    res.data_file.info.data_offsets.push(data_offset);
                    res.data_file.info.data_sizes.push(uncompressed_size as i32);
                    data_index as i32
                },
            };
            info_settings.write_to_vec(&mut info_data);

            let info_item = CDatafileItem {
                size: info_data.len() as i32,
                type_and_id: ((MapItemTypes::Info as i32) << 16),
            };
            info_item.write_to_vec(&mut data_items);
            data_items.append(&mut info_data);

            res.data_file.info.item_types.push(CDatafileItemType {
                item_type: MapItemTypes::Info as i32,
                start: item_index,
                num: 1,
            });
        }

        // finish
        let types_size =
            res.data_file.info.item_types.len() * std::mem::size_of::<CDatafileItemType>();
        let header_size = std::mem::size_of::<CDatafileHeader>();
        let offset_size = (res.data_file.info.item_offsets.len()
            + res.data_file.info.data_offsets.len()
            + res.data_file.info.data_sizes.len())
            * std::mem::size_of::<i32>(); // ItemOffsets, DataOffsets, DataUncompressedSizes

        let file_size =
            header_size + types_size + offset_size + data_items.len() + data_compressed_data.len();
        let swap_size = file_size - data_compressed_data.len();
        res.data_file.header.size = file_size as u32 - 16;
        res.data_file.header.swap_len = swap_size as u32 - 16;
        res.data_file.header.num_item_types = res.data_file.info.item_types.len() as u32;
        res.data_file.header.num_items = res.data_file.info.item_offsets.len() as u32;
        res.data_file.header.num_raw_data = res.data_file.info.data_offsets.len() as u32;
        res.data_file.header.item_size = data_items.len() as u32;
        res.data_file.header.data_size = data_compressed_data.len() as u32;

        let mut data_all: Vec<u8> = Vec::new();
        res.data_file.header.write_to_vec(&mut data_all);
        // item types
        res.data_file
            .info
            .item_types
            .iter()
            .for_each(|o| o.write_to_vec(&mut data_all));
        // item offsets
        data_all.extend(
            res.data_file
                .info
                .item_offsets
                .iter()
                .flat_map(|o| o.to_le_bytes()),
        );
        // data offsets
        data_all.extend(
            res.data_file
                .info
                .data_offsets
                .iter()
                .flat_map(|o| o.to_le_bytes()),
        );
        // data sizes
        data_all.extend(
            res.data_file
                .info
                .data_sizes
                .iter()
                .flat_map(|o| o.to_le_bytes()),
        );
        data_all.append(&mut data_items);
        data_all.append(&mut data_compressed_data);

        data_all
    }
}
