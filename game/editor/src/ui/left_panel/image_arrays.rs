use base::reduced_ascii_str::ReducedAsciiString;
use base_io::io::Io;
use map::map::{
    Map,
    resources::{MapResourceMetaData, MapResourceRef},
};

use crate::{
    actions::actions::{
        ActAddImage2dArray, ActAddRemImage, ActChangeTileLayerDesignAttr, ActRemImage2dArray,
        EditorAction, EditorActionGroup,
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
        &mut resources.image_arrays,
        panel_data,
        io,
        |client, image_arrays, name, file| {
            let ty = name.extension().unwrap().to_string_lossy().to_string();
            let (name, hash) =
                Map::name_and_hash(&name.file_stem().unwrap().to_string_lossy(), &file);

            client.execute(
                EditorAction::AddImage2dArray(ActAddImage2dArray {
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
                        index: image_arrays.len(),
                    },
                }),
                None,
            );
        },
        |client, image_arrays, index| {
            let mut actions = Vec::new();
            let mut change_layers = |groups: &Vec<EditorGroup>, is_background: bool| {
                for (g, group) in groups.iter().enumerate() {
                    for (l, layer) in group.layers.iter().enumerate() {
                        if let EditorLayer::Tile(layer) = layer
                            && layer.layer.attr.image_array >= Some(index)
                        {
                            let mut attr = layer.layer.attr;
                            attr.image_array = if layer.layer.attr.image_array == Some(index) {
                                None
                            } else {
                                layer.layer.attr.image_array.map(|index| index - 1)
                            };
                            actions.push(EditorAction::ChangeTileLayerDesignAttr(
                                ActChangeTileLayerDesignAttr {
                                    is_background,
                                    group_index: g,
                                    layer_index: l,
                                    old_attr: layer.layer.attr,
                                    new_attr: attr,

                                    old_tiles: layer.layer.tiles.clone(),
                                    new_tiles: layer.layer.tiles.clone(),
                                },
                            ));
                        }
                    }
                }
            };

            change_layers(&groups.background, true);
            change_layers(&groups.foreground, false);

            actions.push(EditorAction::RemImage2dArray(ActRemImage2dArray {
                base: ActAddRemImage {
                    res: image_arrays[index].def.clone(),
                    file: image_arrays[index].user.file.as_ref().clone(),
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
