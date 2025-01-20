use base::reduced_ascii_str::ReducedAsciiString;
use base_io::io::Io;
use map::map::{
    resources::{MapResourceMetaData, MapResourceRef},
    Map,
};

use crate::{
    actions::actions::{
        ActAddRemSound, ActAddSound, ActChangeSoundLayerAttr, ActRemSound, EditorAction,
        EditorActionGroup,
    },
    client::EditorClient,
    map::{EditorGroup, EditorGroupPanelResources, EditorGroups, EditorLayer, EditorResources},
};

pub fn render(
    ui: &mut egui::Ui,
    client: &mut EditorClient,
    groups: &EditorGroups,
    resources: &mut EditorResources,
    panel_data: &mut EditorGroupPanelResources,
    io: &Io,
) {
    super::resource_panel::render(
        ui,
        client,
        &mut resources.sounds,
        panel_data,
        io,
        |client, sounds, name, file| {
            let ty = name.extension().unwrap().to_string_lossy().to_string();
            let (name, hash) =
                Map::name_and_hash(&name.file_stem().unwrap().to_string_lossy(), &file);

            client.execute(
                EditorAction::AddSound(ActAddSound {
                    base: ActAddRemSound {
                        res: MapResourceRef {
                            name: ReducedAsciiString::from_str_autoconvert(&name),
                            meta: MapResourceMetaData {
                                blake3_hash: hash,
                                ty: ReducedAsciiString::from_str_autoconvert(&ty),
                            },
                            hq_meta: None,
                        },
                        file,
                        index: sounds.len(),
                    },
                }),
                None,
            );
        },
        |client, sounds, index| {
            let mut actions = Vec::new();
            let mut change_layers = |groups: &Vec<EditorGroup>, is_background: bool| {
                for (g, group) in groups.iter().enumerate() {
                    for (l, layer) in group.layers.iter().enumerate() {
                        if let EditorLayer::Sound(layer) = layer {
                            if layer.layer.attr.sound >= Some(index) {
                                let mut attr = layer.layer.attr;
                                attr.sound = if layer.layer.attr.sound == Some(index) {
                                    None
                                } else {
                                    layer.layer.attr.sound.map(|index| index - 1)
                                };
                                actions.push(EditorAction::ChangeSoundLayerAttr(
                                    ActChangeSoundLayerAttr {
                                        is_background,
                                        group_index: g,
                                        layer_index: l,
                                        old_attr: layer.layer.attr,
                                        new_attr: attr,
                                    },
                                ));
                            }
                        }
                    }
                }
            };

            change_layers(&groups.background, true);
            change_layers(&groups.foreground, false);

            actions.push(EditorAction::RemSound(ActRemSound {
                base: ActAddRemSound {
                    res: sounds[index].def.clone(),
                    file: sounds[index].user.file.as_ref().clone(),
                    index,
                },
            }));
            client.execute_group(EditorActionGroup {
                actions,
                identifier: None,
            })
        },
    );
}
