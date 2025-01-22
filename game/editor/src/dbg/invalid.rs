use std::time::Duration;

use base::hash::generate_hash_for;
use hashlink::LinkedHashMap;
use map::map::{
    animations::{AnimBase, AnimPoint, AnimPointCurveType},
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
            tiles::{MapTileLayerAttr, MapTileLayerPhysicsTiles},
        },
        MapGroup, MapGroupAttr, MapGroupPhysicsAttr,
    },
    resources::{MapResourceMetaData, MapResourceRef},
};
use math::math::vector::uffixed;
use rand::RngCore;

use crate::{
    actions::actions::{
        ActAddColorAnim, ActAddGroup, ActAddImage, ActAddImage2dArray, ActAddPhysicsTileLayer,
        ActAddPosAnim, ActAddQuadLayer, ActAddRemColorAnim, ActAddRemGroup, ActAddRemImage,
        ActAddRemPhysicsTileLayer, ActAddRemPosAnim, ActAddRemQuadLayer, ActAddRemSoundAnim,
        ActAddRemSoundLayer, ActAddRemTileLayer, ActAddSoundAnim, ActAddSoundLayer,
        ActAddTileLayer, ActChangeDesignLayerName, ActChangeGroupAttr, ActChangeGroupName,
        ActChangePhysicsGroupAttr, ActChangeQuadAttr, ActChangeQuadLayerAttr, ActChangeSoundAttr,
        ActChangeSoundLayerAttr, ActChangeSwitch, ActChangeTeleporter,
        ActChangeTileLayerDesignAttr, ActChangeTuneZone, ActLayerChangeImageIndex,
        ActLayerChangeSoundIndex, ActMoveGroup, ActMoveLayer, ActQuadLayerAddQuads,
        ActQuadLayerAddRemQuads, ActQuadLayerRemQuads, ActRemColorAnim, ActRemGroup, ActRemImage,
        ActRemImage2dArray, ActRemPhysicsTileLayer, ActRemPosAnim, ActRemQuadLayer,
        ActRemSoundAnim, ActRemSoundLayer, ActRemTileLayer, ActSetCommands,
        ActSoundLayerAddRemSounds, ActSoundLayerAddSounds, ActSoundLayerRemSounds,
        ActTileLayerReplTilesBase, ActTileLayerReplaceTiles, ActTilePhysicsLayerReplTilesBase,
        ActTilePhysicsLayerReplaceTiles, EditorAction,
    },
    dbg::valid::{
        add_quad_layer_valid, add_sound_layer_valid, add_tile_layer_valid,
        change_layer_design_attr_valid, change_quad_attr_valid, change_sound_attr_valid,
        sound_layer_add_sounds_valid, VALID_PNG,
    },
    map::EditorMap,
};

use super::valid::quad_layer_add_quads_valid;

fn move_group_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    vec![EditorAction::MoveGroup(ActMoveGroup {
        old_is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
        old_group: rand::rngs::OsRng.next_u64() as usize,
        new_is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
        new_group: rand::rngs::OsRng.next_u64() as usize,
    })]
}

fn move_layer_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    vec![EditorAction::MoveLayer(ActMoveLayer {
        old_is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
        old_group: rand::rngs::OsRng.next_u64() as usize,
        old_layer: rand::rngs::OsRng.next_u64() as usize,
        new_is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
        new_group: rand::rngs::OsRng.next_u64() as usize,
        new_layer: rand::rngs::OsRng.next_u64() as usize,
    })]
}
const INVALID_PNG: [u8; 5] = [0x89, 0x50, 0x4e, 0x47, 0x0d];
fn add_img_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    vec![EditorAction::AddImage(ActAddImage {
        base: ActAddRemImage {
            res: MapResourceRef {
                name: format!("dbg{}", rand::rngs::OsRng.next_u64())
                    .as_str()
                    .try_into()
                    .unwrap(),
                meta: MapResourceMetaData {
                    blake3_hash: if (rand::rngs::OsRng.next_u64() % 2) == 0 {
                        generate_hash_for(&INVALID_PNG)
                    } else if (rand::rngs::OsRng.next_u64() % 2) == 0 {
                        generate_hash_for(&VALID_PNG)
                    } else {
                        Default::default()
                    },
                    ty: if (rand::rngs::OsRng.next_u64() % 2) == 0 {
                        "png".try_into().unwrap()
                    } else {
                        Default::default()
                    },
                },
                hq_meta: if (rand::rngs::OsRng.next_u64() % 2) == 0 {
                    None
                } else {
                    Some(MapResourceMetaData {
                        blake3_hash: Default::default(),
                        ty: Default::default(),
                    })
                },
            },
            file: if (rand::rngs::OsRng.next_u64() % 2) == 0 {
                VALID_PNG.to_vec()
            } else {
                INVALID_PNG.to_vec()
            },
            index: rand::rngs::OsRng.next_u64() as usize,
        },
    })]
}

fn add_img_2d_array_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    vec![EditorAction::AddImage2dArray(ActAddImage2dArray {
        base: ActAddRemImage {
            res: MapResourceRef {
                name: format!("dbg{}", rand::rngs::OsRng.next_u64())
                    .as_str()
                    .try_into()
                    .unwrap(),
                meta: MapResourceMetaData {
                    blake3_hash: if (rand::rngs::OsRng.next_u64() % 2) == 0 {
                        generate_hash_for(&INVALID_PNG)
                    } else {
                        Default::default()
                    },
                    ty: if (rand::rngs::OsRng.next_u64() % 2) == 0 {
                        "png".try_into().unwrap()
                    } else {
                        Default::default()
                    },
                },
                hq_meta: if (rand::rngs::OsRng.next_u64() % 2) == 0 {
                    None
                } else {
                    Some(MapResourceMetaData {
                        blake3_hash: Default::default(),
                        ty: Default::default(),
                    })
                },
            },
            file: if (rand::rngs::OsRng.next_u64() % 2) == 0 {
                VALID_PNG.to_vec()
            } else {
                INVALID_PNG.to_vec()
            },
            index: rand::rngs::OsRng.next_u64() as usize,
        },
    })]
}

fn rem_img_invalid(map: &EditorMap) -> Vec<EditorAction> {
    vec![EditorAction::RemImage(ActRemImage {
        base: ActAddRemImage {
            res: if let Some(res) = map
                .resources
                .images
                .get(rand::rngs::OsRng.next_u64() as usize)
            {
                res.def.clone()
            } else {
                MapResourceRef {
                    name: format!("dbg{}", rand::rngs::OsRng.next_u64())
                        .as_str()
                        .try_into()
                        .unwrap(),
                    meta: MapResourceMetaData {
                        blake3_hash: Default::default(),
                        ty: Default::default(),
                    },
                    hq_meta: None,
                }
            },
            file: Default::default(),
            index: rand::rngs::OsRng.next_u64() as usize,
        },
    })]
}

fn rem_img_2d_array_invalid(map: &EditorMap) -> Vec<EditorAction> {
    vec![EditorAction::RemImage2dArray(ActRemImage2dArray {
        base: ActAddRemImage {
            res: if let Some(res) = map
                .resources
                .image_arrays
                .get(rand::rngs::OsRng.next_u64() as usize)
            {
                res.def.clone()
            } else {
                MapResourceRef {
                    name: format!("dbg{}", rand::rngs::OsRng.next_u64())
                        .as_str()
                        .try_into()
                        .unwrap(),
                    meta: MapResourceMetaData {
                        blake3_hash: Default::default(),
                        ty: Default::default(),
                    },
                    hq_meta: None,
                }
            },
            file: Default::default(),
            index: rand::rngs::OsRng.next_u64() as usize,
        },
    })]
}

fn layer_change_image_index_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    vec![EditorAction::LayerChangeImageIndex(
        ActLayerChangeImageIndex {
            is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
            group_index: rand::rngs::OsRng.next_u64() as usize,
            layer_index: rand::rngs::OsRng.next_u64() as usize,
            old_index: (rand::rngs::OsRng.next_u64() % 2 == 0)
                .then_some(rand::rngs::OsRng.next_u64() as usize),
            new_index: (rand::rngs::OsRng.next_u64() % 2 == 0)
                .then_some(rand::rngs::OsRng.next_u64() as usize),
        },
    )]
}

fn layer_change_sound_index_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    vec![EditorAction::LayerChangeSoundIndex(
        ActLayerChangeSoundIndex {
            is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
            group_index: rand::rngs::OsRng.next_u64() as usize,
            layer_index: rand::rngs::OsRng.next_u64() as usize,
            old_index: (rand::rngs::OsRng.next_u64() % 2 == 0)
                .then_some(rand::rngs::OsRng.next_u64() as usize),
            new_index: (rand::rngs::OsRng.next_u64() % 2 == 0)
                .then_some(rand::rngs::OsRng.next_u64() as usize),
        },
    )]
}

fn quad_layer_add_quads_invalid(map: &EditorMap) -> Vec<EditorAction> {
    // do a semi valid invalid action
    if rand::rngs::OsRng.next_u64() % 2 == 0 {
        let Some(EditorAction::QuadLayerAddQuads(mut act)) = quad_layer_add_quads_valid(map).pop()
        else {
            return Default::default();
        };
        for q in act.base.quads.iter_mut() {
            q.color_anim = Some(rand::rngs::OsRng.next_u64() as usize);
            q.pos_anim = Some(rand::rngs::OsRng.next_u64() as usize);
        }
        vec![EditorAction::QuadLayerAddQuads(act)]
    } else {
        vec![EditorAction::QuadLayerAddQuads(ActQuadLayerAddQuads {
            base: ActQuadLayerAddRemQuads {
                is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
                group_index: rand::rngs::OsRng.next_u64() as usize,
                layer_index: rand::rngs::OsRng.next_u64() as usize,
                index: rand::rngs::OsRng.next_u64() as usize,
                quads: {
                    let mut res = vec![];

                    for _ in 0..(rand::rngs::OsRng.next_u64() % 100) + 1 {
                        res.push(Quad::default())
                    }

                    res
                },
            },
        })]
    }
}

fn sound_layer_add_sounds_invalid(map: &EditorMap) -> Vec<EditorAction> {
    // do a semi valid invalid action
    if rand::rngs::OsRng.next_u64() % 2 == 0 {
        let Some(EditorAction::SoundLayerAddSounds(mut act)) =
            sound_layer_add_sounds_valid(map).pop()
        else {
            return Default::default();
        };
        for q in act.base.sounds.iter_mut() {
            q.sound_anim = Some(rand::rngs::OsRng.next_u64() as usize);
            q.pos_anim = Some(rand::rngs::OsRng.next_u64() as usize);
        }
        vec![EditorAction::SoundLayerAddSounds(act)]
    } else {
        vec![EditorAction::SoundLayerAddSounds(ActSoundLayerAddSounds {
            base: ActSoundLayerAddRemSounds {
                is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
                group_index: rand::rngs::OsRng.next_u64() as usize,
                layer_index: rand::rngs::OsRng.next_u64() as usize,
                index: rand::rngs::OsRng.next_u64() as usize,
                sounds: {
                    let mut res = vec![];

                    for _ in 0..(rand::rngs::OsRng.next_u64() % 100) + 1 {
                        res.push(Sound {
                            pos: Default::default(),
                            looped: Default::default(),
                            panning: Default::default(),
                            time_delay: Default::default(),
                            falloff: Default::default(),
                            pos_anim: Default::default(),
                            pos_anim_offset: Default::default(),
                            sound_anim: Default::default(),
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
}

fn quad_layer_rem_quads_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    vec![EditorAction::QuadLayerRemQuads(ActQuadLayerRemQuads {
        base: ActQuadLayerAddRemQuads {
            is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
            group_index: rand::rngs::OsRng.next_u64() as usize,
            layer_index: rand::rngs::OsRng.next_u64() as usize,
            index: rand::rngs::OsRng.next_u64() as usize,
            quads: {
                let mut res = vec![];

                for _ in 0..(rand::rngs::OsRng.next_u64() % 100) + 1 {
                    res.push(Quad::default())
                }

                res
            },
        },
    })]
}

fn sound_layer_rem_sounds_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    vec![EditorAction::SoundLayerRemSounds(ActSoundLayerRemSounds {
        base: ActSoundLayerAddRemSounds {
            is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
            group_index: rand::rngs::OsRng.next_u64() as usize,
            layer_index: rand::rngs::OsRng.next_u64() as usize,
            index: rand::rngs::OsRng.next_u64() as usize,
            sounds: {
                let mut res = vec![];

                for _ in 0..(rand::rngs::OsRng.next_u64() % 100) + 1 {
                    res.push(Sound {
                        pos: Default::default(),
                        looped: Default::default(),
                        panning: Default::default(),
                        time_delay: Default::default(),
                        falloff: Default::default(),
                        pos_anim: Default::default(),
                        pos_anim_offset: Default::default(),
                        sound_anim: Default::default(),
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

fn add_tile_layer_invalid(map: &EditorMap) -> Vec<EditorAction> {
    // do a semi valid invalid action
    if rand::rngs::OsRng.next_u64() % 2 == 0 {
        let Some(EditorAction::AddTileLayer(mut act)) = add_tile_layer_valid(map).pop() else {
            return Default::default();
        };
        act.base.layer.attr.image_array = Some(rand::rngs::OsRng.next_u64() as usize);

        vec![EditorAction::AddTileLayer(act)]
    } else {
        vec![EditorAction::AddTileLayer(ActAddTileLayer {
            base: ActAddRemTileLayer {
                is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
                group_index: rand::rngs::OsRng.next_u64() as usize,
                index: rand::rngs::OsRng.next_u64() as usize,
                layer: MapLayerTile {
                    attr: MapTileLayerAttr {
                        width: ((rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u16 + 1)
                            .try_into()
                            .unwrap(),
                        height: ((rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u16 + 1)
                            .try_into()
                            .unwrap(),
                        color: Default::default(),
                        high_detail: Default::default(),
                        color_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                            Default::default()
                        } else {
                            Some(rand::rngs::OsRng.next_u64() as usize)
                        },
                        color_anim_offset: Default::default(),
                        image_array: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                            Default::default()
                        } else {
                            Some(rand::rngs::OsRng.next_u64() as usize)
                        },
                    },
                    tiles: vec![
                        Default::default();
                        ((rand::rngs::OsRng.next_u64() % u8::MAX as u64)
                            * (rand::rngs::OsRng.next_u64() % u8::MAX as u64))
                            as usize
                    ],
                    name: Default::default(),
                },
            },
        })]
    }
}

fn add_quad_layer_invalid(map: &EditorMap) -> Vec<EditorAction> {
    // do a semi valid invalid action
    if rand::rngs::OsRng.next_u64() % 2 == 0 {
        let Some(EditorAction::AddQuadLayer(mut act)) = add_quad_layer_valid(map).pop() else {
            return Default::default();
        };
        if rand::rngs::OsRng.next_u64() % 2 == 0 {
            act.base.layer.attr.image = Some(rand::rngs::OsRng.next_u64() as usize);
        } else {
            for q in act.base.layer.quads.iter_mut() {
                q.color_anim = Some(rand::rngs::OsRng.next_u64() as usize);
                q.pos_anim = Some(rand::rngs::OsRng.next_u64() as usize);
            }
        }
        vec![EditorAction::AddQuadLayer(act)]
    } else {
        vec![EditorAction::AddQuadLayer(ActAddQuadLayer {
            base: ActAddRemQuadLayer {
                is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
                group_index: rand::rngs::OsRng.next_u64() as usize,
                index: rand::rngs::OsRng.next_u64() as usize,
                layer: MapLayerQuad {
                    attr: MapLayerQuadsAttrs {
                        image: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                            Default::default()
                        } else {
                            Some(rand::rngs::OsRng.next_u64() as usize)
                        },
                        high_detail: Default::default(),
                    },
                    quads: Default::default(),
                    name: Default::default(),
                },
            },
        })]
    }
}

fn add_sound_layer_invalid(map: &EditorMap) -> Vec<EditorAction> {
    // do a semi valid invalid action
    if rand::rngs::OsRng.next_u64() % 2 == 0 {
        let Some(EditorAction::AddSoundLayer(mut act)) = add_sound_layer_valid(map).pop() else {
            return Default::default();
        };
        if rand::rngs::OsRng.next_u64() % 2 == 0 {
            act.base.layer.attr.sound = Some(rand::rngs::OsRng.next_u64() as usize);
        } else {
            for q in act.base.layer.sounds.iter_mut() {
                q.sound_anim = Some(rand::rngs::OsRng.next_u64() as usize);
                q.pos_anim = Some(rand::rngs::OsRng.next_u64() as usize);
            }
        }
        vec![EditorAction::AddSoundLayer(act)]
    } else {
        vec![EditorAction::AddSoundLayer(ActAddSoundLayer {
            base: ActAddRemSoundLayer {
                is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
                group_index: rand::rngs::OsRng.next_u64() as usize,
                index: rand::rngs::OsRng.next_u64() as usize,
                layer: MapLayerSound {
                    attr: MapLayerSoundAttrs {
                        sound: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                            Default::default()
                        } else {
                            Some(rand::rngs::OsRng.next_u64() as usize)
                        },
                        high_detail: Default::default(),
                    },
                    sounds: Default::default(),
                    name: Default::default(),
                },
            },
        })]
    }
}

fn rem_design_layer_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    vec![match rand::rngs::OsRng.next_u64() % 3 {
        0 => EditorAction::RemTileLayer(ActRemTileLayer {
            base: ActAddRemTileLayer {
                is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
                group_index: rand::rngs::OsRng.next_u64() as usize,
                index: rand::rngs::OsRng.next_u64() as usize,
                layer: MapLayerTile {
                    attr: MapTileLayerAttr {
                        width: ((rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u16 + 1)
                            .try_into()
                            .unwrap(),
                        height: ((rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u16 + 1)
                            .try_into()
                            .unwrap(),
                        color: Default::default(),
                        high_detail: Default::default(),
                        color_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                            Default::default()
                        } else {
                            Some(rand::rngs::OsRng.next_u64() as usize)
                        },
                        color_anim_offset: Default::default(),
                        image_array: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                            Default::default()
                        } else {
                            Some(rand::rngs::OsRng.next_u64() as usize)
                        },
                    },
                    tiles: vec![
                        Default::default();
                        ((rand::rngs::OsRng.next_u64() % u8::MAX as u64)
                            * (rand::rngs::OsRng.next_u64() % u8::MAX as u64))
                            as usize
                    ],
                    name: Default::default(),
                },
            },
        }),
        1 => EditorAction::RemQuadLayer(ActRemQuadLayer {
            base: ActAddRemQuadLayer {
                is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
                group_index: rand::rngs::OsRng.next_u64() as usize,
                index: rand::rngs::OsRng.next_u64() as usize,
                layer: MapLayerQuad {
                    attr: MapLayerQuadsAttrs {
                        image: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                            Default::default()
                        } else {
                            Some(rand::rngs::OsRng.next_u64() as usize)
                        },
                        high_detail: Default::default(),
                    },
                    quads: Default::default(),
                    name: Default::default(),
                },
            },
        }),
        2 => EditorAction::RemSoundLayer(ActRemSoundLayer {
            base: ActAddRemSoundLayer {
                is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
                group_index: rand::rngs::OsRng.next_u64() as usize,
                index: rand::rngs::OsRng.next_u64() as usize,
                layer: MapLayerSound {
                    attr: MapLayerSoundAttrs {
                        sound: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                            Default::default()
                        } else {
                            Some(rand::rngs::OsRng.next_u64() as usize)
                        },
                        high_detail: Default::default(),
                    },
                    sounds: Default::default(),
                    name: Default::default(),
                },
            },
        }),
        _ => panic!("logic bug"),
    }]
}

fn add_physics_tile_layer_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    let index = rand::rngs::OsRng.next_u64() as usize;
    let len = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as usize
        * (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as usize;
    vec![EditorAction::AddPhysicsTileLayer(ActAddPhysicsTileLayer {
        base: ActAddRemPhysicsTileLayer {
            index,
            layer: match rand::rngs::OsRng.next_u64() as usize % 5 {
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

fn rem_physics_tile_layer_invalid(map: &EditorMap) -> Vec<EditorAction> {
    let EditorAction::AddPhysicsTileLayer(act) = add_physics_tile_layer_invalid(map).remove(0)
    else {
        panic!("expected add action")
    };

    vec![EditorAction::RemPhysicsTileLayer(ActRemPhysicsTileLayer {
        base: act.base,
    })]
}

fn tile_layer_replace_tiles_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    vec![EditorAction::TileLayerReplaceTiles(
        ActTileLayerReplaceTiles {
            base: ActTileLayerReplTilesBase {
                is_background: rand::rngs::OsRng.next_u64() as usize % 2 == 0,
                group_index: rand::rngs::OsRng.next_u64() as usize,
                layer_index: rand::rngs::OsRng.next_u64() as usize,
                old_tiles: vec![
                    Default::default();
                    ((rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u16 + 1)
                        as usize
                ],
                new_tiles: vec![
                    Default::default();
                    ((rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u16 + 1)
                        as usize
                ],
                x: (rand::rngs::OsRng.next_u64() % u16::MAX as u64) as u16,
                y: (rand::rngs::OsRng.next_u64() % u16::MAX as u64) as u16,
                w: ((rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u16 + 1)
                    .try_into()
                    .unwrap(),
                h: ((rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u16 + 1)
                    .try_into()
                    .unwrap(),
            },
        },
    )]
}

fn gen_phy_tiles() -> MapTileLayerPhysicsTiles {
    match rand::rngs::OsRng.next_u64() % 6 {
        0 => MapTileLayerPhysicsTiles::Game(vec![
            Default::default();
            ((rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u16 + 1)
                as usize
        ]),
        1 => MapTileLayerPhysicsTiles::Front(vec![
            Default::default();
            ((rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u16 + 1)
                as usize
        ]),
        2 => MapTileLayerPhysicsTiles::Tele(vec![
            Default::default();
            ((rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u16 + 1)
                as usize
        ]),
        3 => MapTileLayerPhysicsTiles::Speedup(vec![
            Default::default();
            ((rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u16 + 1)
                as usize
        ]),
        4 => MapTileLayerPhysicsTiles::Switch(vec![
            Default::default();
            ((rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u16 + 1)
                as usize
        ]),
        5 => MapTileLayerPhysicsTiles::Tune(vec![
            Default::default();
            ((rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u16 + 1)
                as usize
        ]),
        _ => panic!("indices over 5 not implemented."),
    }
}
fn tile_physics_layer_replace_tiles_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    let (old_tiles, new_tiles) = (gen_phy_tiles(), gen_phy_tiles());

    vec![EditorAction::TilePhysicsLayerReplaceTiles(
        ActTilePhysicsLayerReplaceTiles {
            base: ActTilePhysicsLayerReplTilesBase {
                layer_index: rand::rngs::OsRng.next_u64() as usize,
                old_tiles,
                new_tiles,
                x: (rand::rngs::OsRng.next_u64() % u16::MAX as u64) as u16,
                y: (rand::rngs::OsRng.next_u64() % u16::MAX as u64) as u16,
                w: ((rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u16 + 1)
                    .try_into()
                    .unwrap(),
                h: ((rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u16 + 1)
                    .try_into()
                    .unwrap(),
            },
        },
    )]
}

fn add_group_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    vec![EditorAction::AddGroup(ActAddGroup {
        base: ActAddRemGroup {
            is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
            index: rand::rngs::OsRng.next_u64() as usize,
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

fn rem_group_invalid(map: &EditorMap) -> Vec<EditorAction> {
    let EditorAction::AddGroup(act) = add_group_invalid(map).remove(0) else {
        panic!("expected add group action");
    };

    vec![EditorAction::RemGroup(ActRemGroup { base: act.base })]
}

fn change_group_attr_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    let is_background = (rand::rngs::OsRng.next_u64() % 2) == 0;
    let index = rand::rngs::OsRng.next_u64() as usize;

    vec![EditorAction::ChangeGroupAttr(ActChangeGroupAttr {
        is_background,
        group_index: index,
        old_attr: MapGroupAttr {
            offset: Default::default(),
            parallax: Default::default(),
            clipping: Default::default(),
        },
        new_attr: MapGroupAttr {
            offset: Default::default(),
            parallax: Default::default(),
            clipping: Default::default(),
        },
    })]
}
fn change_group_name_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    let is_background = (rand::rngs::OsRng.next_u64() % 2) == 0;
    let index = rand::rngs::OsRng.next_u64() as usize;

    vec![EditorAction::ChangeGroupName(ActChangeGroupName {
        is_background,
        group_index: index,
        old_name: Default::default(),
        new_name: format!("{}", rand::rngs::OsRng.next_u64()),
    })]
}

fn change_physics_group_attr_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    let w = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) + 1;
    let h = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) + 1;

    vec![EditorAction::ChangePhysicsGroupAttr(
        ActChangePhysicsGroupAttr {
            old_attr: MapGroupPhysicsAttr {
                width: (w as u16).try_into().unwrap(),
                height: (h as u16).try_into().unwrap(),
            },
            new_attr: MapGroupPhysicsAttr {
                width: (w as u16).try_into().unwrap(),
                height: (h as u16).try_into().unwrap(),
            },
            old_layer_tiles: vec![
                gen_phy_tiles();
                ((rand::rngs::OsRng.next_u64() % 5) + 1) as usize
            ],
            new_layer_tiles: vec![
                gen_phy_tiles();
                ((rand::rngs::OsRng.next_u64() % 5) + 1) as usize
            ],
        },
    )]
}

fn change_layer_design_attr_invalid(map: &EditorMap) -> Vec<EditorAction> {
    // do a semi valid invalid action
    if rand::rngs::OsRng.next_u64() % 2 == 0 {
        let Some(act) = change_layer_design_attr_valid(map).pop() else {
            return Default::default();
        };
        if let EditorAction::ChangeTileLayerDesignAttr(mut act) = act {
            act.new_attr.image_array = Some(rand::rngs::OsRng.next_u64() as usize);

            vec![EditorAction::ChangeTileLayerDesignAttr(act)]
        } else if let EditorAction::ChangeQuadLayerAttr(mut act) = act {
            act.new_attr.image = Some(rand::rngs::OsRng.next_u64() as usize);

            vec![EditorAction::ChangeQuadLayerAttr(act)]
        } else if let EditorAction::ChangeSoundLayerAttr(mut act) = act {
            act.new_attr.sound = Some(rand::rngs::OsRng.next_u64() as usize);

            vec![EditorAction::ChangeSoundLayerAttr(act)]
        } else {
            Default::default()
        }
    } else {
        match rand::rngs::OsRng.next_u64() % 3 {
            0 => {
                let w = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) + 1;
                let h = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) + 1;
                let len = ((rand::rngs::OsRng.next_u64() % u8::MAX as u64) + 1) as usize;
                Some(EditorAction::ChangeTileLayerDesignAttr(
                    ActChangeTileLayerDesignAttr {
                        is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
                        group_index: rand::rngs::OsRng.next_u64() as usize,
                        layer_index: rand::rngs::OsRng.next_u64() as usize,
                        old_attr: MapTileLayerAttr {
                            width: (w as u16).try_into().unwrap(),
                            height: (h as u16).try_into().unwrap(),
                            color: Default::default(),
                            high_detail: Default::default(),
                            color_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                                Default::default()
                            } else {
                                Some(rand::rngs::OsRng.next_u64() as usize)
                            },
                            color_anim_offset: Default::default(),
                            image_array: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                                Default::default()
                            } else {
                                Some(rand::rngs::OsRng.next_u64() as usize)
                            },
                        },
                        new_attr: MapTileLayerAttr {
                            width: (w as u16).try_into().unwrap(),
                            height: (h as u16).try_into().unwrap(),
                            color: Default::default(),
                            high_detail: Default::default(),
                            color_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                                Default::default()
                            } else {
                                Some(rand::rngs::OsRng.next_u64() as usize)
                            },
                            color_anim_offset: Default::default(),
                            image_array: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                                Default::default()
                            } else {
                                Some(rand::rngs::OsRng.next_u64() as usize)
                            },
                        },
                        old_tiles: vec![Default::default(); len],
                        new_tiles: vec![Default::default(); len],
                    },
                ))
            }
            1 => Some(EditorAction::ChangeQuadLayerAttr(ActChangeQuadLayerAttr {
                is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
                group_index: rand::rngs::OsRng.next_u64() as usize,
                layer_index: rand::rngs::OsRng.next_u64() as usize,
                old_attr: MapLayerQuadsAttrs {
                    image: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                        Default::default()
                    } else {
                        Some(rand::rngs::OsRng.next_u64() as usize)
                    },
                    high_detail: Default::default(),
                },
                new_attr: MapLayerQuadsAttrs {
                    image: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                        Default::default()
                    } else {
                        Some(rand::rngs::OsRng.next_u64() as usize)
                    },
                    high_detail: Default::default(),
                },
            })),
            2 => Some(EditorAction::ChangeSoundLayerAttr(
                ActChangeSoundLayerAttr {
                    is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
                    group_index: rand::rngs::OsRng.next_u64() as usize,
                    layer_index: rand::rngs::OsRng.next_u64() as usize,
                    old_attr: MapLayerSoundAttrs {
                        sound: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                            Default::default()
                        } else {
                            Some(rand::rngs::OsRng.next_u64() as usize)
                        },
                        high_detail: Default::default(),
                    },
                    new_attr: MapLayerSoundAttrs {
                        sound: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                            Default::default()
                        } else {
                            Some(rand::rngs::OsRng.next_u64() as usize)
                        },
                        high_detail: Default::default(),
                    },
                },
            )),
            _ => panic!("indices over 3 are not implemented."),
        }
        .into_iter()
        .collect()
    }
}
fn change_design_layer_name_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    vec![EditorAction::ChangeDesignLayerName(
        ActChangeDesignLayerName {
            is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
            group_index: rand::rngs::OsRng.next_u64() as usize,
            layer_index: rand::rngs::OsRng.next_u64() as usize,
            old_name: Default::default(),
            new_name: format!("{}", rand::rngs::OsRng.next_u64()),
        },
    )]
}

fn change_quad_attr_invalid(map: &EditorMap) -> Vec<EditorAction> {
    // do a semi valid invalid action
    if rand::rngs::OsRng.next_u64() % 2 == 0 {
        let Some(EditorAction::ChangeQuadAttr(mut act)) = change_quad_attr_valid(map).pop() else {
            return Default::default();
        };
        act.new_attr.color_anim = Some(rand::rngs::OsRng.next_u64() as usize);
        act.new_attr.pos_anim = Some(rand::rngs::OsRng.next_u64() as usize);

        vec![EditorAction::ChangeQuadAttr(act)]
    } else {
        let index = rand::rngs::OsRng.next_u64() as usize;

        vec![EditorAction::ChangeQuadAttr(Box::new(ActChangeQuadAttr {
            is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
            group_index: rand::rngs::OsRng.next_u64() as usize,
            layer_index: rand::rngs::OsRng.next_u64() as usize,
            index,
            old_attr: Quad {
                points: Default::default(),
                colors: Default::default(),
                tex_coords: Default::default(),
                pos_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                    Default::default()
                } else {
                    Some(rand::rngs::OsRng.next_u64() as usize)
                },
                pos_anim_offset: Default::default(),
                color_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                    Default::default()
                } else {
                    Some(rand::rngs::OsRng.next_u64() as usize)
                },
                color_anim_offset: Default::default(),
            },
            new_attr: Quad {
                points: Default::default(),
                colors: Default::default(),
                tex_coords: Default::default(),
                pos_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                    Default::default()
                } else {
                    Some(rand::rngs::OsRng.next_u64() as usize)
                },
                pos_anim_offset: Default::default(),
                color_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                    Default::default()
                } else {
                    Some(rand::rngs::OsRng.next_u64() as usize)
                },
                color_anim_offset: Default::default(),
            },
        }))]
    }
}
fn change_sound_attr_invalid(map: &EditorMap) -> Vec<EditorAction> {
    // do a semi valid invalid action
    if rand::rngs::OsRng.next_u64() % 2 == 0 {
        let Some(EditorAction::ChangeSoundAttr(mut act)) = change_sound_attr_valid(map).pop()
        else {
            return Default::default();
        };
        act.new_attr.sound_anim = Some(rand::rngs::OsRng.next_u64() as usize);
        act.new_attr.pos_anim = Some(rand::rngs::OsRng.next_u64() as usize);

        vec![EditorAction::ChangeSoundAttr(act)]
    } else {
        let index = rand::rngs::OsRng.next_u64() as usize;

        vec![EditorAction::ChangeSoundAttr(ActChangeSoundAttr {
            is_background: rand::rngs::OsRng.next_u64() % 2 == 0,
            group_index: rand::rngs::OsRng.next_u64() as usize,
            layer_index: rand::rngs::OsRng.next_u64() as usize,
            index,
            old_attr: Sound {
                pos: Default::default(),
                looped: Default::default(),
                panning: Default::default(),
                time_delay: Default::default(),
                falloff: Default::default(),
                pos_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                    Default::default()
                } else {
                    Some(rand::rngs::OsRng.next_u64() as usize)
                },
                pos_anim_offset: Default::default(),
                sound_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                    Default::default()
                } else {
                    Some(rand::rngs::OsRng.next_u64() as usize)
                },
                sound_anim_offset: Default::default(),
                shape: SoundShape::Circle {
                    radius: uffixed::from_num(30),
                },
            },
            new_attr: Sound {
                pos: Default::default(),
                looped: Default::default(),
                panning: Default::default(),
                time_delay: Default::default(),
                falloff: Default::default(),
                pos_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                    Default::default()
                } else {
                    Some(rand::rngs::OsRng.next_u64() as usize)
                },
                pos_anim_offset: Default::default(),
                sound_anim: if rand::rngs::OsRng.next_u64() % 2 == 0 {
                    Default::default()
                } else {
                    Some(rand::rngs::OsRng.next_u64() as usize)
                },
                sound_anim_offset: Default::default(),
                shape: SoundShape::Circle {
                    radius: uffixed::from_num(30),
                },
            },
        })]
    }
}

fn change_teleport_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    let index = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8;
    vec![EditorAction::ChangeTeleporter(ActChangeTeleporter {
        index,
        old_name: Default::default(),
        new_name: "created".into(),
    })]
}
fn change_switch_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    let index = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8;
    vec![EditorAction::ChangeSwitch(ActChangeSwitch {
        index,
        old_name: Default::default(),
        new_name: "created".into(),
    })]
}
fn change_tune_zone_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    let index = (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as u8;
    vec![EditorAction::ChangeTuneZone(ActChangeTuneZone {
        index,
        old_name: Default::default(),
        new_name: "created".into(),
        old_tunes: Default::default(),
        new_tunes: vec![
            Default::default();
            (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as usize
        ]
        .into_iter()
        .collect(),
    })]
}

fn add_pos_anim_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    let index = rand::rngs::OsRng.next_u64() as usize;
    vec![EditorAction::AddPosAnim(ActAddPosAnim {
        base: ActAddRemPosAnim {
            index,
            anim: AnimBase {
                name: "".to_string(),
                points: vec![
                    AnimPoint {
                        curve_type: AnimPointCurveType::Linear,
                        time: Duration::from_secs(rand::rngs::OsRng.next_u64()),
                        value: Default::default()
                    };
                    (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as usize
                ],
                synchronized: Default::default(),
            },
        },
    })]
}
fn rem_pos_anim_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    let index = rand::rngs::OsRng.next_u64() as usize;
    vec![EditorAction::RemPosAnim(ActRemPosAnim {
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

fn add_color_anim_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    let index = rand::rngs::OsRng.next_u64() as usize;
    vec![EditorAction::AddColorAnim(ActAddColorAnim {
        base: ActAddRemColorAnim {
            index,
            anim: AnimBase {
                name: "".to_string(),
                points: vec![
                    AnimPoint {
                        curve_type: AnimPointCurveType::Linear,
                        time: Duration::from_secs(rand::rngs::OsRng.next_u64()),
                        value: Default::default()
                    };
                    (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as usize
                ],
                synchronized: Default::default(),
            },
        },
    })]
}
fn rem_color_anim_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    let index = rand::rngs::OsRng.next_u64() as usize;
    vec![EditorAction::RemColorAnim(ActRemColorAnim {
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

fn add_sound_anim_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    let index = rand::rngs::OsRng.next_u64() as usize;
    vec![EditorAction::AddSoundAnim(ActAddSoundAnim {
        base: ActAddRemSoundAnim {
            index,
            anim: AnimBase {
                name: "".to_string(),
                points: vec![
                    AnimPoint {
                        curve_type: AnimPointCurveType::Linear,
                        time: Duration::from_secs(rand::rngs::OsRng.next_u64()),
                        value: Default::default()
                    };
                    (rand::rngs::OsRng.next_u64() % u8::MAX as u64) as usize
                ],
                synchronized: Default::default(),
            },
        },
    })]
}
fn rem_sound_anim_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    let index = rand::rngs::OsRng.next_u64() as usize;
    vec![EditorAction::RemSoundAnim(ActRemSoundAnim {
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

fn set_commands_invalid(_map: &EditorMap) -> Vec<EditorAction> {
    vec![EditorAction::SetCommands(ActSetCommands {
        old_commands: Default::default(),
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

/// Invalid here still makes sure that no memory exhaustion happens.
pub fn random_invalid_action(map: &EditorMap) -> Vec<EditorAction> {
    // must match the last value in the `match` + 1
    const TOTAL_ACTIONS: u64 = 45;
    loop {
        match match rand::rngs::OsRng.next_u64() % TOTAL_ACTIONS {
            0 => move_group_invalid(map),
            1 => move_layer_invalid(map),
            2 => add_img_invalid(map),
            3 => add_img_2d_array_invalid(map),
            // reserved for add_sound_valid
            4 => Default::default(),
            5 => rem_img_invalid(map),
            6 => rem_img_2d_array_invalid(map),
            // reserved for rem_sound_valid
            7 => Default::default(),
            8 => layer_change_image_index_invalid(map),
            9 => layer_change_sound_index_invalid(map),
            10 => quad_layer_add_quads_invalid(map),
            11 => sound_layer_add_sounds_invalid(map),
            12 => quad_layer_rem_quads_invalid(map),
            13 => sound_layer_rem_sounds_invalid(map),
            14 => add_tile_layer_invalid(map),
            15 => add_quad_layer_invalid(map),
            16 => add_sound_layer_invalid(map),
            17..=19 => rem_design_layer_invalid(map),
            20 => add_physics_tile_layer_invalid(map),
            21 => rem_physics_tile_layer_invalid(map),
            22 => tile_layer_replace_tiles_invalid(map),
            23 => tile_physics_layer_replace_tiles_invalid(map),
            24 => add_group_invalid(map),
            25 => rem_group_invalid(map),
            26 => change_group_attr_invalid(map),
            27 => change_group_name_invalid(map),
            28 => change_physics_group_attr_invalid(map),
            29..=31 => change_layer_design_attr_invalid(map),
            32 => change_design_layer_name_invalid(map),
            33 => change_quad_attr_invalid(map),
            34 => change_sound_attr_invalid(map),
            35 => change_teleport_invalid(map),
            36 => change_switch_invalid(map),
            37 => change_tune_zone_invalid(map),
            38 => add_pos_anim_invalid(map),
            39 => rem_pos_anim_invalid(map),
            40 => add_color_anim_invalid(map),
            41 => rem_color_anim_invalid(map),
            42 => add_sound_anim_invalid(map),
            43 => rem_sound_anim_invalid(map),
            44 => set_commands_invalid(map),
            _ => panic!("unsupported action count"),
        } {
            act if !act.is_empty() => return act,
            _ => {
                // continue
            }
        }
    }
}
