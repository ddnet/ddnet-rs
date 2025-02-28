use base::hash::generate_hash_for;
use hashlink::LinkedHashMap;
use map::map::{
    animations::AnimBase,
    groups::{
        layers::{
            design::{
                MapLayerQuad, MapLayerQuadsAttrs, MapLayerSound, MapLayerSoundAttrs, MapLayerTile,
                Quad, Sound, SoundShape,
            },
            physics::{
                MapLayerPhysics, MapLayerTilePhysicsBase, MapLayerTilePhysicsSwitch,
                MapLayerTilePhysicsTele, MapLayerTilePhysicsTune,
            },
            tiles::{MapTileLayerAttr, MapTileLayerPhysicsTiles, TileFlags},
        },
        MapGroup, MapGroupAttr, MapGroupPhysicsAttr,
    },
    metadata::Metadata,
    resources::{MapResourceMetaData, MapResourceRef},
};
use math::math::vector::uffixed;
use rand::RngCore;

use crate::{
    actions::{
        actions::{
            ActAddColorAnim, ActAddGroup, ActAddImage, ActAddImage2dArray, ActAddPhysicsTileLayer,
            ActAddPosAnim, ActAddQuadLayer, ActAddRemColorAnim, ActAddRemGroup, ActAddRemImage,
            ActAddRemPhysicsTileLayer, ActAddRemPosAnim, ActAddRemQuadLayer, ActAddRemSoundAnim,
            ActAddRemSoundLayer, ActAddRemTileLayer, ActAddSoundAnim, ActAddSoundLayer,
            ActAddTileLayer, ActChangeDesignLayerName, ActChangeGroupAttr, ActChangeGroupName,
            ActChangePhysicsGroupAttr, ActChangeQuadAttr, ActChangeQuadLayerAttr,
            ActChangeSoundAttr, ActChangeSoundLayerAttr, ActChangeSwitch, ActChangeTeleporter,
            ActChangeTileLayerDesignAttr, ActChangeTuneZone, ActLayerChangeImageIndex,
            ActLayerChangeSoundIndex, ActMoveGroup, ActMoveLayer, ActQuadLayerAddQuads,
            ActQuadLayerAddRemQuads, ActQuadLayerRemQuads, ActRemGroup, ActRemImage,
            ActRemImage2dArray, ActRemPhysicsTileLayer, ActRemQuadLayer, ActRemSoundLayer,
            ActRemTileLayer, ActSetCommands, ActSetMetadata, ActSoundLayerAddRemSounds,
            ActSoundLayerAddSounds, ActSoundLayerRemSounds, ActTileLayerReplTilesBase,
            ActTileLayerReplaceTiles, ActTilePhysicsLayerReplTilesBase,
            ActTilePhysicsLayerReplaceTiles, EditorAction,
        },
        utils::{rem_color_anim, rem_pos_anim, rem_sound_anim},
    },
    map::{EditorLayer, EditorMap, EditorPhysicsLayer},
};

fn move_group_valid(map: &EditorMap) -> Vec<EditorAction> {
    let mut old_is_background = false;
    let new_is_background = rand::rngs::OsRng.next_u64() % 2 == 0;
    if map.groups.background.is_empty() && map.groups.foreground.is_empty() {
        return Default::default();
    } else if !map.groups.background.is_empty() && map.groups.foreground.is_empty() {
        old_is_background = true;
    } else if !map.groups.background.is_empty() && !map.groups.foreground.is_empty() {
        old_is_background = rand::rngs::OsRng.next_u64() % 2 == 0;
    }

    let old_groups = if old_is_background {
        &map.groups.background
    } else {
        &map.groups.foreground
    };
    let new_groups = if new_is_background {
        &map.groups.background
    } else {
        &map.groups.foreground
    };

    let old_group = rand::rngs::OsRng.next_u64() as usize % old_groups.len();
    let new_group = rand::rngs::OsRng.next_u64() as usize % (new_groups.len() + 1);
    let sub_group = if old_is_background == new_is_background && old_group <= new_group {
        1
    } else {
        0
    };

    vec![EditorAction::MoveGroup(ActMoveGroup {
        old_is_background,
        old_group,
        new_is_background,
        new_group: new_group.saturating_sub(sub_group),
    })]
}

fn move_layer_valid(map: &EditorMap) -> Vec<EditorAction> {
    let mut old_is_background = false;
    let mut new_is_background = false;
    if map.groups.background.is_empty() && map.groups.foreground.is_empty() {
        return Default::default();
    } else if !map.groups.background.is_empty() && map.groups.foreground.is_empty() {
        old_is_background = true;
        new_is_background = true;
    } else if !map.groups.background.is_empty() && !map.groups.foreground.is_empty() {
        old_is_background = rand::rngs::OsRng.next_u64() % 2 == 0;
        new_is_background = rand::rngs::OsRng.next_u64() % 2 == 0;
    }

    if map.groups.background.iter().any(|g| !g.layers.is_empty())
        && !map.groups.foreground.iter().any(|g| !g.layers.is_empty())
    {
        old_is_background = true;
    } else if !map.groups.background.iter().any(|g| !g.layers.is_empty())
        && map.groups.foreground.iter().any(|g| !g.layers.is_empty())
    {
        old_is_background = false;
    } else if !map.groups.background.iter().any(|g| !g.layers.is_empty())
        && !map.groups.foreground.iter().any(|g| !g.layers.is_empty())
    {
        return Default::default();
    }

    let old_groups = if old_is_background {
        &map.groups.background
    } else {
        &map.groups.foreground
    };
    let new_groups = if new_is_background {
        &map.groups.background
    } else {
        &map.groups.foreground
    };

    let old_group = {
        let groups = old_groups
            .iter()
            .enumerate()
            .filter(|(_, g)| !g.layers.is_empty())
            .collect::<Vec<_>>();
        let group_index = rand::rngs::OsRng.next_u64() as usize % groups.len();

        groups[group_index].0
    };
    let new_group = rand::rngs::OsRng.next_u64() as usize % new_groups.len();

    let old_group_ref = &old_groups[old_group];
    let new_group_ref = &new_groups[new_group];

    let old_layer = rand::rngs::OsRng.next_u64() as usize % old_group_ref.layers.len();
    let new_layer = rand::rngs::OsRng.next_u64() as usize % (new_group_ref.layers.len() + 1);
    let new_layer_sub = if old_is_background == new_is_background
        && old_group == new_group
        && old_layer <= new_layer
    {
        1
    } else {
        0
    };

    vec![EditorAction::MoveLayer(ActMoveLayer {
        old_is_background,
        old_group,
        old_layer,
        new_is_background,
        new_group,
        new_layer: new_layer.saturating_sub(new_layer_sub),
    })]
}
pub(crate) const VALID_PNG: [u8; 528] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x10, 0x08, 0x04, 0x00, 0x00, 0x00, 0xb5, 0xfa, 0x37,
    0xea, 0x00, 0x00, 0x01, 0x70, 0x69, 0x43, 0x43, 0x50, 0x69, 0x63, 0x63, 0x00, 0x00, 0x78, 0xda,
    0x95, 0xd1, 0x39, 0x48, 0x03, 0x41, 0x14, 0xc6, 0xf1, 0xff, 0x46, 0x45, 0xf1, 0x40, 0xf0, 0x00,
    0x11, 0x8b, 0x2d, 0xa2, 0x95, 0x69, 0x54, 0xc4, 0x32, 0x46, 0x21, 0x08, 0x11, 0x42, 0x8c, 0x90,
    0xa8, 0x85, 0x7b, 0xe4, 0x82, 0xec, 0x26, 0xec, 0x26, 0xd8, 0xa4, 0x14, 0x6c, 0x03, 0x16, 0x1e,
    0x8d, 0x51, 0x0b, 0x1b, 0x6b, 0x6d, 0x2d, 0x6c, 0x05, 0x41, 0xf0, 0x00, 0xb1, 0x17, 0xac, 0x14,
    0x6d, 0x24, 0xac, 0x3b, 0x04, 0x4d, 0x10, 0x22, 0xf8, 0xaa, 0x1f, 0xdf, 0xcc, 0x1b, 0x66, 0xde,
    0x80, 0xa7, 0x92, 0xd5, 0x0c, 0xbb, 0xd5, 0x0f, 0x86, 0x59, 0xb0, 0x22, 0xc1, 0x80, 0x1c, 0x8b,
    0xaf, 0xc8, 0xed, 0xcf, 0x48, 0x0c, 0xd2, 0xc5, 0x10, 0x7d, 0x8a, 0x66, 0xe7, 0x67, 0xc3, 0xe1,
    0x10, 0x4d, 0xeb, 0xe3, 0x16, 0x09, 0xe0, 0xc6, 0x27, 0xce, 0xe2, 0x7f, 0xd5, 0xa3, 0x27, 0x6c,
    0x0d, 0x24, 0x19, 0xf0, 0x6b, 0x79, 0xab, 0xe0, 0x7a, 0x1d, 0x98, 0xde, 0x28, 0xe4, 0x85, 0x77,
    0x81, 0x01, 0x2d, 0xad, 0xe8, 0xae, 0x4f, 0x81, 0x71, 0xcb, 0xbd, 0xa0, 0xeb, 0x7b, 0x91, 0xab,
    0x35, 0xbf, 0x08, 0xa7, 0x84, 0xf1, 0x20, 0x6c, 0x45, 0x23, 0x73, 0xae, 0x07, 0x00, 0x39, 0xd5,
    0x60, 0xb5, 0xc1, 0x5a, 0xda, 0x32, 0x5c, 0x4f, 0x01, 0x5e, 0xdd, 0x30, 0x75, 0xd7, 0xb1, 0x9a,
    0x75, 0xe1, 0x92, 0xb0, 0x91, 0x2d, 0x6a, 0x00, 0x80, 0x04, 0x74, 0x27, 0xcc, 0xe5, 0x25, 0x91,
    0x03, 0x23, 0x04, 0x59, 0x60, 0x91, 0x30, 0x32, 0x2a, 0x45, 0x32, 0x64, 0x29, 0xe0, 0x23, 0x83,
    0x89, 0x8c, 0x4d, 0x84, 0x20, 0x81, 0x26, 0xfd, 0xc3, 0x88, 0xfe, 0x30, 0x45, 0x54, 0xb2, 0x64,
    0xd0, 0x90, 0x99, 0x27, 0x87, 0x81, 0x82, 0xe8, 0x47, 0xfc, 0xc1, 0xef, 0xd9, 0xda, 0xc9, 0xc9,
    0x09, 0x00, 0xa4, 0xee, 0x00, 0xb4, 0x3d, 0x39, 0xce, 0xdb, 0x28, 0xb4, 0x6f, 0x43, 0xb5, 0xec,
    0x38, 0x9f, 0x87, 0x8e, 0x53, 0x3d, 0x82, 0x96, 0x47, 0xb8, 0x30, 0xeb, 0xfd, 0xb9, 0x0a, 0xcc,
    0xbc, 0xbb, 0x79, 0xb9, 0x9e, 0x79, 0x0f, 0xa0, 0x77, 0x13, 0xce, 0x2e, 0xeb, 0x99, 0xba, 0x03,
    0xe7, 0x5b, 0x30, 0xf4, 0x90, 0x57, 0x2c, 0x05, 0x80, 0x16, 0xc0, 0x93, 0x4c, 0xc2, 0xeb, 0x09,
    0xf4, 0xc4, 0xa1, 0xff, 0x1a, 0x3a, 0x57, 0xc5, 0xdc, 0x7e, 0xd6, 0x39, 0xbe, 0x83, 0x68, 0x09,
    0x42, 0x57, 0xb0, 0xb7, 0x0f, 0x63, 0x29, 0xe8, 0x5d, 0x6b, 0xf2, 0xee, 0x8e, 0xc6, 0xb9, 0xfd,
    0xb5, 0xe7, 0x7b, 0x7e, 0x5f, 0x5b, 0xb3, 0x72, 0x9d, 0xbe, 0x98, 0x90, 0xd0, 0x00, 0x00, 0x00,
    0x09, 0x70, 0x48, 0x59, 0x73, 0x00, 0x00, 0x2e, 0x23, 0x00, 0x00, 0x2e, 0x23, 0x01, 0x78, 0xa5,
    0x3f, 0x76, 0x00, 0x00, 0x00, 0x07, 0x74, 0x49, 0x4d, 0x45, 0x07, 0xe9, 0x01, 0x15, 0x0b, 0x18,
    0x30, 0x0f, 0xaf, 0x29, 0xe3, 0x00, 0x00, 0x00, 0x19, 0x74, 0x45, 0x58, 0x74, 0x43, 0x6f, 0x6d,
    0x6d, 0x65, 0x6e, 0x74, 0x00, 0x43, 0x72, 0x65, 0x61, 0x74, 0x65, 0x64, 0x20, 0x77, 0x69, 0x74,
    0x68, 0x20, 0x47, 0x49, 0x4d, 0x50, 0x57, 0x81, 0x0e, 0x17, 0x00, 0x00, 0x00, 0x0e, 0x49, 0x44,
    0x41, 0x54, 0x78, 0xda, 0x63, 0x18, 0x05, 0xa3, 0x00, 0x09, 0x00, 0x00, 0x02, 0x10, 0x00, 0x01,
    0xe1, 0xe8, 0x2a, 0x57, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];
fn add_img_valid(map: &EditorMap) -> Vec<EditorAction> {
    vec![EditorAction::AddImage(ActAddImage {
        base: ActAddRemImage {
            res: MapResourceRef {
                name: format!("dbg{}", rand::rngs::OsRng.next_u64())
                    .as_str()
                    .try_into()
                    .unwrap(),
                meta: MapResourceMetaData {
                    blake3_hash: generate_hash_for(&VALID_PNG),
                    ty: "png".try_into().unwrap(),
                },
                hq_meta: None,
            },
            file: VALID_PNG.to_vec(),
            index: rand::rngs::OsRng.next_u64() as usize % (map.resources.images.len() + 1),
        },
    })]
}

fn add_img_2d_array_valid(map: &EditorMap) -> Vec<EditorAction> {
    vec![EditorAction::AddImage2dArray(ActAddImage2dArray {
        base: ActAddRemImage {
            res: MapResourceRef {
                name: format!("dbg{}", rand::rngs::OsRng.next_u64())
                    .as_str()
                    .try_into()
                    .unwrap(),
                meta: MapResourceMetaData {
                    blake3_hash: generate_hash_for(&VALID_PNG),
                    ty: "png".try_into().unwrap(),
                },
                hq_meta: None,
            },
            file: VALID_PNG.to_vec(),
            index: rand::rngs::OsRng.next_u64() as usize % (map.resources.image_arrays.len() + 1),
        },
    })]
}

fn rem_img_valid(map: &EditorMap) -> Vec<EditorAction> {
    let res = &map.resources.images;
    if res.is_empty() {
        return Default::default();
    }
    let index = rand::rngs::OsRng.next_u64() as usize % res.len();
    let mut actions = vec![];

    for (is_background, group_index, layer_index, layer) in map
        .groups
        .background
        .iter()
        .enumerate()
        .map(|g| (true, g))
        .chain(map.groups.foreground.iter().enumerate().map(|g| (false, g)))
        .flat_map(|(is_background, (g, group))| {
            group
                .layers
                .iter()
                .enumerate()
                .filter_map(move |(l, layer)| match layer {
                    EditorLayer::Abritrary(_) | EditorLayer::Tile(_) | EditorLayer::Sound(_) => {
                        None
                    }
                    EditorLayer::Quad(layer) => Some((is_background, g, l, layer)),
                })
        })
    {
        if layer.layer.attr.image == Some(index) {
            actions.push(EditorAction::LayerChangeImageIndex(
                ActLayerChangeImageIndex {
                    is_background,
                    group_index,
                    layer_index,
                    old_index: layer.layer.attr.image,
                    new_index: None,
                },
            ));
        } else if layer.layer.attr.image.is_some_and(|i| i > index) {
            actions.push(EditorAction::LayerChangeImageIndex(
                ActLayerChangeImageIndex {
                    is_background,
                    group_index,
                    layer_index,
                    old_index: layer.layer.attr.image,
                    new_index: Some(index.saturating_sub(1)),
                },
            ));
        }
    }

    [
        actions,
        vec![EditorAction::RemImage(ActRemImage {
            base: ActAddRemImage {
                res: res[index].def.clone(),
                file: res[index].user.file.to_vec(),
                index,
            },
        })],
    ]
    .concat()
}

fn rem_img_2d_array_valid(map: &EditorMap) -> Vec<EditorAction> {
    let res = &map.resources.image_arrays;
    if res.is_empty() {
        return Default::default();
    }
    let index = rand::rngs::OsRng.next_u64() as usize % res.len();
    let mut actions = vec![];

    for (is_background, group_index, layer_index, layer) in map
        .groups
        .background
        .iter()
        .enumerate()
        .map(|g| (true, g))
        .chain(map.groups.foreground.iter().enumerate().map(|g| (false, g)))
        .flat_map(|(is_background, (g, group))| {
            group
                .layers
                .iter()
                .enumerate()
                .filter_map(move |(l, layer)| match layer {
                    EditorLayer::Abritrary(_) | EditorLayer::Quad(_) | EditorLayer::Sound(_) => {
                        None
                    }
                    EditorLayer::Tile(layer) => Some((is_background, g, l, layer)),
                })
        })
    {
        if layer.layer.attr.image_array == Some(index) {
            actions.push(EditorAction::LayerChangeImageIndex(
                ActLayerChangeImageIndex {
                    is_background,
                    group_index,
                    layer_index,
                    old_index: layer.layer.attr.image_array,
                    new_index: None,
                },
            ));
        } else if layer.layer.attr.image_array.is_some_and(|i| i > index) {
            actions.push(EditorAction::LayerChangeImageIndex(
                ActLayerChangeImageIndex {
                    is_background,
                    group_index,
                    layer_index,
                    old_index: layer.layer.attr.image_array,
                    new_index: Some(layer.layer.attr.image_array.unwrap() - 1),
                },
            ));
        }
    }

    [
        actions,
        vec![EditorAction::RemImage2dArray(ActRemImage2dArray {
            base: ActAddRemImage {
                res: res[index].def.clone(),
                file: res[index].user.file.to_vec(),
                index,
            },
        })],
    ]
    .concat()
}

fn layer_change_image_index_valid(map: &EditorMap) -> Vec<EditorAction> {
    let valid_layers = map
        .groups
        .background
        .iter()
        .enumerate()
        .flat_map(|(g, group)| {
            group
                .layers
                .iter()
                .enumerate()
                .filter_map(|(l, layer)| match layer {
                    EditorLayer::Abritrary(_) => None,
                    EditorLayer::Tile(layer) => {
                        (!map.resources.image_arrays.is_empty()).then(|| {
                            (true, g, l, layer.layer.attr.image_array, {
                                let index = rand::rngs::OsRng.next_u64() as usize
                                    % (map.resources.image_arrays.len() + 1);
                                (index != map.resources.image_arrays.len()).then_some(index)
                            })
                        })
                    }
                    EditorLayer::Quad(layer) => (!map.resources.images.is_empty()).then(|| {
                        (true, g, l, layer.layer.attr.image, {
                            let index = rand::rngs::OsRng.next_u64() as usize
                                % (map.resources.images.len() + 1);
                            (index != map.resources.images.len()).then_some(index)
                        })
                    }),
                    EditorLayer::Sound(_) => None,
                })
                .collect::<Vec<_>>()
        })
        .chain(
            map.groups
                .foreground
                .iter()
                .enumerate()
                .flat_map(|(g, group)| {
                    group
                        .layers
                        .iter()
                        .enumerate()
                        .filter_map(|(l, layer)| match layer {
                            EditorLayer::Abritrary(_) => None,
                            EditorLayer::Tile(layer) => (!map.resources.image_arrays.is_empty())
                                .then(|| {
                                    (false, g, l, layer.layer.attr.image_array, {
                                        let index = rand::rngs::OsRng.next_u64() as usize
                                            % (map.resources.image_arrays.len() + 1);
                                        (index != map.resources.image_arrays.len()).then_some(index)
                                    })
                                }),
                            EditorLayer::Quad(layer) => {
                                (!map.resources.images.is_empty()).then(|| {
                                    (false, g, l, layer.layer.attr.image, {
                                        let index = rand::rngs::OsRng.next_u64() as usize
                                            % (map.resources.images.len() + 1);
                                        (index != map.resources.images.len()).then_some(index)
                                    })
                                })
                            }
                            EditorLayer::Sound(_) => None,
                        })
                        .collect::<Vec<_>>()
                }),
        )
        .collect::<Vec<_>>();
    if valid_layers.is_empty() {
        return Default::default();
    }
    let (is_background, group_index, layer_index, old_index, new_index) =
        valid_layers[rand::rngs::OsRng.next_u64() as usize % valid_layers.len()];
    vec![EditorAction::LayerChangeImageIndex(
        ActLayerChangeImageIndex {
            is_background,
            group_index,
            layer_index,
            old_index,
            new_index,
        },
    )]
}

fn layer_change_sound_index_valid(map: &EditorMap) -> Vec<EditorAction> {
    let valid_layers = map
        .groups
        .background
        .iter()
        .enumerate()
        .flat_map(|(g, group)| {
            group
                .layers
                .iter()
                .enumerate()
                .filter_map(|(l, layer)| match layer {
                    EditorLayer::Abritrary(_) => None,
                    EditorLayer::Tile(_) => None,
                    EditorLayer::Quad(_) => None,
                    EditorLayer::Sound(layer) => (!map.resources.sounds.is_empty()).then(|| {
                        (true, g, l, layer.layer.attr.sound, {
                            let index = rand::rngs::OsRng.next_u64() as usize
                                % (map.resources.sounds.len() + 1);
                            (index != map.resources.sounds.len()).then_some(index)
                        })
                    }),
                })
                .collect::<Vec<_>>()
        })
        .chain(
            map.groups
                .foreground
                .iter()
                .enumerate()
                .flat_map(|(g, group)| {
                    group
                        .layers
                        .iter()
                        .enumerate()
                        .filter_map(|(l, layer)| match layer {
                            EditorLayer::Abritrary(_) => None,
                            EditorLayer::Tile(_) => None,
                            EditorLayer::Quad(_) => None,
                            EditorLayer::Sound(layer) => {
                                (!map.resources.sounds.is_empty()).then(|| {
                                    (false, g, l, layer.layer.attr.sound, {
                                        let index = rand::rngs::OsRng.next_u64() as usize
                                            % (map.resources.sounds.len() + 1);
                                        (index != map.resources.sounds.len()).then_some(index)
                                    })
                                })
                            }
                        })
                        .collect::<Vec<_>>()
                }),
        )
        .collect::<Vec<_>>();
    if valid_layers.is_empty() {
        return Default::default();
    }
    let (is_background, group_index, layer_index, old_index, new_index) =
        valid_layers[rand::rngs::OsRng.next_u64() as usize % valid_layers.len()];
    vec![EditorAction::LayerChangeSoundIndex(
        ActLayerChangeSoundIndex {
            is_background,
            group_index,
            layer_index,
            old_index,
            new_index,
        },
    )]
}

pub(crate) fn quad_layer_add_quads_valid(map: &EditorMap) -> Vec<EditorAction> {
    let valid_layers = map
        .groups
        .background
        .iter()
        .enumerate()
        .flat_map(|(g, group)| {
            group
                .layers
                .iter()
                .enumerate()
                .filter_map(|(l, layer)| match layer {
                    EditorLayer::Abritrary(_) => None,
                    EditorLayer::Tile(_) => None,
                    EditorLayer::Quad(_) => Some((true, g, l)),
                    EditorLayer::Sound(_) => None,
                })
                .collect::<Vec<_>>()
        })
        .chain(
            map.groups
                .foreground
                .iter()
                .enumerate()
                .flat_map(|(g, group)| {
                    group
                        .layers
                        .iter()
                        .enumerate()
                        .filter_map(|(l, layer)| match layer {
                            EditorLayer::Abritrary(_) => None,
                            EditorLayer::Tile(_) => None,
                            EditorLayer::Quad(_) => Some((false, g, l)),
                            EditorLayer::Sound(_) => None,
                        })
                        .collect::<Vec<_>>()
                }),
        )
        .collect::<Vec<_>>();
    if valid_layers.is_empty() {
        return Default::default();
    }
    let (is_background, group_index, layer_index) =
        valid_layers[rand::rngs::OsRng.next_u64() as usize % valid_layers.len()];
    let EditorLayer::Quad(layer) = &if is_background {
        &map.groups.background
    } else {
        &map.groups.foreground
    }[group_index]
        .layers[layer_index]
    else {
        panic!("not a quad layer, check above calculation.")
    };
    vec![EditorAction::QuadLayerAddQuads(ActQuadLayerAddQuads {
        base: ActQuadLayerAddRemQuads {
            is_background,
            group_index,
            layer_index,
            index: rand::rngs::OsRng.next_u64() as usize % (layer.layer.quads.len() + 1),
            quads: {
                let mut res = vec![];

                for _ in 0..(rand::rngs::OsRng.next_u64() % 100) + 1 {
                    res.push(Quad {
                        color_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                            None
                        } else {
                            (!map.animations.color.is_empty()).then(|| {
                                rand::rngs::OsRng.next_u64() as usize % map.animations.color.len()
                            })
                        },
                        pos_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                            None
                        } else {
                            (!map.animations.pos.is_empty()).then(|| {
                                rand::rngs::OsRng.next_u64() as usize % map.animations.pos.len()
                            })
                        },
                        ..Default::default()
                    })
                }

                res
            },
        },
    })]
}

pub(crate) fn sound_layer_add_sounds_valid(map: &EditorMap) -> Vec<EditorAction> {
    let valid_layers = map
        .groups
        .background
        .iter()
        .enumerate()
        .flat_map(|(g, group)| {
            group
                .layers
                .iter()
                .enumerate()
                .filter_map(|(l, layer)| match layer {
                    EditorLayer::Abritrary(_) => None,
                    EditorLayer::Tile(_) => None,
                    EditorLayer::Quad(_) => None,
                    EditorLayer::Sound(_) => Some((true, g, l)),
                })
                .collect::<Vec<_>>()
        })
        .chain(
            map.groups
                .foreground
                .iter()
                .enumerate()
                .flat_map(|(g, group)| {
                    group
                        .layers
                        .iter()
                        .enumerate()
                        .filter_map(|(l, layer)| match layer {
                            EditorLayer::Abritrary(_) => None,
                            EditorLayer::Tile(_) => None,
                            EditorLayer::Quad(_) => None,
                            EditorLayer::Sound(_) => Some((false, g, l)),
                        })
                        .collect::<Vec<_>>()
                }),
        )
        .collect::<Vec<_>>();
    if valid_layers.is_empty() {
        return Default::default();
    }
    let (is_background, group_index, layer_index) =
        valid_layers[rand::rngs::OsRng.next_u64() as usize % valid_layers.len()];
    let EditorLayer::Sound(layer) = &if is_background {
        &map.groups.background
    } else {
        &map.groups.foreground
    }[group_index]
        .layers[layer_index]
    else {
        panic!("not a sound layer, check above calculation.")
    };
    vec![EditorAction::SoundLayerAddSounds(ActSoundLayerAddSounds {
        base: ActSoundLayerAddRemSounds {
            is_background,
            group_index,
            layer_index,
            index: rand::rngs::OsRng.next_u64() as usize % (layer.layer.sounds.len() + 1),
            sounds: {
                let mut res = vec![];

                for _ in 0..(rand::rngs::OsRng.next_u64() % 100) + 1 {
                    res.push(Sound {
                        pos: Default::default(),
                        looped: Default::default(),
                        panning: Default::default(),
                        time_delay: Default::default(),
                        falloff: Default::default(),
                        pos_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                            None
                        } else {
                            (!map.animations.pos.is_empty()).then(|| {
                                rand::rngs::OsRng.next_u64() as usize % map.animations.pos.len()
                            })
                        },
                        pos_anim_offset: Default::default(),
                        sound_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                            None
                        } else {
                            (!map.animations.sound.is_empty()).then(|| {
                                rand::rngs::OsRng.next_u64() as usize % map.animations.sound.len()
                            })
                        },
                        sound_anim_offset: Default::default(),
                        shape: SoundShape::Circle {
                            radius: uffixed::from_num(30),
                        },
                    });
                }

                res
            },
        },
    })]
}

fn quad_layer_rem_quads_valid(map: &EditorMap) -> Vec<EditorAction> {
    let valid_layers = map
        .groups
        .background
        .iter()
        .enumerate()
        .flat_map(|(g, group)| {
            group
                .layers
                .iter()
                .enumerate()
                .filter_map(|(l, layer)| match layer {
                    EditorLayer::Abritrary(_) => None,
                    EditorLayer::Tile(_) => None,
                    EditorLayer::Quad(layer) => {
                        (!layer.layer.quads.is_empty()).then_some((true, g, l))
                    }
                    EditorLayer::Sound(_) => None,
                })
                .collect::<Vec<_>>()
        })
        .chain(
            map.groups
                .foreground
                .iter()
                .enumerate()
                .flat_map(|(g, group)| {
                    group
                        .layers
                        .iter()
                        .enumerate()
                        .filter_map(|(l, layer)| match layer {
                            EditorLayer::Abritrary(_) => None,
                            EditorLayer::Tile(_) => None,
                            EditorLayer::Quad(layer) => {
                                (!layer.layer.quads.is_empty()).then_some((false, g, l))
                            }
                            EditorLayer::Sound(_) => None,
                        })
                        .collect::<Vec<_>>()
                }),
        )
        .collect::<Vec<_>>();
    if valid_layers.is_empty() {
        return Default::default();
    }
    let (is_background, group_index, layer_index) =
        valid_layers[rand::rngs::OsRng.next_u64() as usize % valid_layers.len()];
    let EditorLayer::Quad(layer) = &if is_background {
        &map.groups.background
    } else {
        &map.groups.foreground
    }[group_index]
        .layers[layer_index]
    else {
        panic!("not a quad layer, check above calculation.")
    };
    let index = rand::rngs::OsRng.next_u64() as usize % layer.layer.quads.len();
    let quads = &layer.layer.quads[index..];
    let len = (rand::rngs::OsRng.next_u64() as usize % quads.len()) + 1;
    let quads = &quads[0..len];
    vec![EditorAction::QuadLayerRemQuads(ActQuadLayerRemQuads {
        base: ActQuadLayerAddRemQuads {
            is_background,
            group_index,
            layer_index,
            index,
            quads: quads.to_vec(),
        },
    })]
}

fn sound_layer_rem_sounds_valid(map: &EditorMap) -> Vec<EditorAction> {
    let valid_layers = map
        .groups
        .background
        .iter()
        .enumerate()
        .flat_map(|(g, group)| {
            group
                .layers
                .iter()
                .enumerate()
                .filter_map(|(l, layer)| match layer {
                    EditorLayer::Abritrary(_) => None,
                    EditorLayer::Tile(_) => None,
                    EditorLayer::Sound(layer) => {
                        (!layer.layer.sounds.is_empty()).then_some((true, g, l))
                    }
                    EditorLayer::Quad(_) => None,
                })
                .collect::<Vec<_>>()
        })
        .chain(
            map.groups
                .foreground
                .iter()
                .enumerate()
                .flat_map(|(g, group)| {
                    group
                        .layers
                        .iter()
                        .enumerate()
                        .filter_map(|(l, layer)| match layer {
                            EditorLayer::Abritrary(_) => None,
                            EditorLayer::Tile(_) => None,
                            EditorLayer::Sound(layer) => {
                                (!layer.layer.sounds.is_empty()).then_some((false, g, l))
                            }
                            EditorLayer::Quad(_) => None,
                        })
                        .collect::<Vec<_>>()
                }),
        )
        .collect::<Vec<_>>();
    if valid_layers.is_empty() {
        return Default::default();
    }
    let (is_background, group_index, layer_index) =
        valid_layers[rand::rngs::OsRng.next_u64() as usize % valid_layers.len()];
    let EditorLayer::Sound(layer) = &if is_background {
        &map.groups.background
    } else {
        &map.groups.foreground
    }[group_index]
        .layers[layer_index]
    else {
        panic!("not a sound layer, check above calculation.")
    };
    let index = rand::rngs::OsRng.next_u64() as usize % layer.layer.sounds.len();
    let sounds = &layer.layer.sounds[index..];
    let len = (rand::rngs::OsRng.next_u64() as usize % sounds.len()) + 1;
    let sounds = &sounds[0..len];
    vec![EditorAction::SoundLayerRemSounds(ActSoundLayerRemSounds {
        base: ActSoundLayerAddRemSounds {
            is_background,
            group_index,
            layer_index,
            index,
            sounds: sounds.to_vec(),
        },
    })]
}

pub(crate) fn add_tile_layer_valid(map: &EditorMap) -> Vec<EditorAction> {
    move_group_valid(map)
        .first()
        .and_then(|act| {
            if let EditorAction::MoveGroup(act) = act {
                Some(act)
            } else {
                None
            }
        })
        .map(|act| {
            let group = &if act.old_is_background {
                &map.groups.background
            } else {
                &map.groups.foreground
            }[act.old_group];
            let index = rand::rngs::OsRng.next_u64() as usize % (group.layers.len() + 1);
            let w = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) + 1;
            let h = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) + 1;
            EditorAction::AddTileLayer(ActAddTileLayer {
                base: ActAddRemTileLayer {
                    is_background: act.old_is_background,
                    group_index: act.old_group,
                    index,
                    layer: MapLayerTile {
                        attr: MapTileLayerAttr {
                            width: (w as u16).try_into().unwrap(),
                            height: (h as u16).try_into().unwrap(),
                            color: Default::default(),
                            high_detail: Default::default(),
                            color_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                                None
                            } else {
                                (!map.animations.color.is_empty()).then(|| {
                                    rand::rngs::OsRng.next_u64() as usize
                                        % map.animations.color.len()
                                })
                            },
                            color_anim_offset: Default::default(),
                            image_array: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                                None
                            } else {
                                (!map.resources.image_arrays.is_empty()).then(|| {
                                    rand::rngs::OsRng.next_u64() as usize
                                        % map.resources.image_arrays.len()
                                })
                            },
                        },
                        tiles: vec![Default::default(); (w * h) as usize],
                        name: Default::default(),
                    },
                },
            })
        })
        .into_iter()
        .collect()
}

pub(crate) fn add_quad_layer_valid(map: &EditorMap) -> Vec<EditorAction> {
    move_group_valid(map)
        .first()
        .and_then(|act| {
            if let EditorAction::MoveGroup(act) = act {
                Some(act)
            } else {
                None
            }
        })
        .map(|act| {
            let group = &if act.old_is_background {
                &map.groups.background
            } else {
                &map.groups.foreground
            }[act.old_group];
            let index = rand::rngs::OsRng.next_u64() as usize % (group.layers.len() + 1);
            EditorAction::AddQuadLayer(ActAddQuadLayer {
                base: ActAddRemQuadLayer {
                    is_background: act.old_is_background,
                    group_index: act.old_group,
                    index,
                    layer: MapLayerQuad {
                        attr: MapLayerQuadsAttrs {
                            image: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                                None
                            } else {
                                (!map.resources.images.is_empty()).then(|| {
                                    rand::rngs::OsRng.next_u64() as usize
                                        % map.resources.images.len()
                                })
                            },
                            high_detail: Default::default(),
                        },
                        quads: Default::default(),
                        name: Default::default(),
                    },
                },
            })
        })
        .into_iter()
        .collect()
}

pub fn add_sound_layer_valid(map: &EditorMap) -> Vec<EditorAction> {
    move_group_valid(map)
        .first()
        .and_then(|act| {
            if let EditorAction::MoveGroup(act) = act {
                Some(act)
            } else {
                None
            }
        })
        .map(|act| {
            let group = &if act.old_is_background {
                &map.groups.background
            } else {
                &map.groups.foreground
            }[act.old_group];
            let index = rand::rngs::OsRng.next_u64() as usize % (group.layers.len() + 1);
            EditorAction::AddSoundLayer(ActAddSoundLayer {
                base: ActAddRemSoundLayer {
                    is_background: act.old_is_background,
                    group_index: act.old_group,
                    index,
                    layer: MapLayerSound {
                        attr: MapLayerSoundAttrs {
                            sound: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                                None
                            } else {
                                (!map.resources.sounds.is_empty()).then(|| {
                                    rand::rngs::OsRng.next_u64() as usize
                                        % map.resources.sounds.len()
                                })
                            },
                            high_detail: Default::default(),
                        },
                        sounds: Default::default(),
                        name: Default::default(),
                    },
                },
            })
        })
        .into_iter()
        .collect()
}

fn rem_design_layer_valid(map: &EditorMap) -> Vec<EditorAction> {
    move_layer_valid(map)
        .first()
        .and_then(|act| {
            if let EditorAction::MoveLayer(act) = act {
                Some(act)
            } else {
                None
            }
        })
        .and_then(|act| {
            let layer = &if act.old_is_background {
                &map.groups.background
            } else {
                &map.groups.foreground
            }[act.old_group]
                .layers[act.old_layer];
            match layer {
                EditorLayer::Abritrary(_) => None,
                EditorLayer::Tile(layer) => Some(EditorAction::RemTileLayer(ActRemTileLayer {
                    base: ActAddRemTileLayer {
                        is_background: act.old_is_background,
                        group_index: act.old_group,
                        index: act.old_layer,
                        layer: layer.clone().into(),
                    },
                })),
                EditorLayer::Quad(layer) => Some(EditorAction::RemQuadLayer(ActRemQuadLayer {
                    base: ActAddRemQuadLayer {
                        is_background: act.old_is_background,
                        group_index: act.old_group,
                        index: act.old_layer,
                        layer: layer.clone().into(),
                    },
                })),
                EditorLayer::Sound(layer) => Some(EditorAction::RemSoundLayer(ActRemSoundLayer {
                    base: ActAddRemSoundLayer {
                        is_background: act.old_is_background,
                        group_index: act.old_group,
                        index: act.old_layer,
                        layer: layer.clone().into(),
                    },
                })),
            }
        })
        .into_iter()
        .collect()
}

fn add_physics_tile_layer_valid(map: &EditorMap) -> Vec<EditorAction> {
    // front, tele, speedup, switch, tune
    let mut valid_layers = vec![true, true, true, true, true];
    map.groups.physics.layers.iter().for_each(|l| match l {
        EditorPhysicsLayer::Arbitrary(_) | EditorPhysicsLayer::Game(_) => {
            // ignore
        }
        EditorPhysicsLayer::Front(_) => valid_layers[0] = false,
        EditorPhysicsLayer::Tele(_) => valid_layers[1] = false,
        EditorPhysicsLayer::Speedup(_) => valid_layers[2] = false,
        EditorPhysicsLayer::Switch(_) => valid_layers[3] = false,
        EditorPhysicsLayer::Tune(_) => valid_layers[4] = false,
    });

    let valid_indices = valid_layers
        .into_iter()
        .enumerate()
        .filter(|(_, v)| *v)
        .collect::<Vec<_>>();
    if valid_indices.is_empty() {
        return Default::default();
    }
    let index = rand::rngs::OsRng.next_u64() as usize % valid_indices.len();
    let insert_layer = valid_indices[index].0;
    let len = map.groups.physics.attr.width.get() as usize
        * map.groups.physics.attr.height.get() as usize;

    let index = rand::rngs::OsRng.next_u64() as usize % (map.groups.physics.layers.len() + 1);
    vec![EditorAction::AddPhysicsTileLayer(ActAddPhysicsTileLayer {
        base: ActAddRemPhysicsTileLayer {
            index,
            layer: match insert_layer {
                0 => MapLayerPhysics::Front(MapLayerTilePhysicsBase {
                    tiles: vec![Default::default(); len],
                }),
                1 => MapLayerPhysics::Tele(MapLayerTilePhysicsTele {
                    base: MapLayerTilePhysicsBase {
                        tiles: vec![Default::default(); len],
                    },
                    tele_names: Default::default(),
                }),
                2 => MapLayerPhysics::Speedup(MapLayerTilePhysicsBase {
                    tiles: vec![Default::default(); len],
                }),
                3 => MapLayerPhysics::Switch(MapLayerTilePhysicsSwitch {
                    base: MapLayerTilePhysicsBase {
                        tiles: vec![Default::default(); len],
                    },
                    switch_names: Default::default(),
                }),
                4 => MapLayerPhysics::Tune(MapLayerTilePhysicsTune {
                    base: MapLayerTilePhysicsBase {
                        tiles: vec![Default::default(); len],
                    },
                    tune_zones: Default::default(),
                }),
                _ => panic!("indices over 4 are not implemented."),
            },
        },
    })]
}

fn rem_physics_tile_layer_valid(map: &EditorMap) -> Vec<EditorAction> {
    let valid_layers = map
        .groups
        .physics
        .layers
        .iter()
        .enumerate()
        .filter(|(_, l)| !matches!(l, EditorPhysicsLayer::Game(_)))
        .collect::<Vec<_>>();
    if valid_layers.is_empty() {
        return Default::default();
    }
    let index = rand::rngs::OsRng.next_u64() as usize % valid_layers.len();
    let layer = valid_layers[index].1;
    let index = valid_layers[index].0;
    vec![EditorAction::RemPhysicsTileLayer(ActRemPhysicsTileLayer {
        base: ActAddRemPhysicsTileLayer {
            index,
            layer: layer.clone().into(),
        },
    })]
}

fn tile_layer_replace_tiles_valid(map: &EditorMap) -> Vec<EditorAction> {
    let valid_layers = map
        .groups
        .background
        .iter()
        .enumerate()
        .flat_map(|(g, group)| {
            group
                .layers
                .iter()
                .enumerate()
                .filter_map(|(l, layer)| match layer {
                    EditorLayer::Abritrary(_) => None,
                    EditorLayer::Sound(_) => None,
                    EditorLayer::Tile(layer) => Some((true, g, l, layer)),
                    EditorLayer::Quad(_) => None,
                })
                .collect::<Vec<_>>()
        })
        .chain(
            map.groups
                .foreground
                .iter()
                .enumerate()
                .flat_map(|(g, group)| {
                    group
                        .layers
                        .iter()
                        .enumerate()
                        .filter_map(|(l, layer)| match layer {
                            EditorLayer::Abritrary(_) => None,
                            EditorLayer::Sound(_) => None,
                            EditorLayer::Tile(layer) => Some((false, g, l, layer)),
                            EditorLayer::Quad(_) => None,
                        })
                        .collect::<Vec<_>>()
                }),
        )
        .collect::<Vec<_>>();
    if valid_layers.is_empty() {
        return Default::default();
    }

    let index = rand::rngs::OsRng.next_u64() as usize % valid_layers.len();
    let (is_background, group_index, layer_index, layer) = valid_layers[index];

    let old_tiles = layer.layer.tiles.clone();
    let new_tiles = old_tiles
        .iter()
        .copied()
        .map(|mut t| {
            t.index = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8;
            t.flags = TileFlags::from_bits_truncate(
                (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8,
            );
            t
        })
        .collect();

    vec![EditorAction::TileLayerReplaceTiles(
        ActTileLayerReplaceTiles {
            base: ActTileLayerReplTilesBase {
                is_background,
                group_index,
                layer_index,
                old_tiles,
                new_tiles,
                x: 0,
                y: 0,
                w: layer.layer.attr.width,
                h: layer.layer.attr.height,
            },
        },
    )]
}

fn tile_physics_layer_replace_tiles_valid(map: &EditorMap) -> Vec<EditorAction> {
    let index = rand::rngs::OsRng.next_u64() as usize % map.groups.physics.layers.len();
    let layer = &map.groups.physics.layers[index];

    let (old_tiles, new_tiles) = match layer {
        EditorPhysicsLayer::Arbitrary(_) => {
            return Default::default();
        }
        EditorPhysicsLayer::Game(layer) => (
            MapTileLayerPhysicsTiles::Game(layer.layer.tiles.clone()),
            MapTileLayerPhysicsTiles::Game(
                layer
                    .layer
                    .tiles
                    .clone()
                    .into_iter()
                    .map(|mut t| {
                        t.index = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8;
                        t.flags = TileFlags::from_bits_truncate(
                            (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8,
                        );
                        t
                    })
                    .collect(),
            ),
        ),
        EditorPhysicsLayer::Front(layer) => (
            MapTileLayerPhysicsTiles::Front(layer.layer.tiles.clone()),
            MapTileLayerPhysicsTiles::Front(
                layer
                    .layer
                    .tiles
                    .clone()
                    .into_iter()
                    .map(|mut t| {
                        t.index = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8;
                        t.flags = TileFlags::from_bits_truncate(
                            (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8,
                        );
                        t
                    })
                    .collect(),
            ),
        ),
        EditorPhysicsLayer::Tele(layer) => (
            MapTileLayerPhysicsTiles::Tele(layer.layer.base.tiles.clone()),
            MapTileLayerPhysicsTiles::Tele(
                layer
                    .layer
                    .base
                    .tiles
                    .clone()
                    .into_iter()
                    .map(|mut t| {
                        t.base.index = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8;
                        t.base.flags = TileFlags::from_bits_truncate(
                            (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8,
                        );
                        t.number = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8;
                        t
                    })
                    .collect(),
            ),
        ),
        EditorPhysicsLayer::Speedup(layer) => (
            MapTileLayerPhysicsTiles::Speedup(layer.layer.tiles.clone()),
            MapTileLayerPhysicsTiles::Speedup(
                layer
                    .layer
                    .tiles
                    .clone()
                    .into_iter()
                    .map(|mut t| {
                        t.base.index = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8;
                        t.base.flags = TileFlags::from_bits_truncate(
                            (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8,
                        );
                        t.angle = (rand::rngs::OsRng.next_u64() % u16::MAX as u64) as i16;
                        t.force = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8;
                        t.max_speed = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8;
                        t
                    })
                    .collect(),
            ),
        ),
        EditorPhysicsLayer::Switch(layer) => (
            MapTileLayerPhysicsTiles::Switch(layer.layer.base.tiles.clone()),
            MapTileLayerPhysicsTiles::Switch(
                layer
                    .layer
                    .base
                    .tiles
                    .clone()
                    .into_iter()
                    .map(|mut t| {
                        t.base.index = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8;
                        t.base.flags = TileFlags::from_bits_truncate(
                            (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8,
                        );
                        t.delay = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8;
                        t.number = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8;
                        t
                    })
                    .collect(),
            ),
        ),
        EditorPhysicsLayer::Tune(layer) => (
            MapTileLayerPhysicsTiles::Tune(layer.layer.base.tiles.clone()),
            MapTileLayerPhysicsTiles::Tune(
                layer
                    .layer
                    .base
                    .tiles
                    .clone()
                    .into_iter()
                    .map(|mut t| {
                        t.base.index = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8;
                        t.base.flags = TileFlags::from_bits_truncate(
                            (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8,
                        );
                        t.number = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8;
                        t
                    })
                    .collect(),
            ),
        ),
    };

    vec![EditorAction::TilePhysicsLayerReplaceTiles(
        ActTilePhysicsLayerReplaceTiles {
            base: ActTilePhysicsLayerReplTilesBase {
                layer_index: index,
                old_tiles,
                new_tiles,
                x: 0,
                y: 0,
                w: map.groups.physics.attr.width,
                h: map.groups.physics.attr.height,
            },
        },
    )]
}

fn add_group_valid(map: &EditorMap) -> Vec<EditorAction> {
    let is_background = (rand::rngs::OsRng.next_u64() % 2) == 0;

    let groups = if is_background {
        &map.groups.background
    } else {
        &map.groups.foreground
    };

    let index = rand::rngs::OsRng.next_u64() as usize % (groups.len() + 1);

    vec![EditorAction::AddGroup(ActAddGroup {
        base: ActAddRemGroup {
            is_background,
            index,
            group: MapGroup {
                attr: MapGroupAttr {
                    offset: Default::default(),
                    parallax: Default::default(),
                    clipping: Default::default(),
                },
                layers: Default::default(),
                name: Default::default(),
            },
        },
    })]
}

fn rem_group_valid(map: &EditorMap) -> Vec<EditorAction> {
    let mut is_background = (rand::rngs::OsRng.next_u64() % 2) == 0;

    if map.groups.background.is_empty() && map.groups.foreground.is_empty() {
        return Default::default();
    } else if map.groups.background.is_empty() && !map.groups.foreground.is_empty() {
        is_background = false;
    } else if !map.groups.background.is_empty() && map.groups.foreground.is_empty() {
        is_background = true;
    }

    let groups = if is_background {
        &map.groups.background
    } else {
        &map.groups.foreground
    };

    let index = rand::rngs::OsRng.next_u64() as usize % groups.len();

    vec![EditorAction::RemGroup(ActRemGroup {
        base: ActAddRemGroup {
            is_background,
            index,
            group: groups[index].clone().into(),
        },
    })]
}

fn change_group_attr_valid(map: &EditorMap) -> Vec<EditorAction> {
    let mut is_background = (rand::rngs::OsRng.next_u64() % 2) == 0;

    if map.groups.background.is_empty() && map.groups.foreground.is_empty() {
        return Default::default();
    } else if map.groups.background.is_empty() && !map.groups.foreground.is_empty() {
        is_background = false;
    } else if !map.groups.background.is_empty() && map.groups.foreground.is_empty() {
        is_background = true;
    }

    let groups = if is_background {
        &map.groups.background
    } else {
        &map.groups.foreground
    };

    let index = rand::rngs::OsRng.next_u64() as usize % groups.len();
    let group = &groups[index];

    vec![EditorAction::ChangeGroupAttr(ActChangeGroupAttr {
        is_background,
        group_index: index,
        old_attr: group.attr,
        new_attr: MapGroupAttr {
            offset: Default::default(),
            parallax: Default::default(),
            clipping: Default::default(),
        },
    })]
}
fn change_group_name_valid(map: &EditorMap) -> Vec<EditorAction> {
    let mut is_background = (rand::rngs::OsRng.next_u64() % 2) == 0;

    if map.groups.background.is_empty() && map.groups.foreground.is_empty() {
        return Default::default();
    } else if map.groups.background.is_empty() && !map.groups.foreground.is_empty() {
        is_background = false;
    } else if !map.groups.background.is_empty() && map.groups.foreground.is_empty() {
        is_background = true;
    }

    let groups = if is_background {
        &map.groups.background
    } else {
        &map.groups.foreground
    };

    let index = rand::rngs::OsRng.next_u64() as usize % groups.len();
    let group = &groups[index];

    vec![EditorAction::ChangeGroupName(ActChangeGroupName {
        is_background,
        group_index: index,
        old_name: group.name.clone(),
        new_name: format!("{}", rand::rngs::OsRng.next_u64()),
    })]
}

fn change_physics_group_attr_valid(map: &EditorMap) -> Vec<EditorAction> {
    let w = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) + 1;
    let h = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) + 1;
    let len = w as usize * h as usize;
    let (old_layer_tiles, new_layer_tiles) = map
        .groups
        .physics
        .layers
        .iter()
        .map(|l| match l {
            EditorPhysicsLayer::Arbitrary(b) => (
                MapTileLayerPhysicsTiles::Arbitrary(b.buf.clone()),
                MapTileLayerPhysicsTiles::Arbitrary(Default::default()),
            ),
            EditorPhysicsLayer::Game(layer) => (
                MapTileLayerPhysicsTiles::Game(layer.layer.tiles.clone()),
                MapTileLayerPhysicsTiles::Game(vec![Default::default(); len]),
            ),
            EditorPhysicsLayer::Front(layer) => (
                MapTileLayerPhysicsTiles::Front(layer.layer.tiles.clone()),
                MapTileLayerPhysicsTiles::Front(vec![Default::default(); len]),
            ),
            EditorPhysicsLayer::Tele(layer) => (
                MapTileLayerPhysicsTiles::Tele(layer.layer.base.tiles.clone()),
                MapTileLayerPhysicsTiles::Tele(vec![Default::default(); len]),
            ),
            EditorPhysicsLayer::Speedup(layer) => (
                MapTileLayerPhysicsTiles::Speedup(layer.layer.tiles.clone()),
                MapTileLayerPhysicsTiles::Speedup(vec![Default::default(); len]),
            ),
            EditorPhysicsLayer::Switch(layer) => (
                MapTileLayerPhysicsTiles::Switch(layer.layer.base.tiles.clone()),
                MapTileLayerPhysicsTiles::Switch(vec![Default::default(); len]),
            ),
            EditorPhysicsLayer::Tune(layer) => (
                MapTileLayerPhysicsTiles::Tune(layer.layer.base.tiles.clone()),
                MapTileLayerPhysicsTiles::Tune(vec![Default::default(); len]),
            ),
        })
        .unzip();

    vec![EditorAction::ChangePhysicsGroupAttr(
        ActChangePhysicsGroupAttr {
            old_attr: map.groups.physics.attr,
            new_attr: MapGroupPhysicsAttr {
                width: (w as u16).try_into().unwrap(),
                height: (h as u16).try_into().unwrap(),
            },
            old_layer_tiles,
            new_layer_tiles,
        },
    )]
}

pub(crate) fn change_layer_design_attr_valid(map: &EditorMap) -> Vec<EditorAction> {
    move_layer_valid(map)
        .first()
        .and_then(|act| {
            if let EditorAction::MoveLayer(act) = act {
                Some(act)
            } else {
                None
            }
        })
        .and_then(|act| {
            let layer = &if act.old_is_background {
                &map.groups.background
            } else {
                &map.groups.foreground
            }[act.old_group]
                .layers[act.old_layer];
            match layer {
                EditorLayer::Abritrary(_) => None,
                EditorLayer::Tile(layer) => {
                    let w = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) + 1;
                    let h = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) + 1;
                    let len = w as usize * h as usize;
                    Some(EditorAction::ChangeTileLayerDesignAttr(
                        ActChangeTileLayerDesignAttr {
                            is_background: act.old_is_background,
                            group_index: act.old_group,
                            layer_index: act.old_layer,
                            old_attr: layer.layer.attr,
                            new_attr: MapTileLayerAttr {
                                width: (w as u16).try_into().unwrap(),
                                height: (h as u16).try_into().unwrap(),
                                color: Default::default(),
                                high_detail: Default::default(),
                                color_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                                    None
                                } else {
                                    (!map.animations.color.is_empty()).then(|| {
                                        rand::rngs::OsRng.next_u64() as usize
                                            % map.animations.color.len()
                                    })
                                },
                                color_anim_offset: Default::default(),
                                image_array: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                                    None
                                } else {
                                    (!map.resources.image_arrays.is_empty()).then(|| {
                                        rand::rngs::OsRng.next_u64() as usize
                                            % map.resources.image_arrays.len()
                                    })
                                },
                            },
                            old_tiles: layer.layer.tiles.clone(),
                            new_tiles: vec![Default::default(); len],
                        },
                    ))
                }
                EditorLayer::Quad(layer) => {
                    Some(EditorAction::ChangeQuadLayerAttr(ActChangeQuadLayerAttr {
                        is_background: act.old_is_background,
                        group_index: act.old_group,
                        layer_index: act.old_layer,
                        old_attr: layer.layer.attr,
                        new_attr: MapLayerQuadsAttrs {
                            image: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                                None
                            } else {
                                (!map.resources.images.is_empty()).then(|| {
                                    rand::rngs::OsRng.next_u64() as usize
                                        % map.resources.images.len()
                                })
                            },
                            high_detail: Default::default(),
                        },
                    }))
                }
                EditorLayer::Sound(layer) => Some(EditorAction::ChangeSoundLayerAttr(
                    ActChangeSoundLayerAttr {
                        is_background: act.old_is_background,
                        group_index: act.old_group,
                        layer_index: act.old_layer,
                        old_attr: layer.layer.attr,
                        new_attr: MapLayerSoundAttrs {
                            sound: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                                None
                            } else {
                                (!map.resources.sounds.is_empty()).then(|| {
                                    rand::rngs::OsRng.next_u64() as usize
                                        % map.resources.sounds.len()
                                })
                            },
                            high_detail: Default::default(),
                        },
                    },
                )),
            }
        })
        .into_iter()
        .collect()
}
fn change_design_layer_name_valid(map: &EditorMap) -> Vec<EditorAction> {
    move_layer_valid(map)
        .first()
        .and_then(|act| {
            if let EditorAction::MoveLayer(act) = act {
                Some(act)
            } else {
                None
            }
        })
        .map(|act| {
            let layer = &if act.old_is_background {
                &map.groups.background
            } else {
                &map.groups.foreground
            }[act.old_group]
                .layers[act.old_layer];
            EditorAction::ChangeDesignLayerName(ActChangeDesignLayerName {
                is_background: act.old_is_background,
                group_index: act.old_group,
                layer_index: act.old_layer,
                old_name: layer.name().to_string(),
                new_name: format!("{}", rand::rngs::OsRng.next_u64()),
            })
        })
        .into_iter()
        .collect()
}

pub(crate) fn change_quad_attr_valid(map: &EditorMap) -> Vec<EditorAction> {
    let valid_layers: Vec<_> = map
        .groups
        .background
        .iter()
        .enumerate()
        .map(|g| (true, g))
        .chain(map.groups.foreground.iter().enumerate().map(|g| (false, g)))
        .flat_map(|(is_background, (g, group))| {
            group
                .layers
                .iter()
                .enumerate()
                .filter_map(move |(l, layer)| match layer {
                    EditorLayer::Abritrary(_) | EditorLayer::Tile(_) | EditorLayer::Sound(_) => {
                        None
                    }
                    EditorLayer::Quad(layer) => {
                        (!layer.layer.quads.is_empty()).then_some((is_background, g, l, layer))
                    }
                })
        })
        .collect();
    if valid_layers.is_empty() {
        return Default::default();
    }
    let index = rand::rngs::OsRng.next_u64() as usize % valid_layers.len();
    let (is_background, group_index, layer_index, layer) = valid_layers[index];

    let index = rand::rngs::OsRng.next_u64() as usize % layer.layer.quads.len();
    let quad = layer.layer.quads[index];

    vec![EditorAction::ChangeQuadAttr(Box::new(ActChangeQuadAttr {
        is_background,
        group_index,
        layer_index,
        index,
        old_attr: quad,
        new_attr: Quad {
            points: Default::default(),
            colors: Default::default(),
            tex_coords: Default::default(),
            pos_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                None
            } else {
                (!map.animations.pos.is_empty())
                    .then(|| rand::rngs::OsRng.next_u64() as usize % map.animations.pos.len())
            },
            pos_anim_offset: Default::default(),
            color_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                None
            } else {
                (!map.animations.color.is_empty())
                    .then(|| rand::rngs::OsRng.next_u64() as usize % map.animations.color.len())
            },
            color_anim_offset: Default::default(),
        },
    }))]
}
pub(crate) fn change_sound_attr_valid(map: &EditorMap) -> Vec<EditorAction> {
    let valid_layers: Vec<_> = map
        .groups
        .background
        .iter()
        .enumerate()
        .map(|g| (true, g))
        .chain(map.groups.foreground.iter().enumerate().map(|g| (false, g)))
        .flat_map(|(is_background, (g, group))| {
            group
                .layers
                .iter()
                .enumerate()
                .filter_map(move |(l, layer)| match layer {
                    EditorLayer::Abritrary(_) | EditorLayer::Tile(_) | EditorLayer::Quad(_) => None,
                    EditorLayer::Sound(layer) => {
                        (!layer.layer.sounds.is_empty()).then_some((is_background, g, l, layer))
                    }
                })
        })
        .collect();
    if valid_layers.is_empty() {
        return Default::default();
    }
    let index = rand::rngs::OsRng.next_u64() as usize % valid_layers.len();
    let (is_background, group_index, layer_index, layer) = valid_layers[index];

    let index = rand::rngs::OsRng.next_u64() as usize % layer.layer.sounds.len();
    let sound = layer.layer.sounds[index];

    vec![EditorAction::ChangeSoundAttr(ActChangeSoundAttr {
        is_background,
        group_index,
        layer_index,
        index,
        old_attr: sound,
        new_attr: Sound {
            pos: Default::default(),
            looped: Default::default(),
            panning: Default::default(),
            time_delay: Default::default(),
            falloff: Default::default(),
            pos_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                None
            } else {
                (!map.animations.pos.is_empty())
                    .then(|| rand::rngs::OsRng.next_u64() as usize % map.animations.pos.len())
            },
            pos_anim_offset: Default::default(),
            sound_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                None
            } else {
                (!map.animations.sound.is_empty())
                    .then(|| rand::rngs::OsRng.next_u64() as usize % map.animations.sound.len())
            },
            sound_anim_offset: Default::default(),
            shape: SoundShape::Circle {
                radius: uffixed::from_num(30),
            },
        },
    })]
}

fn change_teleport_valid(map: &EditorMap) -> Vec<EditorAction> {
    let Some(layer) = map.groups.physics.layers.iter().find_map(|l| {
        if let EditorPhysicsLayer::Tele(l) = l {
            Some(l)
        } else {
            None
        }
    }) else {
        return Default::default();
    };
    let index = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8;
    vec![EditorAction::ChangeTeleporter(ActChangeTeleporter {
        index,
        old_name: layer
            .layer
            .tele_names
            .get(&index)
            .cloned()
            .unwrap_or_default(),
        new_name: "created".into(),
    })]
}
fn change_switch_valid(map: &EditorMap) -> Vec<EditorAction> {
    let Some(layer) = map.groups.physics.layers.iter().find_map(|l| {
        if let EditorPhysicsLayer::Switch(l) = l {
            Some(l)
        } else {
            None
        }
    }) else {
        return Default::default();
    };
    let index = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8;
    vec![EditorAction::ChangeSwitch(ActChangeSwitch {
        index,
        old_name: layer
            .layer
            .switch_names
            .get(&index)
            .cloned()
            .unwrap_or_default(),
        new_name: "created".into(),
    })]
}
fn change_tune_zone_valid(map: &EditorMap) -> Vec<EditorAction> {
    let Some(layer) = map.groups.physics.layers.iter().find_map(|l| {
        if let EditorPhysicsLayer::Tune(l) = l {
            Some(l)
        } else {
            None
        }
    }) else {
        return Default::default();
    };
    let index = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8;
    vec![EditorAction::ChangeTuneZone(ActChangeTuneZone {
        index,
        old_name: layer
            .layer
            .tune_zones
            .get(&index)
            .map(|t| t.name.clone())
            .unwrap_or_default(),
        new_name: "created".into(),
        old_tunes: layer
            .layer
            .tune_zones
            .get(&index)
            .map(|t| t.tunes.clone())
            .unwrap_or_default(),
        new_tunes: Default::default(),
    })]
}

fn add_pos_anim_valid(map: &EditorMap) -> Vec<EditorAction> {
    let index = rand::rngs::OsRng.next_u64() as usize % (map.animations.pos.len() + 1);
    vec![EditorAction::AddPosAnim(ActAddPosAnim {
        base: ActAddRemPosAnim {
            index,
            anim: AnimBase {
                name: "".to_string(),
                points: Default::default(),
                synchronized: Default::default(),
            },
        },
    })]
}
fn rem_pos_anim_valid(map: &EditorMap) -> Vec<EditorAction> {
    let anims = &map.animations.pos;
    if anims.is_empty() {
        return Default::default();
    }
    let index = rand::rngs::OsRng.next_u64() as usize % anims.len();
    rem_pos_anim(anims, &map.groups, index)
}

fn add_color_anim_valid(map: &EditorMap) -> Vec<EditorAction> {
    let index = rand::rngs::OsRng.next_u64() as usize % (map.animations.color.len() + 1);
    vec![EditorAction::AddColorAnim(ActAddColorAnim {
        base: ActAddRemColorAnim {
            index,
            anim: AnimBase {
                name: "".to_string(),
                points: Default::default(),
                synchronized: Default::default(),
            },
        },
    })]
}
fn rem_color_anim_valid(map: &EditorMap) -> Vec<EditorAction> {
    let anims = &map.animations.color;
    if anims.is_empty() {
        return Default::default();
    }
    let index = rand::rngs::OsRng.next_u64() as usize % anims.len();
    rem_color_anim(anims, &map.groups, index)
}

fn add_sound_anim_valid(map: &EditorMap) -> Vec<EditorAction> {
    let index = rand::rngs::OsRng.next_u64() as usize % (map.animations.sound.len() + 1);
    vec![EditorAction::AddSoundAnim(ActAddSoundAnim {
        base: ActAddRemSoundAnim {
            index,
            anim: AnimBase {
                name: "".to_string(),
                points: Default::default(),
                synchronized: Default::default(),
            },
        },
    })]
}
fn rem_sound_anim_valid(map: &EditorMap) -> Vec<EditorAction> {
    let anims = &map.animations.sound;
    if anims.is_empty() {
        return Default::default();
    }
    let index = rand::rngs::OsRng.next_u64() as usize % anims.len();
    rem_sound_anim(anims, &map.groups, index)
}

fn set_commands_valid(map: &EditorMap) -> Vec<EditorAction> {
    vec![EditorAction::SetCommands(ActSetCommands {
        old_commands: map.config.def.commands.clone(),
        new_commands: {
            let mut cmds: LinkedHashMap<_, _> = Default::default();

            for _ in 0..rand::rngs::OsRng.next_u64() % 20 {
                cmds.insert(
                    format!("{}", rand::rngs::OsRng.next_u64()),
                    format!("{}", rand::rngs::OsRng.next_u64()),
                );
            }

            cmds
        },
    })]
}

fn set_metadata_valid(map: &EditorMap) -> Vec<EditorAction> {
    vec![EditorAction::SetMetadata(ActSetMetadata {
        old_meta: map.meta.def.clone(),
        new_meta: Metadata {
            authors: {
                let mut s: Vec<_> = Default::default();

                for _ in 0..rand::rngs::OsRng.next_u64() % 5 {
                    s.push(format!("{}", rand::rngs::OsRng.next_u64()));
                }

                s
            },
            licenses: {
                let mut s: Vec<_> = Default::default();

                for _ in 0..rand::rngs::OsRng.next_u64() % 5 {
                    s.push(format!("{}", rand::rngs::OsRng.next_u64()));
                }

                s
            },
            version: format!("{}", rand::rngs::OsRng.next_u64()),
            credits: format!("{}", rand::rngs::OsRng.next_u64()),
            memo: format!("{}", rand::rngs::OsRng.next_u64()),
        },
    })]
}

pub fn random_valid_action(map: &EditorMap) -> Vec<EditorAction> {
    // must match the last value in the `match` + 1
    const TOTAL_ACTIONS: u64 = 46;
    loop {
        match match rand::rngs::OsRng.next_u64() % TOTAL_ACTIONS {
            0 => move_group_valid(map),
            1 => move_layer_valid(map),
            2 => add_img_valid(map),
            3 => add_img_2d_array_valid(map),
            // reserved for add_sound_valid
            4 => Default::default(),
            5 => rem_img_valid(map),
            6 => rem_img_2d_array_valid(map),
            // reserved for rem_sound_valid
            7 => Default::default(),
            8 => layer_change_image_index_valid(map),
            9 => layer_change_sound_index_valid(map),
            10 => quad_layer_add_quads_valid(map),
            11 => sound_layer_add_sounds_valid(map),
            12 => quad_layer_rem_quads_valid(map),
            13 => sound_layer_rem_sounds_valid(map),
            14 => add_tile_layer_valid(map),
            15 => add_quad_layer_valid(map),
            16 => add_sound_layer_valid(map),
            17..=19 => rem_design_layer_valid(map),
            20 => add_physics_tile_layer_valid(map),
            21 => rem_physics_tile_layer_valid(map),
            22 => tile_layer_replace_tiles_valid(map),
            23 => tile_physics_layer_replace_tiles_valid(map),
            24 => add_group_valid(map),
            25 => rem_group_valid(map),
            26 => change_group_attr_valid(map),
            27 => change_group_name_valid(map),
            28 => change_physics_group_attr_valid(map),
            29..=31 => change_layer_design_attr_valid(map),
            32 => change_design_layer_name_valid(map),
            33 => change_quad_attr_valid(map),
            34 => change_sound_attr_valid(map),
            35 => change_teleport_valid(map),
            36 => change_switch_valid(map),
            37 => change_tune_zone_valid(map),
            38 => add_pos_anim_valid(map),
            39 => rem_pos_anim_valid(map),
            40 => add_color_anim_valid(map),
            41 => rem_color_anim_valid(map),
            42 => add_sound_anim_valid(map),
            43 => rem_sound_anim_valid(map),
            44 => set_commands_valid(map),
            45 => set_metadata_valid(map),
            _ => panic!("unsupported action count"),
        } {
            act if !act.is_empty() => return act,
            _ => {
                // continue
            }
        }
    }
}
