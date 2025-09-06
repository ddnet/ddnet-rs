use base::reduced_ascii_str::ReducedAsciiString;
use base_io::io::Io;
use map::map::{
    Map,
    resources::{MapResourceMetaData, MapResourceRef},
};

use crate::{
    actions::actions::{
        ActAddImage, ActAddRemImage, ActChangeQuadLayerAttr, ActRemImage, EditorAction,
        EditorActionGroup,
    },
    client::EditorClient,
    map::{EditorGroup, EditorGroupPanelResources, EditorGroups, EditorLayer, EditorResources},
};

use super::resource_limit::check_legacy_resource_limit_images;

pub fn render(
    ui: &mut egui::Ui,
    client: &EditorClient,
    groups: &EditorGroups,
    resources: &mut EditorResources,
    panel_data: &mut EditorGroupPanelResources,
    io: &Io,
) {
    check_legacy_resource_limit_images(client, resources);
    super::resource_panel::render(
        ui,
        client,
        &mut resources.images,
        panel_data,
        io,
        |client, images, name, file| {
            let ty = name.extension().unwrap().to_string_lossy().to_string();
            let (name, hash) =
                Map::name_and_hash(&name.file_stem().unwrap().to_string_lossy(), &file);

            client.execute(
                EditorAction::AddImage(ActAddImage {
                    base: ActAddRemImage {
                        res: MapResourceRef {
                            name: ReducedAsciiString::from_str_autoconvert(&name),
                            meta: MapResourceMetaData {
                                blake3_hash: hash,
                                ty: ReducedAsciiString::from_str_autoconvert(&ty),
                            },
                            hq_meta: None,
                        },
                        file,
                        index: images.len(),
                    },
                }),
                None,
            );
        },
        |client, images, index| {
            let mut actions = Vec::new();
            let mut change_layers = |groups: &Vec<EditorGroup>, is_background: bool| {
                for (g, group) in groups.iter().enumerate() {
                    for (l, layer) in group.layers.iter().enumerate() {
                        if let EditorLayer::Quad(layer) = layer
                            && layer.layer.attr.image >= Some(index)
                        {
                            let mut attr = layer.layer.attr;
                            attr.image = if layer.layer.attr.image == Some(index) {
                                None
                            } else {
                                layer.layer.attr.image.map(|index| index - 1)
                            };
                            actions.push(EditorAction::ChangeQuadLayerAttr(
                                ActChangeQuadLayerAttr {
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
            };

            change_layers(&groups.background, true);
            change_layers(&groups.foreground, false);

            actions.push(EditorAction::RemImage(ActRemImage {
                base: ActAddRemImage {
                    res: images[index].def.clone(),
                    file: images[index].user.file.as_ref().clone(),
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
