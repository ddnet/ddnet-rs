use std::{collections::BTreeMap, rc::Rc, sync::Arc};

use anyhow::anyhow;
use base::hash::generate_hash_for;
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
use hashlink::lru_cache::Entry;
use image_utils::{png::load_png_image_as_rgba, utils::texture_2d_to_3d};
use map::{
    map::groups::{
        layers::{
            design::{
                MapLayer, MapLayerQuad, MapLayerQuadsAttrs, MapLayerSound, MapLayerSoundAttrs,
                MapLayerTile,
            },
            physics::{MapLayerPhysics, MapLayerTilePhysicsTuneZone},
            tiles::{MapTileLayerAttr, MapTileLayerPhysicsTiles},
        },
        MapGroup,
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
        ActAddTileLayer, ActChangeDesignLayerName, ActChangeGroupAttr, ActChangeGroupName,
        ActChangePhysicsGroupAttr, ActChangeQuadAttr, ActChangeQuadLayerAttr, ActChangeSoundAttr,
        ActChangeSoundLayerAttr, ActChangeSwitch, ActChangeTeleporter,
        ActChangeTileLayerDesignAttr, ActChangeTuneZone, ActLayerChangeImageIndex,
        ActLayerChangeSoundIndex, ActMoveGroup, ActMoveLayer, ActQuadLayerAddQuads,
        ActQuadLayerAddRemQuads, ActQuadLayerRemQuads, ActRemColorAnim, ActRemGroup, ActRemImage,
        ActRemImage2dArray, ActRemPhysicsTileLayer, ActRemPosAnim, ActRemQuadLayer, ActRemSound,
        ActRemSoundAnim, ActRemSoundLayer, ActRemTileLayer, ActSetCommands, ActSetMetadata,
        ActSoundLayerAddRemSounds, ActSoundLayerAddSounds, ActSoundLayerRemSounds,
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

fn merge_quad_add_base(
    mut act1: ActQuadLayerAddRemQuads,
    act2: ActQuadLayerAddRemQuads,
) -> anyhow::Result<(ActQuadLayerAddRemQuads, Option<ActQuadLayerAddRemQuads>)> {
    if act1.index <= act2.index && act1.index + act1.quads.len() >= act2.index {
        let index = act2.index - act1.index;
        act1.quads.splice(index..index, act2.quads);
        Ok((act1, None))
    } else {
        Ok((act1, Some(act2)))
    }
}

fn merge_quad_rem_base(
    act1: ActQuadLayerAddRemQuads,
    act2: ActQuadLayerAddRemQuads,
) -> anyhow::Result<(ActQuadLayerAddRemQuads, Option<ActQuadLayerAddRemQuads>)> {
    merge_quad_add_base(act2, act1).map(|(act1, act2)| {
        // if failed, restore order
        match act2 {
            Some(act2) => (act2, Some(act1)),
            None => (act1, None),
        }
    })
}

fn merge_sound_add_base(
    mut act1: ActSoundLayerAddRemSounds,
    act2: ActSoundLayerAddRemSounds,
) -> anyhow::Result<(ActSoundLayerAddRemSounds, Option<ActSoundLayerAddRemSounds>)> {
    if act1.index <= act2.index && act1.index + act1.sounds.len() >= act2.index {
        let index = act2.index - act1.index;
        act1.sounds.splice(index..index, act2.sounds);
        Ok((act1, None))
    } else {
        Ok((act1, Some(act2)))
    }
}

fn merge_sound_rem_base(
    act1: ActSoundLayerAddRemSounds,
    act2: ActSoundLayerAddRemSounds,
) -> anyhow::Result<(ActSoundLayerAddRemSounds, Option<ActSoundLayerAddRemSounds>)> {
    merge_sound_add_base(act2, act1).map(|(act1, act2)| {
        // if failed, restore order
        match act2 {
            Some(act2) => (act2, Some(act1)),
            None => (act1, None),
        }
    })
}

/// returns at least one action
/// if both actions are returned, that means these actions are not mergeable
fn merge_actions_group(
    action1: EditorAction,
    action2: EditorAction,
) -> anyhow::Result<(EditorAction, Option<EditorAction>)> {
    match (action1, action2) {
        (EditorAction::MoveGroup(mut act1), EditorAction::MoveGroup(act2)) => {
            if act1.new_is_background == act2.old_is_background && act1.new_group == act2.old_group
            {
                act1.new_is_background = act2.new_is_background;
                act1.new_group = act2.new_group;

                Ok((EditorAction::MoveGroup(act1), None))
            } else {
                Ok((
                    EditorAction::MoveGroup(act1),
                    Some(EditorAction::MoveGroup(act2)),
                ))
            }
        }
        (EditorAction::MoveLayer(mut act1), EditorAction::MoveLayer(act2)) => {
            if act1.new_is_background == act2.old_is_background
                && act1.new_group == act2.old_group
                && act1.new_layer == act2.old_layer
            {
                act1.new_is_background = act2.new_is_background;
                act1.new_group = act2.new_group;
                act1.new_layer = act2.new_layer;

                Ok((EditorAction::MoveLayer(act1), None))
            } else {
                Ok((
                    EditorAction::MoveLayer(act1),
                    Some(EditorAction::MoveLayer(act2)),
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
                let (act1, act2) = merge_quad_add_base(act1.base, act2.base)?;

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
                let (act1, act2) = merge_sound_add_base(act1.base, act2.base)?;

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
                let (act1, act2) = merge_quad_rem_base(act1.base, act2.base)?;

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
                let (act1, act2) = merge_sound_rem_base(act1.base, act2.base)?;

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
        (EditorAction::ChangeGroupName(mut act1), EditorAction::ChangeGroupName(act2)) => {
            if act1.is_background == act2.is_background && act1.group_index == act2.group_index {
                act1.new_name = act2.new_name;
                Ok((EditorAction::ChangeGroupName(act1), None))
            } else {
                Ok((
                    EditorAction::ChangeGroupName(act1),
                    Some(EditorAction::ChangeGroupName(act2)),
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
        (
            EditorAction::ChangeDesignLayerName(mut act1),
            EditorAction::ChangeDesignLayerName(act2),
        ) => {
            if act1.is_background == act2.is_background
                && act1.group_index == act2.group_index
                && act1.layer_index == act2.layer_index
            {
                act1.new_name = act2.new_name;
                Ok((EditorAction::ChangeDesignLayerName(act1), None))
            } else {
                Ok((
                    EditorAction::ChangeDesignLayerName(act1),
                    Some(EditorAction::ChangeDesignLayerName(act2)),
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
        (EditorAction::SetCommands(mut act1), EditorAction::SetCommands(act2)) => {
            act1.new_commands = act2.new_commands;
            Ok((EditorAction::SetCommands(act1), None))
        }
        (EditorAction::SetMetadata(mut act1), EditorAction::SetMetadata(act2)) => {
            act1.new_meta = act2.new_meta;
            Ok((EditorAction::SetMetadata(act1), None))
        }
        (act1, act2) => Ok((act1, Some(act2))),
    }
}

/// Merge multiple same actions into as few as possible.
///
/// The implementation automatically decides if it thinks
/// that the actions should be merged.
/// If two or more actions are not similar, this function still returns Ok(_),
/// it will simply not merge them.
///
/// Returns `Ok(true)` if an action was merged.
pub fn merge_actions(actions: &mut Vec<EditorAction>) -> anyhow::Result<bool> {
    if actions.is_empty() {
        return Ok(false);
    }

    let mut had_merge = false;
    while actions.len() > 1 {
        let act2 = actions.pop();
        let act1 = actions.pop();

        if let (Some(act1), Some(act2)) = (act1, act2) {
            let (act1, act2) = merge_actions_group(act1, act2)?;
            actions.push(act1);
            if let Some(act2) = act2 {
                actions.push(act2);
                break;
            } else {
                had_merge = true;
            }
        } else {
            unreachable!();
        }
    }

    Ok(had_merge)
}

pub fn check_and_copy_tiles<T: Copy + PartialEq>(
    layer_index: usize,
    dst_tiles: &mut [T],
    check_tiles: &mut [T],
    copy_tiles: &[T],
    w: usize,
    h: usize,
    sub_x: usize,
    sub_y: usize,
    sub_w: usize,
    sub_h: usize,
    fix_check_tiles: bool,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        (sub_x + sub_w) <= w,
        "{} + {} was out of bounds for layer {} with width {}",
        sub_x,
        sub_w,
        layer_index,
        w
    );
    anyhow::ensure!(
        (sub_y + sub_h) <= h,
        "{} + {} was out of bounds for layer {} with height {}",
        sub_y,
        sub_h,
        layer_index,
        h
    );
    anyhow::ensure!(
        sub_h * sub_w == copy_tiles.len(),
        "brush tiles were not equal to the copy w * h in layer {}",
        layer_index,
    );
    anyhow::ensure!(
        sub_h * sub_w == check_tiles.len(),
        "brush old tiles were not equal to the copy w * h in layer {}",
        layer_index,
    );
    if fix_check_tiles {
        // fix tiles if wanted
        dst_tiles
            .chunks(w)
            .skip(sub_y)
            .take(sub_h)
            .enumerate()
            .for_each(|(index, chunk)| {
                let copy_tiles_y_offset = index * sub_w;
                check_tiles[copy_tiles_y_offset..copy_tiles_y_offset + sub_w]
                    .copy_from_slice(&chunk[sub_x..sub_x + sub_w]);
            });
    }

    // check tiles
    let tiles_matches = dst_tiles
        .chunks_mut(w)
        .skip(sub_y)
        .take(sub_h)
        .enumerate()
        .all(|(index, chunk)| {
            let copy_tiles_y_offset = index * sub_w;
            chunk[sub_x..sub_x + sub_w]
                == check_tiles[copy_tiles_y_offset..copy_tiles_y_offset + sub_w]
        });
    anyhow::ensure!(
        tiles_matches,
        "previous tiles in action did not \
            match the current ones in the map."
    );
    // apply tiles
    dst_tiles
        .chunks_mut(w)
        .skip(sub_y)
        .take(sub_h)
        .enumerate()
        .for_each(|(index, chunk)| {
            let copy_tiles_y_offset = index * sub_w;
            chunk[sub_x..sub_x + sub_w]
                .copy_from_slice(&copy_tiles[copy_tiles_y_offset..copy_tiles_y_offset + sub_w]);
        });
    Ok(())
}

/// Validates and executes the action.
///
/// If `fix_action` is true the action will try
/// fix mostly `old_*` parameters like previous
/// layers when layers are deleted.
/// Making it easier for out of sync clients to push
/// actions.
pub fn do_action(
    tp: &Arc<rayon::ThreadPool>,
    sound_mt: &SoundMultiThreaded,
    graphics_mt: &GraphicsMultiThreaded,
    buffer_object_handle: &GraphicsBufferObjectHandle,
    backend_handle: &GraphicsBackendHandle,
    texture_handle: &GraphicsTextureHandle,
    mut action: EditorAction,
    map: &mut EditorMap,
    fix_action: bool,
) -> anyhow::Result<EditorAction> {
    let mut remove_layer =
        |is_background: bool,
         group_index: usize,
         index: usize,
         validate_layer: &mut dyn FnMut(&EditorLayer) -> anyhow::Result<()>| {
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
            validate_layer(&group.layers[index])?;
            group.layers.remove(index);
            anyhow::Ok(())
        };

    match &mut action {
        EditorAction::MoveGroup(act) => {
            let groups = if act.old_is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            anyhow::ensure!(
                groups.get(act.old_group).is_some(),
                "group {} is out of bounds",
                act.old_group
            );
            let group = groups.remove(act.old_group);
            let groups = if act.new_is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            if act.new_group <= groups.len() {
                groups.insert(act.new_group, group);
            } else {
                let groups = if act.old_is_background {
                    &mut map.groups.background
                } else {
                    &mut map.groups.foreground
                };
                // add group back
                groups.insert(act.old_group, group);
                anyhow::bail!("group {} is out of bounds", act.new_group);
            }
        }
        EditorAction::MoveLayer(act) => {
            let groups = if act.old_is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            let group = groups
                .get_mut(act.old_group)
                .ok_or_else(|| anyhow!("group {} is out of bounds", act.old_group))?;
            anyhow::ensure!(
                group.layers.get(act.old_layer).is_some(),
                "layer {} is out of bounds",
                act.old_layer
            );
            let layer = group.layers.remove(act.old_layer);
            let groups = if act.new_is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            if let Some(group) = groups
                .get_mut(act.new_group)
                .and_then(|group| (act.new_layer <= group.layers.len()).then_some(group))
            {
                group.layers.insert(act.new_layer, layer);
            } else {
                let groups = if act.old_is_background {
                    &mut map.groups.background
                } else {
                    &mut map.groups.foreground
                };
                let group = groups
                    .get_mut(act.old_group)
                    .expect("Group must exist at this point. logic bug.");
                // add layer back
                group.layers.insert(act.old_layer, layer);
                anyhow::bail!(
                    "layer {} or group {} is out of bounds",
                    act.new_layer,
                    act.new_group
                );
            }
        }
        EditorAction::AddImage(act) => {
            anyhow::ensure!(
                act.base.index <= map.resources.images.len(),
                "{} is out of bounds for image resources",
                act.base.index
            );
            anyhow::ensure!(
                act.base.res.meta.ty.as_str() == "png",
                "currently only png images are allowed",
            );
            anyhow::ensure!(
                act.base.res.meta.blake3_hash == generate_hash_for(&act.base.file),
                "resource hash did not match file hash",
            );
            anyhow::ensure!(
                act.base.res.hq_meta.is_none(),
                "hq assets are currently not supported",
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
                    def: act.base.res.clone(),
                },
            );
        }
        EditorAction::AddImage2dArray(act) => {
            anyhow::ensure!(
                act.base.index <= map.resources.image_arrays.len(),
                "{} is out of bounds for image 2d array resources",
                act.base.index
            );
            anyhow::ensure!(
                act.base.res.meta.ty.as_str() == "png",
                "currently only png images are allowed",
            );
            anyhow::ensure!(
                act.base.res.meta.blake3_hash == generate_hash_for(&act.base.file),
                "resource hash did not match file hash",
            );
            anyhow::ensure!(
                act.base.res.hq_meta.is_none(),
                "hq assets are currently not supported",
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
                            .load_texture_2d_array_rgba_u8(mem, act.base.res.name.as_str())?,
                        file: Rc::new(act.base.file.clone()),
                        hq: None,
                    },
                    def: act.base.res.clone(),
                },
            );
        }
        EditorAction::AddSound(act) => {
            anyhow::ensure!(
                act.base.index <= map.resources.sounds.len(),
                "{} is out of bounds for sound resources",
                act.base.index
            );
            anyhow::ensure!(
                act.base.res.meta.ty.as_str() == "ogg",
                "currently only ogg sounds are allowed",
            );
            anyhow::ensure!(
                act.base.res.meta.blake3_hash == generate_hash_for(&act.base.file),
                "resource hash did not match file hash",
            );
            anyhow::ensure!(
                act.base.res.hq_meta.is_none(),
                "hq assets are currently not supported",
            );
            map.resources.sounds.insert(
                act.base.index,
                EditorSound {
                    def: act.base.res.clone(),
                    user: EditorResource {
                        user: {
                            let mut mem = sound_mt.mem_alloc(act.base.file.len());
                            mem.as_mut_slice().copy_from_slice(&act.base.file);
                            map.user.sound_scene.sound_object_handle.create(mem)
                        },
                        file: Rc::new(act.base.file.clone()),
                        hq: None,
                    },
                },
            );
        }
        EditorAction::RemImage(ActRemImage {
            base: ActAddRemImage { index, file, res },
        }) => {
            let index = *index;
            anyhow::ensure!(
                index < map.resources.images.len(),
                "{} is out of bounds for image resources",
                index
            );
            anyhow::ensure!(
                *map.resources.images[index].user.file == *file,
                "image that was about to be deleted was \
                not the same file as the one given in the action"
            );
            anyhow::ensure!(
                map.resources.images[index].def == *res,
                "image resource props did not match\
                the props given in the action"
            );
            // ensure that the map is still valid
            let layers_valid = map
                .groups
                .background
                .iter()
                .chain(map.groups.foreground.iter())
                .all(|g| {
                    g.layers.iter().all(|l| match l {
                        EditorLayer::Quad(layer) => layer
                            .layer
                            .attr
                            .image
                            .is_none_or(|i| i < map.resources.images.len().saturating_sub(1)),
                        EditorLayer::Abritrary(_)
                        | EditorLayer::Tile(_)
                        | EditorLayer::Sound(_) => true,
                    })
                });
            anyhow::ensure!(
                layers_valid,
                "deleting image would invalidate some quad layers."
            );
            map.resources.images.remove(index);
        }
        EditorAction::RemImage2dArray(ActRemImage2dArray {
            base: ActAddRemImage { index, file, res },
        }) => {
            let index = *index;
            anyhow::ensure!(
                index < map.resources.image_arrays.len(),
                "{} is out of bounds for image 2d array resources",
                index
            );
            anyhow::ensure!(
                *map.resources.image_arrays[index].user.file == *file,
                "image array that was about to be deleted was \
                not the same file as the one given in the action"
            );
            anyhow::ensure!(
                map.resources.image_arrays[index].def == *res,
                "image array resource props did not match\
                the props given in the action"
            );
            // ensure that the map is still valid
            let layers_valid = map
                .groups
                .background
                .iter()
                .chain(map.groups.foreground.iter())
                .all(|g| {
                    g.layers.iter().all(|l| match l {
                        EditorLayer::Tile(layer) => {
                            layer.layer.attr.image_array.is_none_or(|i| {
                                i < map.resources.image_arrays.len().saturating_sub(1)
                            })
                        }
                        EditorLayer::Abritrary(_)
                        | EditorLayer::Quad(_)
                        | EditorLayer::Sound(_) => true,
                    })
                });
            anyhow::ensure!(
                layers_valid,
                "deleting image 2d array would invalidate some tile layers."
            );
            map.resources.image_arrays.remove(index);
        }
        EditorAction::RemSound(ActRemSound {
            base: ActAddRemSound { index, file, res },
        }) => {
            let index = *index;
            anyhow::ensure!(
                index < map.resources.sounds.len(),
                "{} is out of bounds for sound resources",
                index
            );
            anyhow::ensure!(
                *map.resources.sounds[index].user.file == *file,
                "sound that was about to be deleted was \
                not the same file as the one given in the action"
            );
            anyhow::ensure!(
                map.resources.sounds[index].def == *res,
                "sound resource props did not match\
                the props given in the action"
            );
            // ensure that the map is still valid
            let layers_valid = map
                .groups
                .background
                .iter()
                .chain(map.groups.foreground.iter())
                .all(|g| {
                    g.layers.iter().all(|l| match l {
                        EditorLayer::Sound(layer) => layer
                            .layer
                            .attr
                            .sound
                            .is_none_or(|i| i < map.resources.sounds.len().saturating_sub(1)),
                        EditorLayer::Abritrary(_) | EditorLayer::Quad(_) | EditorLayer::Tile(_) => {
                            true
                        }
                    })
                });
            anyhow::ensure!(
                layers_valid,
                "deleting sound would invalidate some sound layers."
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
            let images_len = if matches!(map_layer, EditorLayer::Tile(_)) {
                map.resources.image_arrays.len()
            } else {
                map.resources.images.len()
            };
            anyhow::ensure!(
                act.new_index.is_none_or(|i| i < images_len),
                "image index is out of bounds: {:?}, len: {}",
                act.new_index,
                images_len
            );
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
                anyhow::ensure!(
                    act.old_index == *image,
                    "image in action did not match that of the layer"
                );
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
            anyhow::ensure!(
                act.new_index.is_none_or(|i| i < map.resources.sounds.len()),
                "sound index is out of bounds: {:?}, len: {}",
                act.new_index,
                map.resources.sounds.len()
            );
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
                anyhow::ensure!(
                    act.old_index == *sound,
                    "sound in action did not match that of the layer"
                );
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
                    act.base.quads.iter().all(|q| q
                        .color_anim
                        .is_none_or(|i| i < map.animations.color.len())
                        && q.pos_anim.is_none_or(|i| i < map.animations.pos.len())),
                    "color or pos animation of at least one quad is out of bounds."
                );
                anyhow::ensure!(
                    act.base.index <= layer.quads.len(),
                    "quad index {} out of bounds",
                    act.base.index
                );
                layer
                    .quads
                    .splice(act.base.index..act.base.index, act.base.quads.clone());
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
                    act.base.sounds.iter().all(|s| s
                        .sound_anim
                        .is_none_or(|i| i < map.animations.sound.len())
                        && s.pos_anim.is_none_or(|i| i < map.animations.pos.len())),
                    "sound or pos animation of at least one sound is out of bounds."
                );
                anyhow::ensure!(
                    act.base.index <= sounds.len(),
                    "sound index {} out of bounds",
                    act.base.index
                );
                sounds.splice(act.base.index..act.base.index, act.base.sounds.clone());
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
                let quads_range = act.base.index..act.base.index + act.base.quads.len();
                if fix_action {
                    act.base.quads = layer.quads[quads_range.clone()].to_vec();
                }
                anyhow::ensure!(
                    layer.quads[quads_range.clone()] == act.base.quads,
                    "quads given in the action did not \
                    match the current quads in the layer"
                );
                layer.quads.splice(quads_range, []);
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
                let sounds_range = act.base.index..act.base.index + act.base.sounds.len();
                if fix_action {
                    act.base.sounds = sounds[sounds_range.clone()].to_vec();
                }
                anyhow::ensure!(
                    sounds[sounds_range.clone()] == act.base.sounds,
                    "sounds given in the action did not \
                    match the current sounds in the layer"
                );
                sounds.splice(sounds_range, []);
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
            anyhow::ensure!(
                act.base
                    .layer
                    .attr
                    .image_array
                    .is_none_or(|i| i < map.resources.image_arrays.len()),
                "given layer was wrong, image array index out of bounds."
            );
            anyhow::ensure!(
                act.base
                    .layer
                    .attr
                    .color_anim
                    .is_none_or(|i| i < map.animations.color.len()),
                "color animation is out of bounds."
            );
            let layer = act.base.layer.clone();
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
                        auto_mapper_rule: Default::default(),
                        auto_mapper_seed: Default::default(),
                        live_edit: None,
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
            anyhow::ensure!(
                act.base
                    .layer
                    .attr
                    .image
                    .is_none_or(|i| i < map.resources.images.len()),
                "given layer was wrong, image index out of bounds."
            );
            anyhow::ensure!(
                act.base.layer.quads.iter().all(|q| q
                    .color_anim
                    .is_none_or(|i| i < map.animations.color.len())
                    && q.pos_anim.is_none_or(|i| i < map.animations.pos.len())),
                "color or pos animation of at least one quad is out of bounds."
            );
            let layer = act.base.layer.clone();
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
            anyhow::ensure!(
                act.base.layer.sounds.iter().all(|s| s
                    .sound_anim
                    .is_none_or(|i| i < map.animations.sound.len())
                    && s.pos_anim.is_none_or(|i| i < map.animations.pos.len())),
                "sound or pos animation of at least one sound is out of bounds."
            );
            group.layers.insert(
                act.base.index,
                EditorLayer::Sound(EditorLayerSound {
                    user: EditorSoundLayerProps {
                        attr: EditorCommonGroupOrLayerAttr::default(),
                        selected: Default::default(),
                        sounds: SoundLayerSounds::default(),
                    },
                    layer: act.base.layer.clone(),
                }),
            );
        }
        EditorAction::RemTileLayer(ActRemTileLayer {
            base:
                ActAddRemTileLayer {
                    is_background,
                    group_index,
                    index,
                    layer,
                },
        }) => {
            remove_layer(*is_background, *group_index, *index, &mut |editor_layer| {
                let EditorLayer::Tile(editor_layer) = editor_layer else {
                    return Err(anyhow!(
                        "Tried to remove a tile layer, \
                        but the layer was no tile layer."
                    ));
                };
                let editor_layer: MapLayerTile = editor_layer.clone().into();
                if fix_action {
                    *layer = editor_layer.clone();
                }
                anyhow::ensure!(
                    editor_layer == *layer,
                    "layer in action did not match the one in the map."
                );

                Ok(())
            })?;
        }
        EditorAction::RemQuadLayer(ActRemQuadLayer {
            base:
                ActAddRemQuadLayer {
                    is_background,
                    group_index,
                    index,
                    layer,
                },
        }) => {
            remove_layer(*is_background, *group_index, *index, &mut |editor_layer| {
                let EditorLayer::Quad(editor_layer) = editor_layer else {
                    return Err(anyhow!(
                        "Tried to remove a quad layer, \
                        but the layer was no quad layer."
                    ));
                };
                let editor_layer: MapLayerQuad = editor_layer.clone().into();
                if fix_action {
                    *layer = editor_layer.clone();
                }
                anyhow::ensure!(
                    editor_layer == *layer,
                    "layer in action did not match the one in the map."
                );

                Ok(())
            })?;
        }
        EditorAction::RemSoundLayer(ActRemSoundLayer {
            base:
                ActAddRemSoundLayer {
                    is_background,
                    group_index,
                    index,
                    layer,
                },
        }) => {
            remove_layer(*is_background, *group_index, *index, &mut |editor_layer| {
                let EditorLayer::Sound(editor_layer) = editor_layer else {
                    return Err(anyhow!(
                        "Tried to remove a sound layer, \
                        but the layer was no sound layer."
                    ));
                };
                let editor_layer: MapLayerSound = editor_layer.clone().into();
                if fix_action {
                    *layer = editor_layer.clone();
                }
                anyhow::ensure!(
                    editor_layer == *layer,
                    "layer in action did not match the one in the map."
                );

                Ok(())
            })?;
        }
        EditorAction::AddPhysicsTileLayer(act) => {
            let physics = &mut map.groups.physics;
            anyhow::ensure!(
                act.base.index <= physics.layers.len(),
                "layer index {} is out of bounds in physics group",
                act.base.index,
            );
            let tiles_count =
                physics.attr.width.get() as usize * physics.attr.height.get() as usize;
            let layer = act.base.layer.clone();
            let layer_of_ty_exists = match &layer {
                MapLayerPhysics::Arbitrary(_) => physics
                    .layers
                    .iter()
                    .any(|l| matches!(l, EditorPhysicsLayer::Arbitrary(_))),
                MapLayerPhysics::Game(layer) => {
                    physics
                        .layers
                        .iter()
                        .any(|l| matches!(l, EditorPhysicsLayer::Game(_)))
                        || layer.tiles.len() != tiles_count
                }
                MapLayerPhysics::Front(layer) => {
                    physics
                        .layers
                        .iter()
                        .any(|l| matches!(l, EditorPhysicsLayer::Front(_)))
                        || layer.tiles.len() != tiles_count
                }
                MapLayerPhysics::Tele(layer) => {
                    physics
                        .layers
                        .iter()
                        .any(|l| matches!(l, EditorPhysicsLayer::Tele(_)))
                        || layer.base.tiles.len() != tiles_count
                }
                MapLayerPhysics::Speedup(layer) => {
                    physics
                        .layers
                        .iter()
                        .any(|l| matches!(l, EditorPhysicsLayer::Speedup(_)))
                        || layer.tiles.len() != tiles_count
                }
                MapLayerPhysics::Switch(layer) => {
                    physics
                        .layers
                        .iter()
                        .any(|l| matches!(l, EditorPhysicsLayer::Switch(_)))
                        || layer.base.tiles.len() != tiles_count
                }
                MapLayerPhysics::Tune(layer) => {
                    physics
                        .layers
                        .iter()
                        .any(|l| matches!(l, EditorPhysicsLayer::Tune(_)))
                        || layer.base.tiles.len() != tiles_count
                }
            };
            anyhow::ensure!(
                !layer_of_ty_exists,
                "Tile count was wrong or a layer of the given type already \
                exists in the physics group."
            );
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
                                number_extra_text: Default::default(),
                                context_menu_open: false,
                                switch_delay: Default::default(),
                                speedup_force: Default::default(),
                                speedup_angle: Default::default(),
                                speedup_max_speed: Default::default(),
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
                                number_extra_text: Default::default(),
                                context_menu_open: false,
                                switch_delay: Default::default(),
                                speedup_force: Default::default(),
                                speedup_angle: Default::default(),
                                speedup_max_speed: Default::default(),
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
                                number_extra_text: Default::default(),
                                context_menu_open: false,
                                switch_delay: Default::default(),
                                speedup_force: Default::default(),
                                speedup_angle: Default::default(),
                                speedup_max_speed: Default::default(),
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
                                number_extra_text: Default::default(),
                                context_menu_open: false,
                                switch_delay: Default::default(),
                                speedup_force: Default::default(),
                                speedup_angle: Default::default(),
                                speedup_max_speed: Default::default(),
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
                                number_extra_text: Default::default(),
                                context_menu_open: false,
                                switch_delay: Default::default(),
                                speedup_force: Default::default(),
                                speedup_angle: Default::default(),
                                speedup_max_speed: Default::default(),
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
                                number_extra_text: Default::default(),
                                context_menu_open: false,
                                switch_delay: Default::default(),
                                speedup_force: Default::default(),
                                speedup_angle: Default::default(),
                                speedup_max_speed: Default::default(),
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
            let layer: MapLayerPhysics = physics.layers[index].clone().into();
            if fix_action {
                act.base.layer = layer.clone();
            }
            anyhow::ensure!(
                !matches!(layer, MapLayerPhysics::Game(_)),
                "deleting the game/main physics layer is forbidden."
            );
            anyhow::ensure!(
                layer == act.base.layer,
                "physics layer in the action does not \
                match the physics layer in the map"
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
                let check_tiles = &mut act.base.old_tiles;
                check_and_copy_tiles(
                    act.base.layer_index,
                    &mut layer.layer.tiles,
                    check_tiles,
                    copy_tiles,
                    layer.layer.attr.width.get() as usize,
                    layer.layer.attr.height.get() as usize,
                    act.base.x as usize,
                    act.base.y as usize,
                    act.base.w.get() as usize,
                    act.base.h.get() as usize,
                    fix_action,
                )?;
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

            match layer {
                MapLayerPhysicsSkeleton::Arbitrary(_) => {
                    return Err(anyhow!("arbitrary tiles are not supported by this editor."));
                }
                MapLayerPhysicsSkeleton::Game(layer) => {
                    let MapTileLayerPhysicsTiles::Game(copy_tiles) = &act.base.new_tiles else {
                        return Err(anyhow!("tiles are not compatible"));
                    };
                    let MapTileLayerPhysicsTiles::Game(check_tiles) = &mut act.base.old_tiles
                    else {
                        return Err(anyhow!("tiles are not compatible"));
                    };
                    check_and_copy_tiles(
                        act.base.layer_index,
                        &mut layer.layer.tiles,
                        check_tiles,
                        copy_tiles,
                        group.attr.width.get() as usize,
                        group.attr.height.get() as usize,
                        act.base.x as usize,
                        act.base.y as usize,
                        act.base.w.get() as usize,
                        act.base.h.get() as usize,
                        fix_action,
                    )?;
                }
                MapLayerPhysicsSkeleton::Front(layer) => {
                    let MapTileLayerPhysicsTiles::Front(copy_tiles) = &act.base.new_tiles else {
                        return Err(anyhow!("tiles are not compatible"));
                    };
                    let MapTileLayerPhysicsTiles::Front(check_tiles) = &mut act.base.old_tiles
                    else {
                        return Err(anyhow!("tiles are not compatible"));
                    };
                    check_and_copy_tiles(
                        act.base.layer_index,
                        &mut layer.layer.tiles,
                        check_tiles,
                        copy_tiles,
                        group.attr.width.get() as usize,
                        group.attr.height.get() as usize,
                        act.base.x as usize,
                        act.base.y as usize,
                        act.base.w.get() as usize,
                        act.base.h.get() as usize,
                        fix_action,
                    )?;
                }
                MapLayerPhysicsSkeleton::Tele(layer) => {
                    let MapTileLayerPhysicsTiles::Tele(copy_tiles) = &act.base.new_tiles else {
                        return Err(anyhow!("tiles are not compatible"));
                    };
                    let MapTileLayerPhysicsTiles::Tele(check_tiles) = &mut act.base.old_tiles
                    else {
                        return Err(anyhow!("tiles are not compatible"));
                    };
                    check_and_copy_tiles(
                        act.base.layer_index,
                        &mut layer.layer.base.tiles,
                        check_tiles,
                        copy_tiles,
                        group.attr.width.get() as usize,
                        group.attr.height.get() as usize,
                        act.base.x as usize,
                        act.base.y as usize,
                        act.base.w.get() as usize,
                        act.base.h.get() as usize,
                        fix_action,
                    )?;
                }
                MapLayerPhysicsSkeleton::Speedup(layer) => {
                    let MapTileLayerPhysicsTiles::Speedup(copy_tiles) = &act.base.new_tiles else {
                        return Err(anyhow!("tiles are not compatible"));
                    };
                    let MapTileLayerPhysicsTiles::Speedup(check_tiles) = &mut act.base.old_tiles
                    else {
                        return Err(anyhow!("tiles are not compatible"));
                    };
                    check_and_copy_tiles(
                        act.base.layer_index,
                        &mut layer.layer.tiles,
                        check_tiles,
                        copy_tiles,
                        group.attr.width.get() as usize,
                        group.attr.height.get() as usize,
                        act.base.x as usize,
                        act.base.y as usize,
                        act.base.w.get() as usize,
                        act.base.h.get() as usize,
                        fix_action,
                    )?;
                }
                MapLayerPhysicsSkeleton::Switch(layer) => {
                    let MapTileLayerPhysicsTiles::Switch(copy_tiles) = &act.base.new_tiles else {
                        return Err(anyhow!("tiles are not compatible"));
                    };
                    let MapTileLayerPhysicsTiles::Switch(check_tiles) = &mut act.base.old_tiles
                    else {
                        return Err(anyhow!("tiles are not compatible"));
                    };
                    check_and_copy_tiles(
                        act.base.layer_index,
                        &mut layer.layer.base.tiles,
                        check_tiles,
                        copy_tiles,
                        group.attr.width.get() as usize,
                        group.attr.height.get() as usize,
                        act.base.x as usize,
                        act.base.y as usize,
                        act.base.w.get() as usize,
                        act.base.h.get() as usize,
                        fix_action,
                    )?;
                }
                MapLayerPhysicsSkeleton::Tune(layer) => {
                    let MapTileLayerPhysicsTiles::Tune(copy_tiles) = &act.base.new_tiles else {
                        return Err(anyhow!("tiles are not compatible"));
                    };
                    let MapTileLayerPhysicsTiles::Tune(check_tiles) = &mut act.base.old_tiles
                    else {
                        return Err(anyhow!("tiles are not compatible"));
                    };
                    check_and_copy_tiles(
                        act.base.layer_index,
                        &mut layer.layer.base.tiles,
                        check_tiles,
                        copy_tiles,
                        group.attr.width.get() as usize,
                        group.attr.height.get() as usize,
                        act.base.x as usize,
                        act.base.y as usize,
                        act.base.w.get() as usize,
                        act.base.h.get() as usize,
                        fix_action,
                    )?;
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
                        .clone()
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
                                        auto_mapper_rule: Default::default(),
                                        auto_mapper_seed: Default::default(),
                                        live_edit: None,
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
                    name: act.base.group.name.clone(),
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
            let group: MapGroup = groups[act.base.index].clone().into();
            if fix_action {
                act.base.group = group.clone();
            }
            anyhow::ensure!(
                group == act.base.group,
                "group in action did not match the one in the map."
            );
            groups.remove(act.base.index);
        }
        EditorAction::ChangeGroupAttr(act) => {
            let groups = if act.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            let group = groups
                .get_mut(act.group_index)
                .ok_or(anyhow!("group {} is out of bounds", act.group_index))?;
            if fix_action {
                act.old_attr = group.attr;
            }
            anyhow::ensure!(
                group.attr == act.old_attr,
                "group attr in action did not match the one in the map"
            );
            group.attr = act.new_attr;
        }
        EditorAction::ChangeGroupName(act) => {
            let groups = if act.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            let group = groups
                .get_mut(act.group_index)
                .ok_or(anyhow!("group {} is out of bounds", act.group_index))?;

            if fix_action {
                act.old_name = group.name.clone();
            }
            anyhow::ensure!(
                group.name == act.old_name,
                "group name in action did not match the one in the map"
            );

            let name = &mut group.name;
            *name = act.new_name.clone();
        }
        EditorAction::ChangePhysicsGroupAttr(act) => {
            let group = &mut map.groups.physics;
            let width_or_height_change =
                group.attr.width != act.new_attr.width || group.attr.height != act.new_attr.height;

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
                        let MapTileLayerPhysicsTiles::Game(tiles) = new_tiles else {
                            anyhow::bail!("game layer expects game tiles");
                        };
                        anyhow::ensure!(
                            tiles.len()
                                == act.new_attr.width.get() as usize
                                    * act.new_attr.height.get() as usize,
                            "game tile layer new tiles len did not match new width * height"
                        );
                    }
                    MapLayerPhysicsSkeleton::Front(_) => {
                        let MapTileLayerPhysicsTiles::Front(tiles) = new_tiles else {
                            anyhow::bail!("front layer expects front tiles");
                        };
                        anyhow::ensure!(
                            tiles.len()
                                == act.new_attr.width.get() as usize
                                    * act.new_attr.height.get() as usize,
                            "front tile layer new tiles len did not match new width * height"
                        );
                    }
                    MapLayerPhysicsSkeleton::Tele(_) => {
                        anyhow::ensure!(matches!(new_tiles, MapTileLayerPhysicsTiles::Tele(_)));

                        let MapTileLayerPhysicsTiles::Tele(tiles) = new_tiles else {
                            anyhow::bail!("tele layer expects tele tiles");
                        };
                        anyhow::ensure!(
                            tiles.len()
                                == act.new_attr.width.get() as usize
                                    * act.new_attr.height.get() as usize,
                            "tele tile layer new tiles len did not match new width * height"
                        );
                    }
                    MapLayerPhysicsSkeleton::Speedup(_) => {
                        anyhow::ensure!(matches!(new_tiles, MapTileLayerPhysicsTiles::Speedup(_)));

                        let MapTileLayerPhysicsTiles::Speedup(tiles) = new_tiles else {
                            anyhow::bail!("speedup layer expects speedup tiles");
                        };
                        anyhow::ensure!(
                            tiles.len()
                                == act.new_attr.width.get() as usize
                                    * act.new_attr.height.get() as usize,
                            "speedup tile layer new tiles len did not match new width * height"
                        );
                    }
                    MapLayerPhysicsSkeleton::Switch(_) => {
                        anyhow::ensure!(matches!(new_tiles, MapTileLayerPhysicsTiles::Switch(_)));

                        let MapTileLayerPhysicsTiles::Switch(tiles) = new_tiles else {
                            anyhow::bail!("switch layer expects switch tiles");
                        };
                        anyhow::ensure!(
                            tiles.len()
                                == act.new_attr.width.get() as usize
                                    * act.new_attr.height.get() as usize,
                            "switch tile layer new tiles len did not match new width * height"
                        );
                    }
                    MapLayerPhysicsSkeleton::Tune(_) => {
                        let MapTileLayerPhysicsTiles::Tune(tiles) = new_tiles else {
                            anyhow::bail!("tune layer expects tune tiles");
                        };
                        anyhow::ensure!(
                            tiles.len()
                                == act.new_attr.width.get() as usize
                                    * act.new_attr.height.get() as usize,
                            "tune tile layer new tiles len did not match new width * height"
                        );
                    }
                }
            }

            let layers: Vec<MapTileLayerPhysicsTiles> = group
                .layers
                .iter()
                .map(|l| match l {
                    MapLayerPhysicsSkeleton::Arbitrary(layer) => {
                        MapTileLayerPhysicsTiles::Arbitrary(layer.buf.clone())
                    }
                    MapLayerPhysicsSkeleton::Game(layer) => {
                        MapTileLayerPhysicsTiles::Game(layer.layer.tiles.clone())
                    }
                    MapLayerPhysicsSkeleton::Front(layer) => {
                        MapTileLayerPhysicsTiles::Front(layer.layer.tiles.clone())
                    }
                    MapLayerPhysicsSkeleton::Tele(layer) => {
                        MapTileLayerPhysicsTiles::Tele(layer.layer.base.tiles.clone())
                    }
                    MapLayerPhysicsSkeleton::Speedup(layer) => {
                        MapTileLayerPhysicsTiles::Speedup(layer.layer.tiles.clone())
                    }
                    MapLayerPhysicsSkeleton::Switch(layer) => {
                        MapTileLayerPhysicsTiles::Switch(layer.layer.base.tiles.clone())
                    }
                    MapLayerPhysicsSkeleton::Tune(layer) => {
                        MapTileLayerPhysicsTiles::Tune(layer.layer.base.tiles.clone())
                    }
                })
                .collect();
            if fix_action {
                act.old_attr = group.attr;
                act.old_layer_tiles = layers.clone();
            }
            anyhow::ensure!(
                group.attr == act.old_attr,
                "phy group attr in action did not match the one in the map"
            );
            anyhow::ensure!(
                layers == act.old_layer_tiles,
                "physics layers in action did not match the one in the map"
            );

            group.attr = act.new_attr;
            if width_or_height_change {
                let width = group.attr.width;
                let height = group.attr.height;
                let new_tiles = act.new_layer_tiles.clone();
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
            anyhow::ensure!(
                act.new_attr
                    .image_array
                    .is_none_or(|i| i < map.resources.image_arrays.len()),
                "image array is out of bounds."
            );
            anyhow::ensure!(
                act.new_attr
                    .color_anim
                    .is_none_or(|i| i < map.animations.color.len()),
                "color animation is out of bounds."
            );
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
                anyhow::ensure!(
                    act.new_tiles.len()
                        == act.new_attr.width.get() as usize * act.new_attr.height.get() as usize,
                    "design tile layer new tiles len did not match new width * height"
                );
                let width_or_height_change = layer.layer.attr.width != act.new_attr.width
                    || layer.layer.attr.height != act.new_attr.height;

                if fix_action {
                    act.old_attr = layer.layer.attr;
                    act.old_tiles = layer.layer.tiles.clone();
                    if !width_or_height_change {
                        act.new_tiles = layer.layer.tiles.clone();
                    }
                }
                anyhow::ensure!(
                    layer.layer.attr == act.old_attr,
                    "design tile layer attr in action did not match the one in the map"
                );
                anyhow::ensure!(
                    layer.layer.tiles == act.old_tiles,
                    "tiles in action did not match the one in the map"
                );

                let has_tex_change = (layer.layer.attr.image_array.is_some()
                    && act.new_attr.image_array.is_none())
                    || (layer.layer.attr.image_array.is_none()
                        && act.new_attr.image_array.is_some());
                let needs_visual_recreate = width_or_height_change || has_tex_change;
                layer.layer.attr = act.new_attr;
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
            anyhow::ensure!(
                act.new_attr
                    .image
                    .is_none_or(|i| i < map.resources.images.len()),
                "image is out of bounds."
            );
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
                if fix_action {
                    act.old_attr = layer.layer.attr;
                }
                anyhow::ensure!(
                    layer.layer.attr == act.old_attr,
                    "quad layer attr in action did not match the one in the map"
                );

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
            anyhow::ensure!(
                act.new_attr
                    .sound
                    .is_none_or(|i| i < map.resources.sounds.len()),
                "sound is out of bounds."
            );
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
                if fix_action {
                    act.old_attr = layer.layer.attr;
                }
                anyhow::ensure!(
                    layer.layer.attr == act.old_attr,
                    "sound layer attr in action did not match the one in the map"
                );
                layer.layer.attr = act.new_attr;
            } else {
                return Err(anyhow!("not a sound layer"));
            }
        }
        EditorAction::ChangeDesignLayerName(act) => {
            let groups = if act.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            let layer = groups
                .get_mut(act.group_index)
                .ok_or(anyhow!("group {} is out of bounds", act.group_index))?
                .layers
                .get_mut(act.layer_index)
                .ok_or(anyhow!(
                    "layer {} is out of bounds in group {}",
                    act.layer_index,
                    act.group_index
                ))?;

            let name = match layer {
                MapLayerSkeleton::Abritrary(_) => anyhow::bail!("Renaming unsupported layer."),
                MapLayerSkeleton::Tile(layer) => &mut layer.layer.name,
                MapLayerSkeleton::Quad(layer) => &mut layer.layer.name,
                MapLayerSkeleton::Sound(layer) => &mut layer.layer.name,
            };
            if fix_action {
                act.old_name = name.clone();
            }
            anyhow::ensure!(
                *name == act.old_name,
                "layer name in action did not match the one in the map"
            );
            *name = act.new_name.clone();
        }
        EditorAction::ChangeQuadAttr(act) => {
            let groups = if act.is_background {
                &mut map.groups.background
            } else {
                &mut map.groups.foreground
            };
            anyhow::ensure!(
                act.new_attr
                    .pos_anim
                    .is_none_or(|i| i < map.animations.pos.len()),
                "pos anim is out of bounds"
            );
            anyhow::ensure!(
                act.new_attr
                    .color_anim
                    .is_none_or(|i| i < map.animations.color.len()),
                "color anim is out of bounds"
            );
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
                let quad = layer
                    .layer
                    .quads
                    .get_mut(act.index)
                    .ok_or(anyhow!("quad index {} is out of bounds", act.index))?;
                if fix_action {
                    act.old_attr = *quad;
                }
                anyhow::ensure!(
                    *quad == act.old_attr,
                    "quad attr in action did not match the one in the map"
                );
                *quad = act.new_attr;
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
            anyhow::ensure!(
                act.new_attr
                    .pos_anim
                    .is_none_or(|i| i < map.animations.pos.len()),
                "pos anim is out of bounds"
            );
            anyhow::ensure!(
                act.new_attr
                    .sound_anim
                    .is_none_or(|i| i < map.animations.sound.len()),
                "sound anim is out of bounds"
            );
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
                let sound = layer
                    .layer
                    .sounds
                    .get_mut(act.index)
                    .ok_or(anyhow!("sound index {} is out of bounds", act.index))?;
                if fix_action {
                    act.old_attr = *sound;
                }
                anyhow::ensure!(
                    *sound == act.old_attr,
                    "sound attr in action did not match the one in the map"
                );
                *sound = act.new_attr;
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
            match layer.layer.tele_names.entry(act.index) {
                Entry::Occupied(mut a) => {
                    if fix_action {
                        act.old_name = a.get().clone();
                    }
                    anyhow::ensure!(
                        *a.get() == act.old_name,
                        "name in the action did not match the name in the map."
                    );
                    if act.new_name.is_empty() {
                        a.remove();
                    } else {
                        *a.get_mut() = act.new_name.clone();
                    }
                }
                Entry::Vacant(a) => {
                    if fix_action {
                        act.old_name = String::new();
                    }
                    anyhow::ensure!(
                        act.old_name.is_empty(),
                        "name was not empty, even tho the map did not have a name before."
                    );
                    anyhow::ensure!(!act.new_name.is_empty(), "name was empty.");
                    a.insert(act.new_name.clone());
                }
            }
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
            match layer.layer.switch_names.entry(act.index) {
                Entry::Occupied(mut a) => {
                    if fix_action {
                        act.old_name = a.get().clone();
                    }
                    anyhow::ensure!(
                        *a.get() == act.old_name,
                        "name in the action did not match the name in the map."
                    );
                    if act.new_name.is_empty() {
                        a.remove();
                    } else {
                        *a.get_mut() = act.new_name.clone();
                    }
                }
                Entry::Vacant(a) => {
                    if fix_action {
                        act.old_name = String::new();
                    }
                    anyhow::ensure!(
                        act.old_name.is_empty(),
                        "name was not empty, even tho the map did not have a name before."
                    );
                    anyhow::ensure!(!act.new_name.is_empty(), "name was empty.");
                    a.insert(act.new_name.clone());
                }
            }
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

            match layer.layer.tune_zones.entry(act.index) {
                Entry::Occupied(mut a) => {
                    if fix_action {
                        act.old_name = a.get().name.clone();
                        act.old_tunes = a.get().tunes.clone();
                    }
                    anyhow::ensure!(
                        a.get().name == act.old_name,
                        "name in the action did not match the name in the map."
                    );
                    anyhow::ensure!(
                        a.get().tunes == act.old_tunes,
                        "tunes in the action did not match the tunes in the map."
                    );
                    if act.new_tunes.is_empty() && act.new_name.is_empty() {
                        a.remove();
                    } else {
                        a.get_mut().name = act.new_name.clone();
                        a.get_mut().tunes = act.new_tunes.clone();
                    }
                }
                Entry::Vacant(a) => {
                    if fix_action {
                        act.old_name = String::new();
                        act.old_tunes = Default::default();
                    }
                    anyhow::ensure!(
                        act.old_name.is_empty(),
                        "name was not empty, even tho the map did not have a name before."
                    );
                    anyhow::ensure!(
                        act.old_tunes.is_empty(),
                        "tunes were not empty, even tho the map did not have these tunes before."
                    );
                    anyhow::ensure!(
                        !act.new_name.is_empty() || !act.new_tunes.is_empty(),
                        "name and tunes were empty."
                    );
                    a.insert(MapLayerTilePhysicsTuneZone {
                        name: act.new_name.clone(),
                        tunes: act.new_tunes.clone(),
                    });
                }
            }
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
                    def: act.base.anim.clone(),
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
            if fix_action {
                act.base.anim = map.animations.pos[act.base.index].def.clone();
            }
            // make sure no potential layer is still using the invalid index
            let expected_len = map.animations.pos.len().saturating_sub(1);
            let not_in_use = map
                .groups
                .background
                .iter()
                .chain(map.groups.foreground.iter())
                .all(|g| {
                    g.layers.iter().all(|l| {
                        if let EditorLayer::Quad(layer) = l {
                            layer
                                .layer
                                .quads
                                .iter()
                                .all(|q| q.pos_anim.is_none_or(|i| i < expected_len))
                        } else if let EditorLayer::Sound(layer) = l {
                            layer
                                .layer
                                .sounds
                                .iter()
                                .all(|s| s.pos_anim.is_none_or(|i| i < expected_len))
                        } else {
                            true
                        }
                    })
                });
            anyhow::ensure!(not_in_use, "pos animation is still in use");
            anyhow::ensure!(
                map.animations.pos[act.base.index].def == act.base.anim,
                "anim in action was not equal to the anim in the map."
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
                    def: act.base.anim.clone(),
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
            if fix_action {
                act.base.anim = map.animations.color[act.base.index].def.clone();
            }
            // make sure no potential layer is still using the invalid index
            let expected_len = map.animations.color.len().saturating_sub(1);
            let not_in_use = map
                .groups
                .background
                .iter()
                .chain(map.groups.foreground.iter())
                .all(|g| {
                    g.layers.iter().all(|l| {
                        if let EditorLayer::Quad(layer) = l {
                            layer
                                .layer
                                .quads
                                .iter()
                                .all(|q| q.color_anim.is_none_or(|i| i < expected_len))
                        } else if let EditorLayer::Tile(layer) = l {
                            layer.layer.attr.color_anim.is_none_or(|i| i < expected_len)
                        } else {
                            true
                        }
                    })
                });
            anyhow::ensure!(not_in_use, "color animation is still in use");
            anyhow::ensure!(
                map.animations.color[act.base.index].def == act.base.anim,
                "anim in action was not equal to the anim in the map."
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
                    def: act.base.anim.clone(),
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
            if fix_action {
                act.base.anim = map.animations.sound[act.base.index].def.clone();
            }
            // make sure no potential layer is still using the invalid index
            let expected_len = map.animations.sound.len().saturating_sub(1);
            let not_in_use = map
                .groups
                .background
                .iter()
                .chain(map.groups.foreground.iter())
                .all(|g| {
                    g.layers.iter().all(|l| {
                        if let EditorLayer::Sound(layer) = l {
                            layer
                                .layer
                                .sounds
                                .iter()
                                .all(|s| s.sound_anim.is_none_or(|i| i < expected_len))
                        } else {
                            true
                        }
                    })
                });
            anyhow::ensure!(not_in_use, "sound animation is still in use");
            anyhow::ensure!(
                map.animations.sound[act.base.index].def == act.base.anim,
                "anim in action was not equal to the anim in the map."
            );
            map.animations.sound.remove(act.base.index);
        }
        EditorAction::SetCommands(act) => {
            if fix_action {
                act.old_commands = map.config.def.commands.clone();
            }
            let old_cmds: BTreeMap<_, _> = act.old_commands.clone().into_iter().collect();
            let cur_cmds: BTreeMap<_, _> = map.config.def.commands.clone().into_iter().collect();
            anyhow::ensure!(
                old_cmds == cur_cmds,
                "commands in action did not match the ones in map."
            );
            map.config.def.commands = act.new_commands.clone();
        }
        EditorAction::SetMetadata(act) => {
            if fix_action {
                act.old_meta = map.meta.def.clone();
            }
            anyhow::ensure!(
                act.old_meta == map.meta.def,
                "metadata in action did not match the ones in map."
            );
            map.meta.def = act.new_meta.clone();
        }
    }
    Ok(action)
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
        EditorAction::MoveGroup(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::MoveGroup(ActMoveGroup {
                old_is_background: act.new_is_background,
                old_group: act.new_group,
                new_is_background: act.old_is_background,
                new_group: act.old_group,
            }),
            map,
            false,
        ),
        EditorAction::MoveLayer(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::MoveLayer(ActMoveLayer {
                old_is_background: act.new_is_background,
                old_group: act.new_group,
                old_layer: act.new_layer,
                new_is_background: act.old_is_background,
                new_group: act.old_group,
                new_layer: act.old_layer,
            }),
            map,
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
        ),
        EditorAction::ChangeGroupName(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::ChangeGroupName(ActChangeGroupName {
                is_background: act.is_background,
                group_index: act.group_index,
                new_name: act.old_name,
                old_name: act.new_name,
            }),
            map,
            false,
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
            false,
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
            false,
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
            false,
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
            false,
        ),
        EditorAction::ChangeDesignLayerName(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::ChangeDesignLayerName(ActChangeDesignLayerName {
                is_background: act.is_background,
                group_index: act.group_index,
                layer_index: act.layer_index,
                new_name: act.old_name,
                old_name: act.new_name,
            }),
            map,
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
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
            false,
        ),
        EditorAction::SetCommands(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::SetCommands(ActSetCommands {
                old_commands: act.new_commands,
                new_commands: act.old_commands,
            }),
            map,
            false,
        ),
        EditorAction::SetMetadata(act) => do_action(
            tp,
            sound_mt,
            graphics_mt,
            buffer_object_handle,
            backend_handle,
            texture_handle,
            EditorAction::SetMetadata(ActSetMetadata {
                old_meta: act.new_meta,
                new_meta: act.old_meta,
            }),
            map,
            false,
        ),
    }
    .map(|_| ())
}

pub fn redo_action(
    tp: &Arc<rayon::ThreadPool>,
    sound_mt: &SoundMultiThreaded,
    graphics_mt: &GraphicsMultiThreaded,
    buffer_object_handle: &GraphicsBufferObjectHandle,
    backend_handle: &GraphicsBackendHandle,
    texture_handle: &GraphicsTextureHandle,
    action: EditorAction,
    map: &mut EditorMap,
) -> anyhow::Result<()> {
    do_action(
        tp,
        sound_mt,
        graphics_mt,
        buffer_object_handle,
        backend_handle,
        texture_handle,
        action,
        map,
        false,
    )
    .map(|_| ())
}
