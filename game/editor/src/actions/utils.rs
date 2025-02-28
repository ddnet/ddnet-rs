use map::map::{
    animations::AnimBase,
    groups::layers::{
        design::{Quad, Sound},
        tiles::MapTileLayerAttr,
    },
};

use crate::map::{
    EditorColorAnimation, EditorGroups, EditorLayer, EditorPosAnimation, EditorSoundAnimation,
};

use super::actions::{
    ActAddRemColorAnim, ActAddRemPosAnim, ActAddRemSoundAnim, ActChangeQuadAttr,
    ActChangeSoundAttr, ActChangeTileLayerDesignAttr, ActRemColorAnim, ActRemPosAnim,
    ActRemSoundAnim, EditorAction,
};

pub fn rem_pos_anim(
    anims: &[EditorPosAnimation],
    groups: &EditorGroups,
    index: usize,
) -> Vec<EditorAction> {
    let anim: AnimBase<_> = anims[index].clone().into();

    let mut actions = vec![];
    for (is_background, group_index, layer_index, layer) in groups
        .background
        .iter()
        .enumerate()
        .map(|g| (true, g))
        .chain(groups.foreground.iter().enumerate().map(|g| (false, g)))
        .flat_map(|(is_background, (g, group))| {
            group
                .layers
                .iter()
                .enumerate()
                .filter_map(move |(l, layer)| match layer {
                    EditorLayer::Abritrary(_) | EditorLayer::Tile(_) => None,
                    EditorLayer::Sound(_) | EditorLayer::Quad(_) => {
                        Some((is_background, g, l, layer))
                    }
                })
        })
    {
        if let EditorLayer::Quad(layer) = layer {
            for (quad_index, q) in layer.layer.quads.iter().enumerate() {
                if q.pos_anim == Some(index) {
                    actions.push(EditorAction::ChangeQuadAttr(Box::new(ActChangeQuadAttr {
                        is_background,
                        group_index,
                        layer_index,
                        index: quad_index,
                        old_attr: *q,
                        new_attr: Quad {
                            pos_anim: None,
                            ..*q
                        },
                    })));
                } else if q.pos_anim.is_some_and(|i| i > index) {
                    actions.push(EditorAction::ChangeQuadAttr(Box::new(ActChangeQuadAttr {
                        is_background,
                        group_index,
                        layer_index,
                        index: quad_index,
                        old_attr: *q,
                        new_attr: Quad {
                            pos_anim: Some(q.pos_anim.unwrap() - 1),
                            ..*q
                        },
                    })));
                }
            }
        } else if let EditorLayer::Sound(layer) = layer {
            for (sound_index, s) in layer.layer.sounds.iter().enumerate() {
                if s.pos_anim == Some(index) {
                    actions.push(EditorAction::ChangeSoundAttr(ActChangeSoundAttr {
                        is_background,
                        group_index,
                        layer_index,
                        index: sound_index,
                        old_attr: *s,
                        new_attr: Sound {
                            pos_anim: None,
                            ..*s
                        },
                    }));
                } else if s.pos_anim.is_some_and(|i| i > index) {
                    actions.push(EditorAction::ChangeSoundAttr(ActChangeSoundAttr {
                        is_background,
                        group_index,
                        layer_index,
                        index: sound_index,
                        old_attr: *s,
                        new_attr: Sound {
                            pos_anim: Some(s.pos_anim.unwrap() - 1),
                            ..*s
                        },
                    }));
                }
            }
        }
    }

    [
        actions,
        vec![EditorAction::RemPosAnim(ActRemPosAnim {
            base: ActAddRemPosAnim { index, anim },
        })],
    ]
    .concat()
}

pub fn rem_color_anim(
    anims: &[EditorColorAnimation],
    groups: &EditorGroups,
    index: usize,
) -> Vec<EditorAction> {
    let anim: AnimBase<_> = anims[index].clone().into();

    let mut actions = vec![];
    for (is_background, group_index, layer_index, layer) in groups
        .background
        .iter()
        .enumerate()
        .map(|g| (true, g))
        .chain(groups.foreground.iter().enumerate().map(|g| (false, g)))
        .flat_map(|(is_background, (g, group))| {
            group
                .layers
                .iter()
                .enumerate()
                .filter_map(move |(l, layer)| match layer {
                    EditorLayer::Abritrary(_) | EditorLayer::Sound(_) => None,
                    EditorLayer::Tile(_) | EditorLayer::Quad(_) => {
                        Some((is_background, g, l, layer))
                    }
                })
        })
    {
        if let EditorLayer::Quad(layer) = layer {
            for (quad_index, q) in layer.layer.quads.iter().enumerate() {
                if q.color_anim == Some(index) {
                    actions.push(EditorAction::ChangeQuadAttr(Box::new(ActChangeQuadAttr {
                        is_background,
                        group_index,
                        layer_index,
                        index: quad_index,
                        old_attr: *q,
                        new_attr: Quad {
                            color_anim: None,
                            ..*q
                        },
                    })));
                } else if q.color_anim.is_some_and(|i| i > index) {
                    actions.push(EditorAction::ChangeQuadAttr(Box::new(ActChangeQuadAttr {
                        is_background,
                        group_index,
                        layer_index,
                        index: quad_index,
                        old_attr: *q,
                        new_attr: Quad {
                            color_anim: Some(q.color_anim.unwrap() - 1),
                            ..*q
                        },
                    })));
                }
            }
        } else if let EditorLayer::Tile(layer) = layer {
            if layer.layer.attr.color_anim == Some(index) {
                actions.push(EditorAction::ChangeTileLayerDesignAttr(
                    ActChangeTileLayerDesignAttr {
                        is_background,
                        group_index,
                        layer_index,
                        old_attr: layer.layer.attr,
                        new_attr: MapTileLayerAttr {
                            color_anim: None,
                            ..layer.layer.attr
                        },
                        old_tiles: layer.layer.tiles.clone(),
                        new_tiles: layer.layer.tiles.clone(),
                    },
                ));
            } else if layer.layer.attr.color_anim.is_some_and(|i| i > index) {
                actions.push(EditorAction::ChangeTileLayerDesignAttr(
                    ActChangeTileLayerDesignAttr {
                        is_background,
                        group_index,
                        layer_index,
                        old_attr: layer.layer.attr,
                        new_attr: MapTileLayerAttr {
                            color_anim: Some(layer.layer.attr.color_anim.unwrap() - 1),
                            ..layer.layer.attr
                        },
                        old_tiles: layer.layer.tiles.clone(),
                        new_tiles: layer.layer.tiles.clone(),
                    },
                ));
            }
        }
    }

    [
        actions,
        vec![EditorAction::RemColorAnim(ActRemColorAnim {
            base: ActAddRemColorAnim { index, anim },
        })],
    ]
    .concat()
}

pub fn rem_sound_anim(
    anims: &[EditorSoundAnimation],
    groups: &EditorGroups,
    index: usize,
) -> Vec<EditorAction> {
    let anim: AnimBase<_> = anims[index].clone().into();

    let mut actions = vec![];
    for (is_background, group_index, layer_index, layer) in groups
        .background
        .iter()
        .enumerate()
        .map(|g| (true, g))
        .chain(groups.foreground.iter().enumerate().map(|g| (false, g)))
        .flat_map(|(is_background, (g, group))| {
            group
                .layers
                .iter()
                .enumerate()
                .filter_map(move |(l, layer)| match layer {
                    EditorLayer::Abritrary(_) | EditorLayer::Tile(_) | EditorLayer::Quad(_) => None,
                    EditorLayer::Sound(layer) => Some((is_background, g, l, layer)),
                })
        })
    {
        for (sound_index, s) in layer.layer.sounds.iter().enumerate() {
            if s.sound_anim == Some(index) {
                actions.push(EditorAction::ChangeSoundAttr(ActChangeSoundAttr {
                    is_background,
                    group_index,
                    layer_index,
                    index: sound_index,
                    old_attr: *s,
                    new_attr: Sound {
                        sound_anim: None,
                        ..*s
                    },
                }));
            } else if s.sound_anim.is_some_and(|i| i > index) {
                actions.push(EditorAction::ChangeSoundAttr(ActChangeSoundAttr {
                    is_background,
                    group_index,
                    layer_index,
                    index: sound_index,
                    old_attr: *s,
                    new_attr: Sound {
                        sound_anim: Some(s.sound_anim.unwrap() - 1),
                        ..*s
                    },
                }));
            }
        }
    }

    [
        actions,
        vec![EditorAction::RemSoundAnim(ActRemSoundAnim {
            base: ActAddRemSoundAnim { index, anim },
        })],
    ]
    .concat()
}
