use std::{rc::Rc, sync::Arc};

use anyhow::anyhow;
use client_render_base::map::map_buffered::SoundLayerSounds;
use graphics::{
    graphics_mt::GraphicsMultiThreaded,
    handles::{
        backend::backend::GraphicsBackendHandle,
        buffer_object::buffer_object::GraphicsBufferObjectHandle,
        texture::texture::GraphicsTextureHandle,
    },
};
use graphics_types::{commands::TexFlags, types::GraphicsMemoryAllocationType};
use image_utils::{png::load_png_image_as_rgba, utils::texture_2d_to_3d};
use map::{
    map::groups::layers::{
        design::{
            MapLayer, MapLayerQuad, MapLayerQuadsAttrs, MapLayerSound, MapLayerSoundAttrs,
            MapLayerTile,
        },
        physics::{MapLayerPhysics, MapLayerTilePhysicsTuneZone},
        tiles::{MapTileLayerAttr, MapTileLayerPhysicsTiles},
    },
    skeleton::groups::layers::{
        design::MapLayerSkeleton,
        physics::{
            MapLayerPhysicsSkeleton, MapLayerSwitchPhysicsSkeleton, MapLayerTelePhysicsSkeleton,
            MapLayerTilePhysicsBaseSkeleton, MapLayerTunePhysicsSkeleton,
        },
    },
};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use sound::sound_mt::SoundMultiThreaded;

use crate::{
    actions::actions::{
        ActAddColorAnim, ActAddGroup, ActAddImage, ActAddImage2dArray, ActAddPhysicsTileLayer,
        ActAddPosAnim, ActAddQuadLayer, ActAddRemImage, ActAddRemQuadLayer, ActAddRemSound,
        ActAddRemSoundLayer, ActAddRemTileLayer, ActAddSound, ActAddSoundAnim, ActAddSoundLayer,
        ActAddTileLayer, ActChangeGroupAttr, ActChangePhysicsGroupAttr, ActChangeQuadAttr,
        ActChangeQuadLayerAttr, ActChangeSoundAttr, ActChangeSoundLayerAttr, ActChangeSwitch,
        ActChangeTeleporter, ActChangeTileLayerDesignAttr, ActChangeTuneZone,
        ActLayerChangeImageIndex, ActLayerChangeSoundIndex, ActQuadLayerAddQuads,
        ActQuadLayerAddRemQuads, ActQuadLayerRemQuads, ActRemColorAnim, ActRemGroup, ActRemImage,
        ActRemImage2dArray, ActRemPhysicsTileLayer, ActRemPosAnim, ActRemQuadLayer, ActRemSound,
        ActRemSoundAnim, ActRemSoundLayer, ActRemTileLayer, ActSoundLayerAddRemSounds,
        ActSoundLayerAddSounds, ActSoundLayerRemSounds, ActSwapGroups, ActSwapLayers,
        ActTileLayerReplTilesBase, ActTileLayerReplaceTiles, ActTilePhysicsLayerReplTilesBase,
        ActTilePhysicsLayerReplaceTiles, EditorAction,
    },
    map::{
        EditorAnimationProps, EditorColorAnimation, EditorCommonGroupOrLayerAttr, EditorGroup,
        EditorGroupProps, EditorImage, EditorImage2dArray, EditorLayer, EditorLayerQuad,
        EditorLayerSound, EditorLayerTile, EditorMap, EditorPhysicsLayer, EditorPhysicsLayerProps,
        EditorPosAnimation, EditorQuadLayerProps, EditorResource, EditorSound,
        EditorSoundAnimation, EditorSoundLayerProps, EditorTileLayerProps,
    },
    map_tools::{
        finish_design_quad_layer_buffer, finish_design_tile_layer_buffer,
        finish_physics_layer_buffer, update_design_quad_layer, update_design_tile_layer,
        update_physics_layer, upload_design_quad_layer_buffer, upload_design_tile_layer_buffer,
        upload_physics_layer_buffer,
    },
};

fn merge_quad_addrem_base(
    mut act1: ActQuadLayerAddRemQuads,
    act2: ActQuadLayerAddRemQuads,
) -> anyhow::Result<(ActQuadLayerAddRemQuads, Option<ActQuadLayerAddRemQuads>)> {
    // if both actions modify the same quad range they can be merged
    let (min_index, mut min_index_quads, max_index, max_index_quads) = if act1.index < act2.index {
        (act1.index, act1.quads, act2.index, act2.quads)
    } else {
        (act2.index, act2.quads, act1.index, act1.quads)
    };
    if min_index + min_index_quads.len() >= max_index {
        act1.index = min_index;
        act1.quads = {
            if act1.index == min_index {
                min_index_quads.splice((max_index - min_index).., max_index_quads);
                min_index_quads
            } else {
                min_index_quads.extend(max_index_quads.into_iter().skip(max_index - min_index));
                min_index_quads
            }
        };
        Ok((act1, None))
    } else {
        let is_background = act1.is_background;
        let group_index = act1.group_index;
        let layer_index = act1.layer_index;
        let (mut act1, mut act2) = (
            ActQuadLayerAddRemQuads {
                is_background,
                group_index,
                layer_index,
                index: min_index,
                quads: min_index_quads,
            },
            ActQuadLayerAddRemQuads {
                is_background,
                group_index,
                layer_index,
                index: max_index,
                quads: max_index_quads,
            },
        );
        if act1.index != min_index {
            std::mem::swap(&mut act1, &mut act2);
        }
        Ok((act1, Some(act2)))
    }
}

fn merge_sound_addrem_base(
    mut act1: ActSoundLayerAddRemSounds,
    act2: ActSoundLayerAddRemSounds,
) -> anyhow::Result<(ActSoundLayerAddRemSounds, Option<ActSoundLayerAddRemSounds>)> {
    // if both actions modify the same sound range they can be merged
    let (min_index, mut min_index_sounds, max_index, max_index_sounds) = if act1.index < act2.index
    {
        (act1.index, act1.sounds, act2.index, act2.sounds)
    } else {
        (act2.index, act2.sounds, act1.index, act1.sounds)
    };
    if min_index + min_index_sounds.len() >= max_index {
        act1.index = min_index;
        act1.sounds = {
            if act1.index == min_index {
                min_index_sounds.splice((max_index - min_index).., max_index_sounds);
                min_index_sounds
            } else {
                min_index_sounds.extend(max_index_sounds.into_iter().skip(max_index - min_index));
                min_index_sounds
            }
        };
        Ok((act1, None))
    } else {
        let is_background = act1.is_background;
        let group_index = act1.group_index;
        let layer_index = act1.layer_index;
        let (mut act1, mut act2) = (
            ActSoundLayerAddRemSounds {
                is_background,
                group_index,
                layer_index,
                index: min_index,
                sounds: min_index_sounds,
            },
            ActSoundLayerAddRemSounds {
                is_background,
                group_index,
                layer_index,
                index: max_index,
                sounds: max_index_sounds,
            },
        );
        if act1.index != min_index {
            std::mem::swap(&mut act1, &mut act2);
        }
        Ok((act1, Some(act2)))
    }
}

/// returns at least one action
/// if both actions are returned, that means these actions are not mergeable
fn merge_actions_group(
    action1: EditorAction,
    action2: EditorAction,
) -> anyhow::Result<(EditorAction, Option<EditorAction>)> {
    match (action1, action2) {
        (EditorAction::SwapGroups(mut act1), EditorAction::SwapGroups(act2)) => {
            if act1.is_background == act2.is_background {
                act1.group2 = act2.group2;

                Ok((EditorAction::SwapGroups(act1), None))
            } else {
                Ok((
                    EditorAction::SwapGroups(act1),
                    Some(EditorAction::SwapGroups(act2)),
                ))
            }
        }
        (EditorAction::SwapLayers(mut act1), EditorAction::SwapLayers(act2)) => {
            if act1.is_background == act2.is_background && act1.group == act2.group {
                act1.layer2 = act2.layer2;

                Ok((EditorAction::SwapLayers(act1), None))
            } else {
                Ok((
                    EditorAction::SwapLayers(act1),
                    Some(EditorAction::SwapLayers(act2)),
                ))
            }
        }
        (EditorAction::AddImage(act1), EditorAction::AddImage(act2)) => Ok((
            EditorAction::AddImage(act1),
            Some(EditorAction::AddImage(act2)),
        )),
        (EditorAction::AddSound(act1), EditorAction::AddSound(act2)) => Ok((
            EditorAction::AddSound(act1),
            Some(EditorAction::AddSound(act2)),
        )),
        (EditorAction::RemImage(act1), EditorAction::RemImage(act2)) => Ok((
            EditorAction::RemImage(act1),
            Some(EditorAction::RemImage(act2)),
        )),
        (EditorAction::RemSound(act1), EditorAction::RemSound(act2)) => Ok((
            EditorAction::RemSound(act1),
            Some(EditorAction::RemSound(act2)),
        )),
        (
            EditorAction::LayerChangeImageIndex(mut act1),
            EditorAction::LayerChangeImageIndex(act2),
        ) => {
            if act1.is_background == act2.is_background
                && act1.group_index == act2.group_index
                && act1.layer_index == act2.layer_index
            {
                act1.new_index = act2.new_index;

                Ok((EditorAction::LayerChangeImageIndex(act1), None))
            } else {
                Ok((
                    EditorAction::LayerChangeImageIndex(act1),
                    Some(EditorAction::LayerChangeImageIndex(act2)),
                ))
            }
        }
        (
            EditorAction::LayerChangeSoundIndex(mut act1),
            EditorAction::LayerChangeSoundIndex(act2),
        ) => {
            if act1.is_background == act2.is_background
                && act1.group_index == act2.group_index
                && act1.layer_index == act2.layer_index
            {
                act1.new_index = act2.new_index;

                Ok((EditorAction::LayerChangeSoundIndex(act1), None))
            } else {
                Ok((
                    EditorAction::LayerChangeSoundIndex(act1),
                    Some(EditorAction::LayerChangeSoundIndex(act2)),
                ))
            }
        }
        (EditorAction::QuadLayerAddQuads(act1), EditorAction::QuadLayerAddQuads(act2)) => {
            if act1.base.is_background == act2.base.is_background
                && act1.base.group_index == act2.base.group_index
                && act1.base.layer_index == act2.base.layer_index
            {
                let (act1, act2) = merge_quad_addrem_base(act1.base, act2.base)?;

                Ok((
                    EditorAction::QuadLayerAddQuads(ActQuadLayerAddQuads { base: act1 }),
                    act2.map(|act| {
                        EditorAction::QuadLayerAddQuads(ActQuadLayerAddQuads { base: act })
                    }),
                ))
            } else {
                Ok((
                    EditorAction::QuadLayerAddQuads(act1),
                    Some(EditorAction::QuadLayerAddQuads(act2)),
                ))
            }
        }
        (EditorAction::SoundLayerAddSounds(act1), EditorAction::SoundLayerAddSounds(act2)) => {
            if act1.base.is_background == act2.base.is_background
                && act1.base.group_index == act2.base.group_index
                && act1.base.layer_index == act2.base.layer_index
            {
                let (act1, act2) = merge_sound_addrem_base(act1.base, act2.base)?;

                Ok((
                    EditorAction::SoundLayerAddSounds(ActSoundLayerAddSounds { base: act1 }),
                    act2.map(|act| {
                        EditorAction::SoundLayerAddSounds(ActSoundLayerAddSounds { base: act })
                    }),
                ))
            } else {
                Ok((
                    EditorAction::SoundLayerAddSounds(act1),
                    Some(EditorAction::SoundLayerAddSounds(act2)),
                ))
            }
        }
        (EditorAction::QuadLayerRemQuads(act1), EditorAction::QuadLayerRemQuads(act2)) => {
            if act1.base.is_background == act2.base.is_background
                && act1.base.group_index == act2.base.group_index
                && act1.base.layer_index == act2.base.layer_index
            {
                let (act1, act2) = merge_quad_addrem_base(act1.base, act2.base)?;

                Ok((
                    EditorAction::QuadLayerRemQuads(ActQuadLayerRemQuads { base: act1 }),
                    act2.map(|act| {
                        EditorAction::QuadLayerRemQuads(ActQuadLayerRemQuads { base: act })
                    }),
                ))
            } else {
                Ok((
                    EditorAction::QuadLayerRemQuads(act1),
                    Some(EditorAction::QuadLayerRemQuads(act2)),
                ))
            }
        }
        (EditorAction::SoundLayerRemSounds(act1), EditorAction::SoundLayerRemSounds(act2)) => {
            if act1.base.is_background == act2.base.is_background
                && act1.base.group_index == act2.base.group_index
                && act1.base.layer_index == act2.base.layer_index
            {
                let (act1, act2) = merge_sound_addrem_base(act1.base, act2.base)?;

                Ok((
                    EditorAction::SoundLayerRemSounds(ActSoundLayerRemSounds { base: act1 }),
                    act2.map(|act| {
                        EditorAction::SoundLayerRemSounds(ActSoundLayerRemSounds { base: act })
                    }),
                ))
            } else {
                Ok((
                    EditorAction::SoundLayerRemSounds(act1),
                    Some(EditorAction::SoundLayerRemSounds(act2)),
                ))
            }
        }
        (EditorAction::AddTileLayer(act1), EditorAction::AddTileLayer(act2)) => Ok((
            EditorAction::AddTileLayer(act1),
            Some(EditorAction::AddTileLayer(act2)),
        )),
        (EditorAction::AddQuadLayer(act1), EditorAction::AddQuadLayer(act2)) => Ok((
            EditorAction::AddQuadLayer(act1),
            Some(EditorAction::AddQuadLayer(act2)),
        )),
        (EditorAction::AddSoundLayer(act1), EditorAction::AddSoundLayer(act2)) => Ok((
            EditorAction::AddSoundLayer(act1),
            Some(EditorAction::AddSoundLayer(act2)),
        )),
        (EditorAction::RemTileLayer(act1), EditorAction::RemTileLayer(act2)) => Ok((
            EditorAction::RemTileLayer(act1),
            Some(EditorAction::RemTileLayer(act2)),
        )),
        (EditorAction::RemQuadLayer(act1), EditorAction::RemQuadLayer(act2)) => Ok((
            EditorAction::RemQuadLayer(act1),
            Some(EditorAction::RemQuadLayer(act2)),
        )),
        (EditorAction::RemSoundLayer(act1), EditorAction::RemSoundLayer(act2)) => Ok((
            EditorAction::RemSoundLayer(act1),
            Some(EditorAction::RemSoundLayer(act2)),
        )),
        (EditorAction::AddPhysicsTileLayer(act1), EditorAction::AddPhysicsTileLayer(act2)) => Ok((
            EditorAction::AddPhysicsTileLayer(act1),
            Some(EditorAction::AddPhysicsTileLayer(act2)),
        )),
        (EditorAction::RemPhysicsTileLayer(act1), EditorAction::RemPhysicsTileLayer(act2)) => Ok((
            EditorAction::RemPhysicsTileLayer(act1),
            Some(EditorAction::RemPhysicsTileLayer(act2)),
        )),
        // replace tiles not worth it, moving the cursor diagonally directly makes 2 actions incompatible
        (EditorAction::TileLayerReplaceTiles(act1), EditorAction::TileLayerReplaceTiles(act2)) => {
            Ok((
                EditorAction::TileLayerReplaceTiles(act1),
                Some(EditorAction::TileLayerReplaceTiles(act2)),
            ))
        }
        // replace tiles not worth it, moving the cursor diagonally directly makes 2 actions incompatible
        (
            EditorAction::TilePhysicsLayerReplaceTiles(act1),
            EditorAction::TilePhysicsLayerReplaceTiles(act2),
        ) => Ok((
            EditorAction::TilePhysicsLayerReplaceTiles(act1),
            Some(EditorAction::TilePhysicsLayerReplaceTiles(act2)),
        )),
        (EditorAction::AddGroup(act1), EditorAction::AddGroup(act2)) => Ok((
            EditorAction::AddGroup(act1),
            Some(EditorAction::AddGroup(act2)),
        )),
        (EditorAction::RemGroup(act1), EditorAction::RemGroup(act2)) => Ok((
            EditorAction::RemGroup(act1),
            Some(EditorAction::RemGroup(act2)),
        )),
        (EditorAction::ChangeGroupAttr(mut act1), EditorAction::ChangeGroupAttr(act2)) => {
            if act1.is_background == act2.is_background && act1.group_index == act2.group_index {
                act1.new_attr = act2.new_attr;
                Ok((EditorAction::ChangeGroupAttr(act1), None))
            } else {
                Ok((
                    EditorAction::ChangeGroupAttr(act1),
                    Some(EditorAction::ChangeGroupAttr(act2)),
                ))
            }
        }
        (
            EditorAction::ChangePhysicsGroupAttr(mut act1),
            EditorAction::ChangePhysicsGroupAttr(act2),
        ) => {
            act1.new_layer_tiles = act2.new_layer_tiles;
            act1.new_attr = act2.new_attr;
            Ok((EditorAction::ChangePhysicsGroupAttr(act1), None))
        }
        (
            EditorAction::ChangeTileLayerDesignAttr(mut act1),
            EditorAction::ChangeTileLayerDesignAttr(act2),
        ) => {
            if act1.is_background == act2.is_background
                && act1.group_index == act2.group_index
                && act1.layer_index == act2.layer_index
            {
                act1.new_attr = act2.new_attr;
                act1.new_tiles = act2.new_tiles;
                Ok((EditorAction::ChangeTileLayerDesignAttr(act1), None))
            } else {
                Ok((
                    EditorAction::ChangeTileLayerDesignAttr(act1),
                    Some(EditorAction::ChangeTileLayerDesignAttr(act2)),
                ))
            }
        }
        (EditorAction::ChangeQuadLayerAttr(mut act1), EditorAction::ChangeQuadLayerAttr(act2)) => {
            if act1.is_background == act2.is_background
                && act1.group_index == act2.group_index
                && act1.layer_index == act2.layer_index
            {
                act1.new_attr = act2.new_attr;
                Ok((EditorAction::ChangeQuadLayerAttr(act1), None))
            } else {
                Ok((
                    EditorAction::ChangeQuadLayerAttr(act1),
                    Some(EditorAction::ChangeQuadLayerAttr(act2)),
                ))
            }
        }
        (
            EditorAction::ChangeSoundLayerAttr(mut act1),
            EditorAction::ChangeSoundLayerAttr(act2),
        ) => {
            if act1.is_background == act2.is_background
                && act1.group_index == act2.group_index
                && act1.layer_index == act2.layer_index
            {
                act1.new_attr = act2.new_attr;
                Ok((EditorAction::ChangeSoundLayerAttr(act1), None))
            } else {
                Ok((
                    EditorAction::ChangeSoundLayerAttr(act1),
                    Some(EditorAction::ChangeSoundLayerAttr(act2)),
                ))
            }
        }
        (EditorAction::ChangeQuadAttr(mut act1), EditorAction::ChangeQuadAttr(act2)) => {
            if act1.is_background == act2.is_background
                && act1.group_index == act2.group_index
                && act1.layer_index == act2.layer_index
                && act1.index == act2.index
            {
                act1.new_attr = act2.new_attr;
                Ok((EditorAction::ChangeQuadAttr(act1), None))
            } else {
                Ok((
                    EditorAction::ChangeQuadAttr(act1),
                    Some(EditorAction::ChangeQuadAttr(act2)),
                ))
            }
        }
        (EditorAction::ChangeSoundAttr(mut act1), EditorAction::ChangeSoundAttr(act2)) => {
            if act1.is_background == act2.is_background
                && act1.group_index == act2.group_index
                && act1.layer_index == act2.layer_index
                && act1.index == act2.index
            {
                act1.new_attr = act2.new_attr;
                Ok((EditorAction::ChangeSoundAttr(act1), None))
            } else {
                Ok((
                    EditorAction::ChangeSoundAttr(act1),
                    Some(EditorAction::ChangeSoundAttr(act2)),
                ))
            }
        }
        (EditorAction::ChangeTeleporter(mut act1), EditorAction::ChangeTeleporter(act2)) => {
            if act1.index == act2.index {
                act1.new_name = act2.new_name;
                Ok((EditorAction::ChangeTeleporter(act1), None))
            } else {
                Ok((
                    EditorAction::ChangeTeleporter(act1),
                    Some(EditorAction::ChangeTeleporter(act2)),
                ))
            }
        }
        (EditorAction::ChangeSwitch(mut act1), EditorAction::ChangeSwitch(act2)) => {
            if act1.index == act2.index {
                act1.new_name = act2.new_name;
                Ok((EditorAction::ChangeSwitch(act1), None))
            } else {
                Ok((
                    EditorAction::ChangeSwitch(act1),
                    Some(EditorAction::ChangeSwitch(act2)),
                ))
            }
        }
        (EditorAction::ChangeTuneZone(mut act1), EditorAction::ChangeTuneZone(act2)) => {
            if act1.index == act2.index {
                act1.new_name = act2.new_name;
                act1.new_tunes = act2.new_tunes;
                Ok((EditorAction::ChangeTuneZone(act1), None))
            } else {
                Ok((
                    EditorAction::ChangeTuneZone(act1),
                    Some(EditorAction::ChangeTuneZone(act2)),
                ))
            }
        }
        (EditorAction::AddPosAnim(act1), EditorAction::AddPosAnim(act2)) => Ok((
            EditorAction::AddPosAnim(act1),
            Some(EditorAction::AddPosAnim(act2)),
        )),
        (EditorAction::RemPosAnim(act1), EditorAction::RemPosAnim(act2)) => Ok((
            EditorAction::RemPosAnim(act1),
            Some(EditorAction::RemPosAnim(act2)),
        )),
        (EditorAction::AddColorAnim(act1), EditorAction::AddColorAnim(act2)) => Ok((
            EditorAction::AddColorAnim(act1),
            Some(EditorAction::AddColorAnim(act2)),
        )),
        (EditorAction::RemColorAnim(act1), EditorAction::RemColorAnim(act2)) => Ok((
            EditorAction::RemColorAnim(act1),
            Some(EditorAction::RemColorAnim(act2)),
        )),
        (EditorAction::AddSoundAnim(act1), EditorAction::AddSoundAnim(act2)) => Ok((
            EditorAction::AddSoundAnim(act1),
            Some(EditorAction::AddSoundAnim(act2)),
        )),
        (EditorAction::RemSoundAnim(act1), EditorAction::RemSoundAnim(act2)) => Ok((
            EditorAction::RemSoundAnim(act1),
            Some(EditorAction::RemSoundAnim(act2)),
        )),
        (act1, act2) => Ok((act1, Some(act2))),
    }
}

/// Merge multiple same actions into as few as possible.
///
/// The implementation automatically decides if it thinks
/// that the actions should be merged.
/// If two or more actions are not similar, this function still returns Ok(_),
/// it will simply not merge them.
pub fn merge_actions(actions: &mut Vec<EditorAction>) -> anyhow::Result<()> {
    if actions.is_empty() {
        return Ok(());
    }

    while actions.len() > 1 {
        let act2 = actions.pop();
        let act1 = actions.pop();

        if let (Some(act1), Some(act2)) = (act1, act2) {
            let (act1, act2) = merge_actions_group(act1, act2)?;
            actions.push(act1);
            if let Some(act2) = act2 {
                actions.push(act2);
                break;
            }
        } else {
            unreachable!();
        }
    }

    Ok(())
}

pub fn do_action(
    tp: &Arc<rayon::ThreadPool>,
    sound_mt: &SoundMultiThreaded,
    graphics_mt: &GraphicsMultiThreaded,
    buffer_object_handle: &GraphicsBufferObjectHandle,
    backend_handle: &GraphicsBackendHandle,
    texture_handle: &GraphicsTextureHandle,
    action: EditorAction,
    map: &mut EditorMap,
) -> anyhow::Result<()> {
    match action {
        EditorAction::SwapGroups(act) => {
            let groups = if act.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            anyhow::ensure!(
                groups.get(act.group1).is_some(),
                "group {} is out of bounds",
                act.group1
            );
            anyhow::ensure!(
                groups.get(act.group2).is_some(),
                "group {} is out of bounds",
                act.group2
            );
            groups.swap(act.group1, act.group2);
        }
        EditorAction::SwapLayers(act) => {
            let groups = if act.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            let group = groups
                .get_mut(act.group)
                .ok_or(anyhow!("group {} is out of bounds", act.group))?;
            anyhow::ensure!(
                group.layers.get(act.layer1).is_some(),
                "layer {} is out of bounds in group {}",
                act.layer1,
                act.group
            );
            anyhow::ensure!(
                group.layers.get(act.layer2).is_some(),
                "layer {} is out of bounds in group {}",
                act.layer2,
                act.group
            );
            group.layers.swap(act.layer1, act.layer2);
        }
        EditorAction::AddImage(act) => {
            anyhow::ensure!(
                act.base.index <= map.resources.images.len(),
                "{} is out of bounds for image resources",
                act.base.index
            );
            let mut img_mem = None;
            let _ = load_png_image_as_rgba(&act.base.file, |width, height, _| {
                img_mem = Some(backend_handle.mem_alloc(
                    GraphicsMemoryAllocationType::TextureRgbaU8 {
                        width: width.try_into().unwrap(),
                        height: height.try_into().unwrap(),
                        flags: TexFlags::empty(),
                    },
                ));
                img_mem.as_mut().unwrap().as_mut_slice()
            })?;
            map.resources.images.insert(
                act.base.index,
                EditorImage {
                    user: EditorResource {
                        user: texture_handle
                            .load_texture_rgba_u8(img_mem.unwrap(), act.base.res.name.as_str())?,
                        file: Rc::new(act.base.file.clone()),
                        hq: None,
                    },
                    def: act.base.res,
                },
            );
        }
        EditorAction::AddImage2dArray(act) => {
            anyhow::ensure!(
                act.base.index <= map.resources.image_arrays.len(),
                "{} is out of bounds for image 2d array resources",
                act.base.index
            );
            let mut png = Vec::new();
            let img = load_png_image_as_rgba(&act.base.file, |width, height, _| {
                png = vec![0; width * height * 4];
                &mut png
            })?;
            let mut mem =
                graphics_mt.mem_alloc(GraphicsMemoryAllocationType::TextureRgbaU82dArray {
                    width: ((img.width / 16) as usize).try_into().unwrap(),
                    height: ((img.height / 16) as usize).try_into().unwrap(),
                    depth: 256.try_into().unwrap(),
                    flags: TexFlags::empty(),
                });
            let mut image_3d_width = 0;
            let mut image_3d_height = 0;
            if !texture_2d_to_3d(
                tp,
                img.data,
                img.width as usize,
                img.height as usize,
                4,
                16,
                16,
                mem.as_mut_slice(),
                &mut image_3d_width,
                &mut image_3d_height,
            ) {
                return Err(anyhow!(
                    "fatal error, could not convert 2d texture to 2d array texture"
                ));
            }
            // ALWAYS clear pixels of first tile, some mapres still have pixels in them
            mem.as_mut_slice()[0..image_3d_width * image_3d_height * 4]
                .iter_mut()
                .for_each(|byte| *byte = 0);
            map.resources.image_arrays.insert(
                act.base.index,
                EditorImage2dArray {
                    user: EditorResource {
                        user: texture_handle
                            .load_texture_3d_rgba_u8(mem, act.base.res.name.as_str())?,
                        file: Rc::new(act.base.file.clone()),
                        hq: None,
                    },
                    def: act.base.res,
                },
            );
        }
        EditorAction::AddSound(act) => {
            anyhow::ensure!(
                act.base.index <= map.resources.sounds.len(),
                "{} is out of bounds for sound resources",
                act.base.index
            );
            map.resources.sounds.insert(
                act.base.index,
                EditorSound {
                    def: act.base.res,
                    user: EditorResource {
                        user: {
                            let mut mem = sound_mt.mem_alloc(act.base.file.len());
                            mem.as_mut_slice().copy_from_slice(&act.base.file);
                            map.user.sound_scene.sound_object_handle.create(mem)
                        },
                        file: Rc::new(act.base.file),
                        hq: None,
                    },
                },
            );
        }
        EditorAction::RemImage(ActRemImage {
            base: ActAddRemImage { index, .. },
        }) => {
            anyhow::ensure!(
                index < map.resources.images.len(),
                "{} is out of bounds for image resources",
                index
            );
            map.resources.images.remove(index);
        }
        EditorAction::RemImage2dArray(ActRemImage2dArray {
            base: ActAddRemImage { index, .. },
        }) => {
            anyhow::ensure!(
                index < map.resources.image_arrays.len(),
                "{} is out of bounds for image 2d array resources",
                index
            );
            map.resources.image_arrays.remove(index);
        }
        EditorAction::RemSound(ActRemSound {
            base: ActAddRemSound { index, .. },
        }) => {
            anyhow::ensure!(
                index < map.resources.sounds.len(),
                "{} is out of bounds for sound resources",
                index
            );
            map.resources.sounds.remove(index);
        }
        EditorAction::LayerChangeImageIndex(act) => {
            let groups = if act.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            let group = groups
                .get_mut(act.group_index)
                .ok_or(anyhow!("group {} is out of bounds", act.group_index))?;
            let map_layer = group.layers.get_mut(act.layer_index).ok_or(anyhow!(
                "layer {} is out of bounds in group {}",
                act.layer_index,
                act.group_index
            ))?;
            if let EditorLayer::Tile(EditorLayerTile {
                layer:
                    MapLayerTile {
                        attr:
                            MapTileLayerAttr {
                                image_array: image, ..
                            },
                        ..
                    },
                ..
            })
            | EditorLayer::Quad(EditorLayerQuad {
                layer:
                    MapLayerQuad {
                        attr: MapLayerQuadsAttrs { image, .. },
                        ..
                    },
                ..
            }) = map_layer
            {
                let was_tex_changed = (image.is_none() && act.new_index.is_some())
                    || (act.new_index.is_none() && image.is_some());
                *image = act.new_index;
                if was_tex_changed {
                    match map_layer {
                        MapLayerSkeleton::Tile(EditorLayerTile { user, layer }) => {
                            user.visuals = {
                                let buffer = tp.install(|| {
                                    upload_design_tile_layer_buffer(
                                        graphics_mt,
                                        &layer.tiles,
                                        layer.attr.width,
                                        layer.attr.height,
                                        layer.attr.image_array.is_some(),
                                    )
                                });
                                finish_design_tile_layer_buffer(
                                    buffer_object_handle,
                                    backend_handle,
                                    buffer,
                                )
                            };
                        }
                        MapLayerSkeleton::Quad(EditorLayerQuad { user, layer }) => {
                            user.visuals = {
                                let buffer = tp.install(|| {
                                    upload_design_quad_layer_buffer(
                                        graphics_mt,
                                        &layer.attr,
                                        &layer.quads,
                                    )
                                });
                                finish_design_quad_layer_buffer(
                                    buffer_object_handle,
                                    backend_handle,
                                    buffer,
                                )
                            }
                        }
                        _ => {}
                    }
                }
            } else {
                return Err(anyhow!("not a tile (design) or quad layer"));
            }
        }
        EditorAction::LayerChangeSoundIndex(act) => {
            let groups = if act.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            if let EditorLayer::Sound(EditorLayerSound {
                layer:
                    MapLayerSound {
                        attr: MapLayerSoundAttrs { sound, .. },
                        ..
                    },
                ..
            }) = groups
                .get_mut(act.group_index)
                .ok_or(anyhow!("group {} is out of bounds", act.group_index))?
                .layers
                .get_mut(act.layer_index)
                .ok_or(anyhow!(
                    "layer {} is out of bounds in group {}",
                    act.layer_index,
                    act.group_index
                ))?
            {
                *sound = act.new_index;
            }
        }
        EditorAction::QuadLayerAddQuads(act) => {
            let groups = if act.base.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            if let EditorLayer::Quad(EditorLayerQuad { layer, user }) = groups
                .get_mut(act.base.group_index)
                .ok_or(anyhow!("group {} is out of bounds", act.base.group_index))?
                .layers
                .get_mut(act.base.layer_index)
                .ok_or(anyhow!(
                    "layer {} is out of bounds in group {}",
                    act.base.layer_index,
                    act.base.group_index
                ))?
            {
                anyhow::ensure!(
                    act.base.index <= layer.quads.len(),
                    "quad index {} out of bounds",
                    act.base.index
                );
                layer
                    .quads
                    .splice(act.base.index..act.base.index, act.base.quads);
                user.visuals = {
                    let buffer = tp.install(|| {
                        upload_design_quad_layer_buffer(graphics_mt, &layer.attr, &layer.quads)
                    });
                    finish_design_quad_layer_buffer(buffer_object_handle, backend_handle, buffer)
                };
            }
        }
        EditorAction::SoundLayerAddSounds(act) => {
            let groups = if act.base.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            if let EditorLayer::Sound(EditorLayerSound {
                layer: MapLayerSound { sounds, .. },
                ..
            }) = groups
                .get_mut(act.base.group_index)
                .ok_or(anyhow!("group {} is out of bounds", act.base.group_index))?
                .layers
                .get_mut(act.base.layer_index)
                .ok_or(anyhow!(
                    "layer {} is out of bounds in group {}",
                    act.base.layer_index,
                    act.base.group_index
                ))?
            {
                anyhow::ensure!(
                    act.base.index <= sounds.len(),
                    "sound index {} out of bounds",
                    act.base.index
                );
                sounds.splice(act.base.index..act.base.index, act.base.sounds);
            }
        }
        EditorAction::QuadLayerRemQuads(act) => {
            let groups = if act.base.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            if let EditorLayer::Quad(EditorLayerQuad { layer, user }) = groups
                .get_mut(act.base.group_index)
                .ok_or(anyhow!("group {} is out of bounds", act.base.group_index))?
                .layers
                .get_mut(act.base.layer_index)
                .ok_or(anyhow!(
                    "layer {} is out of bounds in group {}",
                    act.base.layer_index,
                    act.base.group_index
                ))?
            {
                anyhow::ensure!(
                    act.base.index + act.base.quads.len() <= layer.quads.len(),
                    "quad index {} out of bounds",
                    act.base.index
                );
                layer
                    .quads
                    .splice(act.base.index..act.base.index + act.base.quads.len(), []);
                user.visuals = {
                    let buffer = tp.install(|| {
                        upload_design_quad_layer_buffer(graphics_mt, &layer.attr, &layer.quads)
                    });
                    finish_design_quad_layer_buffer(buffer_object_handle, backend_handle, buffer)
                };
            }
        }
        EditorAction::SoundLayerRemSounds(act) => {
            let groups = if act.base.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            if let EditorLayer::Sound(EditorLayerSound {
                layer: MapLayerSound { sounds, .. },
                ..
            }) = &mut groups
                .get_mut(act.base.group_index)
                .ok_or(anyhow!("group {} is out of bounds", act.base.group_index))?
                .layers
                .get_mut(act.base.layer_index)
                .ok_or(anyhow!(
                    "layer {} is out of bounds in group {}",
                    act.base.layer_index,
                    act.base.group_index
                ))?
            {
                anyhow::ensure!(
                    act.base.index + act.base.sounds.len() <= sounds.len(),
                    "sound index {} out of bounds",
                    act.base.index
                );
                sounds.splice(act.base.index..act.base.index + act.base.sounds.len(), []);
            }
        }
        EditorAction::AddTileLayer(act) => {
            let groups = if act.base.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            let group = groups
                .get_mut(act.base.group_index)
                .ok_or(anyhow!("group {} is out of bounds", act.base.group_index))?;
            anyhow::ensure!(
                act.base.index <= group.layers.len(),
                "layer index {} is out of bounds in group {}",
                act.base.index,
                act.base.group_index
            );
            let layer = act.base.layer;
            let visuals = {
                let buffer = tp.install(|| {
                    upload_design_tile_layer_buffer(
                        graphics_mt,
                        &layer.tiles,
                        layer.attr.width,
                        layer.attr.height,
                        layer.attr.image_array.is_some(),
                    )
                });
                finish_design_tile_layer_buffer(buffer_object_handle, backend_handle, buffer)
            };
            group.layers.insert(
                act.base.index,
                EditorLayer::Tile(EditorLayerTile {
                    layer,
                    user: EditorTileLayerProps {
                        visuals,
                        attr: EditorCommonGroupOrLayerAttr::default(),
                        selected: Default::default(),
                    },
                }),
            );
        }
        EditorAction::AddQuadLayer(act) => {
            let groups = if act.base.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            let group = groups
                .get_mut(act.base.group_index)
                .ok_or(anyhow!("group {} is out of bounds", act.base.group_index))?;
            anyhow::ensure!(
                act.base.index <= group.layers.len(),
                "layer index {} is out of bounds in group {}",
                act.base.index,
                act.base.group_index
            );
            let layer = act.base.layer;
            let visuals = {
                let buffer = tp.install(|| {
                    upload_design_quad_layer_buffer(graphics_mt, &layer.attr, &layer.quads)
                });
                finish_design_quad_layer_buffer(buffer_object_handle, backend_handle, buffer)
            };
            group.layers.insert(
                act.base.index,
                EditorLayer::Quad(EditorLayerQuad {
                    layer,
                    user: EditorQuadLayerProps {
                        visuals,
                        attr: EditorCommonGroupOrLayerAttr::default(),
                        selected: Default::default(),
                    },
                }),
            );
        }
        EditorAction::AddSoundLayer(act) => {
            let groups = if act.base.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            let group = groups
                .get_mut(act.base.group_index)
                .ok_or(anyhow!("group {} is out of bounds", act.base.group_index))?;
            anyhow::ensure!(
                act.base.index <= group.layers.len(),
                "layer index {} is out of bounds in group {}",
                act.base.index,
                act.base.group_index
            );
            anyhow::ensure!(
                act.base
                    .layer
                    .attr
                    .sound
                    .is_none_or(|index| index < map.resources.sounds.len()),
                "the sound used in this layer is out bounds {} vs. length of {}",
                act.base.layer.attr.sound.unwrap_or_default(),
                map.resources.sounds.len()
            );
            group.layers.insert(
                act.base.index,
                EditorLayer::Sound(EditorLayerSound {
                    user: EditorSoundLayerProps {
                        attr: EditorCommonGroupOrLayerAttr::default(),
                        selected: Default::default(),
                        sounds: SoundLayerSounds::default(),
                    },
                    layer: act.base.layer,
                }),
            );
        }
        EditorAction::RemTileLayer(ActRemTileLayer {
            base:
                ActAddRemTileLayer {
                    is_background,
                    group_index,
                    index,
                    ..
                },
        })
        | EditorAction::RemQuadLayer(ActRemQuadLayer {
            base:
                ActAddRemQuadLayer {
                    is_background,
                    group_index,
                    index,
                    ..
                },
        })
        | EditorAction::RemSoundLayer(ActRemSoundLayer {
            base:
                ActAddRemSoundLayer {
                    is_background,
                    group_index,
                    index,
                    ..
                },
        }) => {
            let groups = if is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            let group = groups
                .get_mut(group_index)
                .ok_or(anyhow!("group {} is out of bounds", group_index))?;
            anyhow::ensure!(
                index < group.layers.len(),
                "layer index {} out of bounds in group {}",
                index,
                group_index
            );
            group.layers.remove(index);
        }
        EditorAction::AddPhysicsTileLayer(act) => {
            let physics = &mut map.groups.physics;
            anyhow::ensure!(
                act.base.index <= physics.layers.len(),
                "layer index {} is out of bounds in physics group",
                act.base.index,
            );
            let layer = act.base.layer;
            let visuals = {
                let buffer = tp.install(|| {
                    upload_physics_layer_buffer(
                        graphics_mt,
                        physics.attr.width,
                        physics.attr.height,
                        layer.as_ref().tiles_ref(),
                    )
                });
                finish_physics_layer_buffer(buffer_object_handle, backend_handle, buffer)
            };
            physics.layers.insert(
                act.base.index,
                match layer {
                    MapLayerPhysics::Arbitrary(_) => {
                        return Err(anyhow!("arbitrary layers are not supported"));
                    }
                    MapLayerPhysics::Game(layer) => {
                        EditorPhysicsLayer::Game(MapLayerTilePhysicsBaseSkeleton {
                            layer,
                            user: EditorPhysicsLayerProps {
                                visuals,
                                attr: Default::default(),
                                selected: Default::default(),
                                number_extra: Default::default(),
                                number_extra_texts: Default::default(),
                                context_menu_open: false,
                            },
                        })
                    }
                    MapLayerPhysics::Front(layer) => {
                        EditorPhysicsLayer::Front(MapLayerTilePhysicsBaseSkeleton {
                            layer,
                            user: EditorPhysicsLayerProps {
                                visuals,
                                attr: Default::default(),
                                selected: Default::default(),
                                number_extra: Default::default(),
                                number_extra_texts: Default::default(),
                                context_menu_open: false,
                            },
                        })
                    }
                    MapLayerPhysics::Tele(layer) => {
                        EditorPhysicsLayer::Tele(MapLayerTelePhysicsSkeleton {
                            layer,
                            user: EditorPhysicsLayerProps {
                                visuals,
                                attr: Default::default(),
                                selected: Default::default(),
                                number_extra: Default::default(),
                                number_extra_texts: Default::default(),
                                context_menu_open: false,
                            },
                        })
                    }
                    MapLayerPhysics::Speedup(layer) => {
                        EditorPhysicsLayer::Speedup(MapLayerTilePhysicsBaseSkeleton {
                            layer,
                            user: EditorPhysicsLayerProps {
                                visuals,
                                attr: Default::default(),
                                selected: Default::default(),
                                number_extra: Default::default(),
                                number_extra_texts: Default::default(),
                                context_menu_open: false,
                            },
                        })
                    }
                    MapLayerPhysics::Switch(layer) => {
                        EditorPhysicsLayer::Switch(MapLayerSwitchPhysicsSkeleton {
                            layer,
                            user: EditorPhysicsLayerProps {
                                visuals,
                                attr: Default::default(),
                                selected: Default::default(),
                                number_extra: Default::default(),
                                number_extra_texts: Default::default(),
                                context_menu_open: false,
                            },
                        })
                    }
                    MapLayerPhysics::Tune(layer) => {
                        EditorPhysicsLayer::Tune(MapLayerTunePhysicsSkeleton {
                            layer,
                            user: EditorPhysicsLayerProps {
                                visuals,
                                attr: Default::default(),
                                selected: Default::default(),
                                number_extra: Default::default(),
                                number_extra_texts: Default::default(),
                                context_menu_open: false,
                            },
                        })
                    }
                },
            );
        }
        EditorAction::RemPhysicsTileLayer(act) => {
            let physics = &mut map.groups.physics;
            let index = act.base.index;
            anyhow::ensure!(
                index < physics.layers.len(),
                "layer index {} out of bounds in physics group",
                index,
            );
            physics.layers.remove(index);
        }
        EditorAction::TileLayerReplaceTiles(act) => {
            let groups = if act.base.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            if let EditorLayer::Tile(layer) = groups
                .get_mut(act.base.group_index)
                .ok_or(anyhow!("group {} is out of bounds", act.base.group_index))?
                .layers
                .get_mut(act.base.layer_index)
                .ok_or(anyhow!(
                    "layer {} is out of bounds in group {}",
                    act.base.layer_index,
                    act.base.group_index
                ))?
            {
                let copy_tiles = &act.base.new_tiles;
                anyhow::ensure!(
                    (act.base.x as usize + act.base.w.get() as usize)
                        <= layer.layer.attr.width.get() as usize,
                    "{} + {} was out of bounds for layer {} with width {}",
                    act.base.x,
                    act.base.w,
                    act.base.layer_index,
                    layer.layer.attr.width
                );
                anyhow::ensure!(
                    (act.base.y as usize + act.base.h.get() as usize)
                        <= layer.layer.attr.height.get() as usize,
                    "{} + {} was out of bounds for layer {} with height {}",
                    act.base.x,
                    act.base.w,
                    act.base.layer_index,
                    layer.layer.attr.height
                );
                anyhow::ensure!(
                    act.base.h.get() as usize * act.base.w.get() as usize
                        == act.base.new_tiles.len(),
                    "brush tiles were not equal to the copy w * h in layer {}",
                    act.base.layer_index,
                );
                layer
                    .layer
                    .tiles
                    .chunks_mut(layer.layer.attr.width.get() as usize)
                    .skip(act.base.y as usize)
                    .take(act.base.h.get() as usize)
                    .enumerate()
                    .for_each(|(index, chunk)| {
                        let copy_tiles_y_offset = index * (act.base.w.get() as usize);
                        chunk[act.base.x as usize..(act.base.x + act.base.w.get()) as usize]
                            .copy_from_slice(
                                &copy_tiles[copy_tiles_y_offset
                                    ..copy_tiles_y_offset + act.base.w.get() as usize],
                            );
                    });
                // update the visual buffer too
                update_design_tile_layer(tp, layer, act.base.x, act.base.y, act.base.w, act.base.h);
            } else {
                return Err(anyhow!("not a tile layer"));
            }
        }
        EditorAction::TilePhysicsLayerReplaceTiles(act) => {
            let group = &mut map.groups.physics;
            let group_width = group.attr.width;
            let group_height = group.attr.height;
            let layer = group.layers.get_mut(act.base.layer_index).ok_or(anyhow!(
                "layer {} is out of bounds in physics group",
                act.base.layer_index,
            ))?;

            anyhow::ensure!(
                (act.base.x as usize + act.base.w.get() as usize)
                    <= group.attr.width.get() as usize,
                "{} + {} was out of bounds for layer {} with width {}",
                act.base.x,
                act.base.w,
                act.base.layer_index,
                group.attr.width
            );
            anyhow::ensure!(
                (act.base.y as usize + act.base.h.get() as usize)
                    <= group.attr.height.get() as usize,
                "{} + {} was out of bounds for layer {} with height {}",
                act.base.x,
                act.base.w,
                act.base.layer_index,
                group.attr.height
            );
            anyhow::ensure!(
                act.base.h.get() as usize * act.base.w.get() as usize
                    == act.base.new_tiles.tiles_count(),
                "brush tiles were not equal to the copy w * h in layer {}",
                act.base.layer_index,
            );
            match layer {
                MapLayerPhysicsSkeleton::Arbitrary(_) => {
                    return Err(anyhow!("arbitrary tiles are not supported by this editor."));
                }
                MapLayerPhysicsSkeleton::Game(layer) => {
                    let MapTileLayerPhysicsTiles::Game(copy_tiles) = act.base.new_tiles else {
                        return Err(anyhow!("tiles are not compatible"));
                    };
                    layer
                        .layer
                        .tiles
                        .chunks_mut(group.attr.width.get() as usize)
                        .skip(act.base.y as usize)
                        .take(act.base.h.get() as usize)
                        .enumerate()
                        .for_each(|(index, chunk)| {
                            let copy_tiles_y_offset = index * (act.base.w.get() as usize);
                            chunk[act.base.x as usize..(act.base.x + act.base.w.get()) as usize]
                                .copy_from_slice(
                                    &copy_tiles[copy_tiles_y_offset
                                        ..copy_tiles_y_offset + act.base.w.get() as usize],
                                );
                        });
                }
                MapLayerPhysicsSkeleton::Front(layer) => {
                    let MapTileLayerPhysicsTiles::Front(copy_tiles) = act.base.new_tiles else {
                        return Err(anyhow!("tiles are not compatible"));
                    };
                    layer
                        .layer
                        .tiles
                        .chunks_mut(group.attr.width.get() as usize)
                        .skip(act.base.y as usize)
                        .take(act.base.h.get() as usize)
                        .enumerate()
                        .for_each(|(index, chunk)| {
                            let copy_tiles_y_offset = index * (act.base.w.get() as usize);
                            chunk[act.base.x as usize..(act.base.x + act.base.w.get()) as usize]
                                .copy_from_slice(
                                    &copy_tiles[copy_tiles_y_offset
                                        ..copy_tiles_y_offset + act.base.w.get() as usize],
                                );
                        });
                }
                MapLayerPhysicsSkeleton::Tele(layer) => {
                    let MapTileLayerPhysicsTiles::Tele(copy_tiles) = act.base.new_tiles else {
                        return Err(anyhow!("tiles are not compatible"));
                    };
                    layer
                        .layer
                        .base
                        .tiles
                        .chunks_mut(group.attr.width.get() as usize)
                        .skip(act.base.y as usize)
                        .take(act.base.h.get() as usize)
                        .enumerate()
                        .for_each(|(index, chunk)| {
                            let copy_tiles_y_offset = index * (act.base.w.get() as usize);
                            chunk[act.base.x as usize..(act.base.x + act.base.w.get()) as usize]
                                .copy_from_slice(
                                    &copy_tiles[copy_tiles_y_offset
                                        ..copy_tiles_y_offset + act.base.w.get() as usize],
                                );
                        });
                }
                MapLayerPhysicsSkeleton::Speedup(layer) => {
                    let MapTileLayerPhysicsTiles::Speedup(copy_tiles) = act.base.new_tiles else {
                        return Err(anyhow!("tiles are not compatible"));
                    };
                    layer
                        .layer
                        .tiles
                        .chunks_mut(group.attr.width.get() as usize)
                        .skip(act.base.y as usize)
                        .take(act.base.h.get() as usize)
                        .enumerate()
                        .for_each(|(index, chunk)| {
                            let copy_tiles_y_offset = index * (act.base.w.get() as usize);
                            chunk[act.base.x as usize..(act.base.x + act.base.w.get()) as usize]
                                .copy_from_slice(
                                    &copy_tiles[copy_tiles_y_offset
                                        ..copy_tiles_y_offset + act.base.w.get() as usize],
                                );
                        });
                }
                MapLayerPhysicsSkeleton::Switch(layer) => {
                    let MapTileLayerPhysicsTiles::Switch(copy_tiles) = act.base.new_tiles else {
                        return Err(anyhow!("tiles are not compatible"));
                    };
                    layer
                        .layer
                        .base
                        .tiles
                        .chunks_mut(group.attr.width.get() as usize)
                        .skip(act.base.y as usize)
                        .take(act.base.h.get() as usize)
                        .enumerate()
                        .for_each(|(index, chunk)| {
                            let copy_tiles_y_offset = index * (act.base.w.get() as usize);
                            chunk[act.base.x as usize..(act.base.x + act.base.w.get()) as usize]
                                .copy_from_slice(
                                    &copy_tiles[copy_tiles_y_offset
                                        ..copy_tiles_y_offset + act.base.w.get() as usize],
                                );
                        });
                }
                MapLayerPhysicsSkeleton::Tune(layer) => {
                    let MapTileLayerPhysicsTiles::Tune(copy_tiles) = act.base.new_tiles else {
                        return Err(anyhow!("tiles are not compatible"));
                    };
                    layer
                        .layer
                        .base
                        .tiles
                        .chunks_mut(group.attr.width.get() as usize)
                        .skip(act.base.y as usize)
                        .take(act.base.h.get() as usize)
                        .enumerate()
                        .for_each(|(index, chunk)| {
                            let copy_tiles_y_offset = index * (act.base.w.get() as usize);
                            chunk[act.base.x as usize..(act.base.x + act.base.w.get()) as usize]
                                .copy_from_slice(
                                    &copy_tiles[copy_tiles_y_offset
                                        ..copy_tiles_y_offset + act.base.w.get() as usize],
                                );
                        });
                }
            }

            update_physics_layer(
                tp,
                group_width,
                group_height,
                layer,
                act.base.x,
                act.base.y,
                act.base.w,
                act.base.h,
            );
        }
        EditorAction::AddGroup(act) => {
            let groups = if act.base.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            anyhow::ensure!(
                act.base.index <= groups.len(),
                "group index {} is out of bounds",
                act.base.index
            );
            groups.insert(
                act.base.index,
                EditorGroup {
                    attr: act.base.group.attr,
                    layers: act
                        .base
                        .group
                        .layers
                        .into_iter()
                        .map(|layer| {
                            anyhow::Ok(match layer {
                                MapLayer::Abritrary(_) => {
                                    Err(anyhow!("abritrary layer cannot be created."))?
                                }
                                MapLayer::Tile(layer) => EditorLayer::Tile(EditorLayerTile {
                                    user: EditorTileLayerProps {
                                        visuals: {
                                            let buffer = tp.install(|| {
                                                upload_design_tile_layer_buffer(
                                                    graphics_mt,
                                                    &layer.tiles,
                                                    layer.attr.width,
                                                    layer.attr.height,
                                                    layer.attr.image_array.is_some(),
                                                )
                                            });
                                            finish_design_tile_layer_buffer(
                                                buffer_object_handle,
                                                backend_handle,
                                                buffer,
                                            )
                                        },
                                        attr: EditorCommonGroupOrLayerAttr::default(),
                                        selected: Default::default(),
                                    },
                                    layer,
                                }),
                                MapLayer::Quad(layer) => EditorLayer::Quad(EditorLayerQuad {
                                    user: EditorQuadLayerProps {
                                        visuals: {
                                            let buffer = tp.install(|| {
                                                upload_design_quad_layer_buffer(
                                                    graphics_mt,
                                                    &layer.attr,
                                                    &layer.quads,
                                                )
                                            });
                                            finish_design_quad_layer_buffer(
                                                buffer_object_handle,
                                                backend_handle,
                                                buffer,
                                            )
                                        },
                                        attr: EditorCommonGroupOrLayerAttr::default(),
                                        selected: Default::default(),
                                    },
                                    layer,
                                }),
                                MapLayer::Sound(layer) => EditorLayer::Sound(EditorLayerSound {
                                    user: EditorSoundLayerProps {
                                        attr: EditorCommonGroupOrLayerAttr::default(),
                                        selected: Default::default(),
                                        sounds: SoundLayerSounds::default(),
                                    },
                                    layer,
                                }),
                            })
                        })
                        .collect::<anyhow::Result<_>>()?,
                    name: act.base.group.name,
                    user: EditorGroupProps::default(),
                },
            );
        }
        EditorAction::RemGroup(act) => {
            let groups = if act.base.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            anyhow::ensure!(
                act.base.index < groups.len(),
                "group index {} is out of bounds",
                act.base.index
            );
            groups.remove(act.base.index);
        }
        EditorAction::ChangeGroupAttr(act) => {
            let groups = if act.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            groups
                .get_mut(act.group_index)
                .ok_or(anyhow!("group {} is out of bounds", act.group_index))?
                .attr = act.new_attr;
        }
        EditorAction::ChangePhysicsGroupAttr(act) => {
            let group = &mut map.groups.physics;

            // checks
            anyhow::ensure!(
                group.layers.len() == act.new_layer_tiles.len(),
                "size mismatch between physics layers and physics tiles for all layers"
            );
            for (layer, new_tiles) in group.layers.iter().zip(act.new_layer_tiles.iter()) {
                match layer {
                    MapLayerPhysicsSkeleton::Arbitrary(_) => {
                        return Err(anyhow!("arbitrary physics layers are not supported."));
                    }
                    MapLayerPhysicsSkeleton::Game(_) => {
                        anyhow::ensure!(matches!(new_tiles, MapTileLayerPhysicsTiles::Game(_)));
                    }
                    MapLayerPhysicsSkeleton::Front(_) => {
                        anyhow::ensure!(matches!(new_tiles, MapTileLayerPhysicsTiles::Front(_)));
                    }
                    MapLayerPhysicsSkeleton::Tele(_) => {
                        anyhow::ensure!(matches!(new_tiles, MapTileLayerPhysicsTiles::Tele(_)));
                    }
                    MapLayerPhysicsSkeleton::Speedup(_) => {
                        anyhow::ensure!(matches!(new_tiles, MapTileLayerPhysicsTiles::Speedup(_)));
                    }
                    MapLayerPhysicsSkeleton::Switch(_) => {
                        anyhow::ensure!(matches!(new_tiles, MapTileLayerPhysicsTiles::Switch(_)));
                    }
                    MapLayerPhysicsSkeleton::Tune(_) => {
                        anyhow::ensure!(matches!(new_tiles, MapTileLayerPhysicsTiles::Tune(_)));
                    }
                }
            }

            let width_or_height_change =
                group.attr.width != act.new_attr.width || group.attr.height != act.new_attr.height;
            group.attr = act.new_attr;
            if width_or_height_change {
                let width = group.attr.width;
                let height = group.attr.height;
                let new_tiles = act.new_layer_tiles;
                let buffers: Vec<_> = tp.install(|| {
                    new_tiles
                        .into_par_iter()
                        .map(|new_tiles| {
                            (
                                upload_physics_layer_buffer(
                                    graphics_mt,
                                    width,
                                    height,
                                    new_tiles.as_ref(),
                                ),
                                new_tiles,
                            )
                        })
                        .collect()
                });

                for (layer, (buffer, new_tiles)) in group.layers.iter_mut().zip(buffers.into_iter())
                {
                    match layer {
                        MapLayerPhysicsSkeleton::Arbitrary(_) => {
                            return Err(anyhow!("arbitrary physics layers are not supported"))
                        }
                        MapLayerPhysicsSkeleton::Game(layer) => {
                            let MapTileLayerPhysicsTiles::Game(tiles) = new_tiles else {
                                return Err(anyhow!("not physics game tiles"));
                            };
                            layer.layer.tiles = tiles;
                        }
                        MapLayerPhysicsSkeleton::Front(layer) => {
                            let MapTileLayerPhysicsTiles::Front(tiles) = new_tiles else {
                                return Err(anyhow!("not physics front tiles"));
                            };
                            layer.layer.tiles = tiles;
                        }
                        MapLayerPhysicsSkeleton::Tele(layer) => {
                            let MapTileLayerPhysicsTiles::Tele(tiles) = new_tiles else {
                                return Err(anyhow!("not physics tele tiles"));
                            };
                            layer.layer.base.tiles = tiles;
                        }
                        MapLayerPhysicsSkeleton::Speedup(layer) => {
                            let MapTileLayerPhysicsTiles::Speedup(tiles) = new_tiles else {
                                return Err(anyhow!("not physics speedup tiles"));
                            };
                            layer.layer.tiles = tiles;
                        }
                        MapLayerPhysicsSkeleton::Switch(layer) => {
                            let MapTileLayerPhysicsTiles::Switch(tiles) = new_tiles else {
                                return Err(anyhow!("not physics switch tiles"));
                            };
                            layer.layer.base.tiles = tiles;
                        }
                        MapLayerPhysicsSkeleton::Tune(layer) => {
                            let MapTileLayerPhysicsTiles::Tune(tiles) = new_tiles else {
                                return Err(anyhow!("not physics tune tiles"));
                            };
                            layer.layer.base.tiles = tiles;
                        }
                    }
                    layer.user_mut().visuals =
                        finish_physics_layer_buffer(buffer_object_handle, backend_handle, buffer)
                }
            }
        }
        EditorAction::ChangeTileLayerDesignAttr(act) => {
            let groups = if act.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            if let EditorLayer::Tile(layer) = groups
                .get_mut(act.group_index)
                .ok_or(anyhow!("group {} is out of bounds", act.group_index))?
                .layers
                .get_mut(act.layer_index)
                .ok_or(anyhow!(
                    "layer {} is out of bounds in group {}",
                    act.layer_index,
                    act.group_index
                ))?
            {
                let has_tex_change = (layer.layer.attr.image_array.is_some()
                    && act.new_attr.image_array.is_none())
                    || (layer.layer.attr.image_array.is_none()
                        && act.new_attr.image_array.is_some());
                let width_or_height_change = layer.layer.attr.width != act.new_attr.width
                    || layer.layer.attr.height != act.new_attr.height;
                let needs_visual_recreate = width_or_height_change || has_tex_change;
                layer.layer.attr = act.new_attr.clone();
                if needs_visual_recreate {
                    if width_or_height_change {
                        layer.layer.tiles = act.new_tiles.clone();
                    }

                    layer.user.visuals = {
                        let layer = &layer.layer;
                        let buffer = tp.install(|| {
                            upload_design_tile_layer_buffer(
                                graphics_mt,
                                &layer.tiles,
                                layer.attr.width,
                                layer.attr.height,
                                layer.attr.image_array.is_some(),
                            )
                        });
                        finish_design_tile_layer_buffer(
                            buffer_object_handle,
                            backend_handle,
                            buffer,
                        )
                    };
                }
            } else {
                return Err(anyhow!("not a design tile layer"));
            }
        }
        EditorAction::ChangeQuadLayerAttr(act) => {
            let groups = if act.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            if let EditorLayer::Quad(layer) = groups
                .get_mut(act.group_index)
                .ok_or(anyhow!("group {} is out of bounds", act.group_index))?
                .layers
                .get_mut(act.layer_index)
                .ok_or(anyhow!(
                    "layer {} is out of bounds in group {}",
                    act.layer_index,
                    act.group_index
                ))?
            {
                let has_tex_change = (layer.layer.attr.image.is_none()
                    && act.new_attr.image.is_some())
                    || (layer.layer.attr.image.is_some() && act.new_attr.image.is_none());
                layer.layer.attr = act.new_attr;
                if has_tex_change {
                    layer.user = EditorQuadLayerProps {
                        visuals: {
                            let buffer = tp.install(|| {
                                upload_design_quad_layer_buffer(
                                    graphics_mt,
                                    &layer.layer.attr,
                                    &layer.layer.quads,
                                )
                            });
                            finish_design_quad_layer_buffer(
                                buffer_object_handle,
                                backend_handle,
                                buffer,
                            )
                        },
                        attr: EditorCommonGroupOrLayerAttr::default(),
                        selected: Default::default(),
                    };
                }
            } else {
                return Err(anyhow!("not a quad layer"));
            }
        }
        EditorAction::ChangeSoundLayerAttr(act) => {
            let groups = if act.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            if let EditorLayer::Sound(layer) = groups
                .get_mut(act.group_index)
                .ok_or(anyhow!("group {} is out of bounds", act.group_index))?
                .layers
                .get_mut(act.layer_index)
                .ok_or(anyhow!(
                    "layer {} is out of bounds in group {}",
                    act.layer_index,
                    act.group_index
                ))?
            {
                layer.layer.attr = act.new_attr;
            } else {
                return Err(anyhow!("not a sound layer"));
            }
        }
        EditorAction::ChangeQuadAttr(act) => {
            let groups = if act.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            if let EditorLayer::Quad(layer) = groups
                .get_mut(act.group_index)
                .ok_or(anyhow!("group {} is out of bounds", act.group_index))?
                .layers
                .get_mut(act.layer_index)
                .ok_or(anyhow!(
                    "layer {} is out of bounds in group {}",
                    act.layer_index,
                    act.group_index
                ))?
            {
                *layer
                    .layer
                    .quads
                    .get_mut(act.index)
                    .ok_or(anyhow!("quad index {} is out of bounds", act.index))? = act.new_attr;
                update_design_quad_layer(layer, act.index..act.index + 1);
            } else {
                return Err(anyhow!("not a quad layer"));
            }
        }
        EditorAction::ChangeSoundAttr(act) => {
            let groups = if act.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            if let EditorLayer::Sound(layer) = groups
                .get_mut(act.group_index)
                .ok_or(anyhow!("group {} is out of bounds", act.group_index))?
                .layers
                .get_mut(act.layer_index)
                .ok_or(anyhow!(
                    "layer {} is out of bounds in group {}",
                    act.layer_index,
                    act.group_index
                ))?
            {
                *layer
                    .layer
                    .sounds
                    .get_mut(act.index)
                    .ok_or(anyhow!("sound index {} is out of bounds", act.index))? = act.new_attr;
            } else {
                return Err(anyhow!("not a sound layer"));
            }
        }
        EditorAction::ChangeTeleporter(act) => {
            let physics = &mut map.groups.physics;
            let Some(MapLayerPhysicsSkeleton::Tele(layer)) = physics
                .layers
                .iter_mut()
                .find(|tele| matches!(tele, MapLayerPhysicsSkeleton::Tele(_)))
            else {
                return Err(anyhow!("no tele layer was found"));
            };
            let tele_name = layer
                .layer
                .tele_names
                .entry(act.index)
                .or_insert_with(Default::default);
            *tele_name = act.new_name;
        }
        EditorAction::ChangeSwitch(act) => {
            let physics = &mut map.groups.physics;
            let Some(MapLayerPhysicsSkeleton::Switch(layer)) = physics
                .layers
                .iter_mut()
                .find(|layer| matches!(layer, MapLayerPhysicsSkeleton::Switch(_)))
            else {
                return Err(anyhow!("no switch layer was found"));
            };
            let name = layer
                .layer
                .switch_names
                .entry(act.index)
                .or_insert_with(Default::default);
            *name = act.new_name;
        }
        EditorAction::ChangeTuneZone(act) => {
            let physics = &mut map.groups.physics;
            let Some(MapLayerPhysicsSkeleton::Tune(layer)) = physics
                .layers
                .iter_mut()
                .find(|layer| matches!(layer, MapLayerPhysicsSkeleton::Tune(_)))
            else {
                return Err(anyhow!("no tune layer was found"));
            };
            let zone = layer.layer.tune_zones.entry(act.index).or_insert_with(|| {
                MapLayerTilePhysicsTuneZone {
                    name: "".into(),
                    tunes: Default::default(),
                }
            });
            zone.name = act.new_name;
            zone.tunes = act.new_tunes;
        }
        EditorAction::AddPosAnim(act) => {
            anyhow::ensure!(
                act.base.index <= map.animations.pos.len(),
                "pos anim index {} is out of bounds",
                act.base.index
            );
            map.animations.pos.insert(
                act.base.index,
                EditorPosAnimation {
                    def: act.base.anim,
                    user: EditorAnimationProps::default(),
                },
            );
        }
        EditorAction::RemPosAnim(act) => {
            anyhow::ensure!(
                act.base.index < map.animations.pos.len(),
                "pos anim index {} is out of bounds",
                act.base.index
            );
            map.animations.pos.remove(act.base.index);
        }
        EditorAction::AddColorAnim(act) => {
            anyhow::ensure!(
                act.base.index <= map.animations.color.len(),
                "color anim index {} is out of bounds",
                act.base.index
            );
            map.animations.color.insert(
                act.base.index,
                EditorColorAnimation {
                    def: act.base.anim,
                    user: EditorAnimationProps::default(),
                },
            );
        }
        EditorAction::RemColorAnim(act) => {
            anyhow::ensure!(
                act.base.index < map.animations.color.len(),
                "color anim index {} is out of bounds",
                act.base.index
            );
            map.animations.color.remove(act.base.index);
        }
        EditorAction::AddSoundAnim(act) => {
            anyhow::ensure!(
                act.base.index <= map.animations.sound.len(),
                "sound anim index {} is out of bounds",
                act.base.index
            );
            map.animations.sound.insert(
                act.base.index,
                EditorSoundAnimation {
                    def: act.base.anim,
                    user: EditorAnimationProps::default(),
                },
            );
        }
        EditorAction::RemSoundAnim(act) => {
            anyhow::ensure!(
                act.base.index < map.animations.sound.len(),
                "sound anim index {} is out of bounds",
                act.base.index
            );
            map.animations.sound.remove(act.base.index);
        }
    }
    Ok(())
}

pub fn undo_action(
    tp: &Arc<rayon::ThreadPool>,
    sound_mt: &SoundMultiThreaded,
    graphics_mt: &GraphicsMultiThreaded,
    buffer_object_handle: &GraphicsBufferObjectHandle,
    backend_handle: &GraphicsBackendHandle,
    texture_handle: &GraphicsTextureHandle,
    action: EditorAction,
    map: &mut EditorMap,
) -> anyhow::Result<()> {
    match action {
        EditorAction::SwapGroups(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::SwapGroups(ActSwapGroups {
                is_background: act.is_background,
                group1: act.group2,
                group2: act.group1,
            }),
            map,
        ),
        EditorAction::SwapLayers(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::SwapLayers(ActSwapLayers {
                is_background: act.is_background,
                group: act.group,
                layer1: act.layer2,
                layer2: act.layer1,
            }),
            map,
        ),
        EditorAction::AddImage(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::RemImage(ActRemImage { base: act.base }),
            map,
        ),
        EditorAction::AddImage2dArray(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::RemImage2dArray(ActRemImage2dArray { base: act.base }),
            map,
        ),
        EditorAction::AddSound(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::RemSound(ActRemSound { base: act.base }),
            map,
        ),
        EditorAction::RemImage(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::AddImage(ActAddImage { base: act.base }),
            map,
        ),
        EditorAction::RemImage2dArray(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::AddImage2dArray(ActAddImage2dArray { base: act.base }),
            map,
        ),
        EditorAction::RemSound(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::AddSound(ActAddSound { base: act.base }),
            map,
        ),
        EditorAction::LayerChangeImageIndex(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::LayerChangeImageIndex(ActLayerChangeImageIndex {
                is_background: act.is_background,
                group_index: act.group_index,
                layer_index: act.layer_index,
                new_index: act.old_index,
                old_index: act.new_index,
            }),
            map,
        ),
        EditorAction::LayerChangeSoundIndex(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::LayerChangeSoundIndex(ActLayerChangeSoundIndex {
                is_background: act.is_background,
                group_index: act.group_index,
                layer_index: act.layer_index,
                new_index: act.old_index,
                old_index: act.new_index,
            }),
            map,
        ),
        EditorAction::QuadLayerAddQuads(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::QuadLayerRemQuads(ActQuadLayerRemQuads { base: act.base }),
            map,
        ),
        EditorAction::SoundLayerAddSounds(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::SoundLayerRemSounds(ActSoundLayerRemSounds { base: act.base }),
            map,
        ),
        EditorAction::QuadLayerRemQuads(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::QuadLayerAddQuads(ActQuadLayerAddQuads { base: act.base }),
            map,
        ),
        EditorAction::SoundLayerRemSounds(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::SoundLayerAddSounds(ActSoundLayerAddSounds { base: act.base }),
            map,
        ),
        EditorAction::AddTileLayer(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::RemTileLayer(ActRemTileLayer { base: act.base }),
            map,
        ),
        EditorAction::AddQuadLayer(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::RemQuadLayer(ActRemQuadLayer { base: act.base }),
            map,
        ),
        EditorAction::AddSoundLayer(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::RemSoundLayer(ActRemSoundLayer { base: act.base }),
            map,
        ),
        EditorAction::RemTileLayer(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::AddTileLayer(ActAddTileLayer { base: act.base }),
            map,
        ),
        EditorAction::RemQuadLayer(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::AddQuadLayer(ActAddQuadLayer { base: act.base }),
            map,
        ),
        EditorAction::RemSoundLayer(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::AddSoundLayer(ActAddSoundLayer { base: act.base }),
            map,
        ),
        EditorAction::AddPhysicsTileLayer(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::RemPhysicsTileLayer(ActRemPhysicsTileLayer { base: act.base }),
            map,
        ),
        EditorAction::RemPhysicsTileLayer(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::AddPhysicsTileLayer(ActAddPhysicsTileLayer { base: act.base }),
            map,
        ),
        EditorAction::TileLayerReplaceTiles(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::TileLayerReplaceTiles(ActTileLayerReplaceTiles {
                base: ActTileLayerReplTilesBase {
                    is_background: act.base.is_background,
                    group_index: act.base.group_index,
                    layer_index: act.base.layer_index,
                    new_tiles: act.base.old_tiles,
                    old_tiles: act.base.new_tiles,
                    x: act.base.x,
                    y: act.base.y,
                    w: act.base.w,
                    h: act.base.h,
                },
            }),
            map,
        ),
        EditorAction::TilePhysicsLayerReplaceTiles(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::TilePhysicsLayerReplaceTiles(ActTilePhysicsLayerReplaceTiles {
                base: ActTilePhysicsLayerReplTilesBase {
                    layer_index: act.base.layer_index,
                    new_tiles: act.base.old_tiles,
                    old_tiles: act.base.new_tiles,
                    x: act.base.x,
                    y: act.base.y,
                    w: act.base.w,
                    h: act.base.h,
                },
            }),
            map,
        ),
        EditorAction::AddGroup(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::RemGroup(ActRemGroup { base: act.base }),
            map,
        ),
        EditorAction::RemGroup(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::AddGroup(ActAddGroup { base: act.base }),
            map,
        ),
        EditorAction::ChangeGroupAttr(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::ChangeGroupAttr(ActChangeGroupAttr {
                is_background: act.is_background,
                group_index: act.group_index,
                new_attr: act.old_attr,
                old_attr: act.new_attr,
            }),
            map,
        ),
        EditorAction::ChangePhysicsGroupAttr(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::ChangePhysicsGroupAttr(ActChangePhysicsGroupAttr {
                new_attr: act.old_attr,
                old_attr: act.new_attr,

                new_layer_tiles: act.old_layer_tiles,
                old_layer_tiles: act.new_layer_tiles,
            }),
            map,
        ),
        EditorAction::ChangeTileLayerDesignAttr(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::ChangeTileLayerDesignAttr(ActChangeTileLayerDesignAttr {
                is_background: act.is_background,
                group_index: act.group_index,
                layer_index: act.layer_index,
                new_attr: act.old_attr,
                old_attr: act.new_attr,
                new_tiles: act.old_tiles,
                old_tiles: act.new_tiles,
            }),
            map,
        ),
        EditorAction::ChangeQuadLayerAttr(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::ChangeQuadLayerAttr(ActChangeQuadLayerAttr {
                is_background: act.is_background,
                group_index: act.group_index,
                layer_index: act.layer_index,
                new_attr: act.old_attr,
                old_attr: act.new_attr,
            }),
            map,
        ),
        EditorAction::ChangeSoundLayerAttr(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::ChangeSoundLayerAttr(ActChangeSoundLayerAttr {
                is_background: act.is_background,
                group_index: act.group_index,
                layer_index: act.layer_index,
                new_attr: act.old_attr,
                old_attr: act.new_attr,
            }),
            map,
        ),
        EditorAction::ChangeQuadAttr(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::ChangeQuadAttr(Box::new(ActChangeQuadAttr {
                is_background: act.is_background,
                group_index: act.group_index,
                layer_index: act.layer_index,
                new_attr: act.old_attr,
                old_attr: act.new_attr,
                index: act.index,
            })),
            map,
        ),
        EditorAction::ChangeSoundAttr(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::ChangeSoundAttr(ActChangeSoundAttr {
                is_background: act.is_background,
                group_index: act.group_index,
                layer_index: act.layer_index,
                new_attr: act.old_attr,
                old_attr: act.new_attr,
                index: act.index,
            }),
            map,
        ),
        EditorAction::ChangeTeleporter(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::ChangeTeleporter(ActChangeTeleporter {
                index: act.index,
                new_name: act.old_name,
                old_name: act.new_name,
            }),
            map,
        ),
        EditorAction::ChangeSwitch(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::ChangeSwitch(ActChangeSwitch {
                index: act.index,
                new_name: act.old_name,
                old_name: act.new_name,
            }),
            map,
        ),
        EditorAction::ChangeTuneZone(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::ChangeTuneZone(ActChangeTuneZone {
                index: act.index,
                new_name: act.old_name,
                old_name: act.new_name,
                new_tunes: act.old_tunes,
                old_tunes: act.new_tunes,
            }),
            map,
        ),
        EditorAction::AddPosAnim(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::RemPosAnim(ActRemPosAnim { base: act.base }),
            map,
        ),
        EditorAction::RemPosAnim(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::AddPosAnim(ActAddPosAnim { base: act.base }),
            map,
        ),
        EditorAction::AddColorAnim(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::RemColorAnim(ActRemColorAnim { base: act.base }),
            map,
        ),
        EditorAction::RemColorAnim(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::AddColorAnim(ActAddColorAnim { base: act.base }),
            map,
        ),
        EditorAction::AddSoundAnim(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::RemSoundAnim(ActRemSoundAnim { base: act.base }),
            map,
        ),
        EditorAction::RemSoundAnim(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::AddSoundAnim(ActAddSoundAnim { base: act.base }),
            map,
        ),
    }
}
