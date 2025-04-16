use std::path::Path;

use base::hash::generate_hash_for;
use base_io::io::Io;
use egui::{vec2, Button, Layout, ScrollArea};
use egui_file_dialog::{DialogMode, DialogState};
use map::skeleton::resources::MapResourceRefSkeleton;

use crate::{
    client::EditorClient, fs::read_file_editor, map::EditorGroupPanelResources,
    notifications::EditorNotification,
};

pub fn render<F, R, U>(
    ui: &mut egui::Ui,
    client: &EditorClient,
    resources: &mut Vec<MapResourceRefSkeleton<U>>,
    panel_data: &mut EditorGroupPanelResources,
    io: &Io,
    load_resource: F,
    rem_resource: R,
) where
    F: Fn(&EditorClient, &mut Vec<MapResourceRefSkeleton<U>>, &Path, Vec<u8>),
    R: Fn(&EditorClient, &mut Vec<MapResourceRefSkeleton<U>>, usize),
{
    ScrollArea::vertical().show(ui, |ui| {
        ui.vertical(|ui| {
            let mut del_index = None;
            for (index, resource) in resources.iter().enumerate() {
                ui.with_layout(Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    if ui.button("\u{f2ed}").clicked() {
                        del_index = Some(index);
                    }

                    ui.vertical_centered_justified(|ui| {
                        if ui.add(Button::new(resource.def.name.as_str())).clicked() {
                            // show resource?
                        }
                    });
                });
            }

            if let Some(index) = del_index {
                rem_resource(client, resources, index);
            }

            if ui.button("\u{f0fe}").clicked() {
                panel_data.file_dialog.pick_file();
            }
        });
    });

    panel_data.loading_tasks = panel_data
        .loading_tasks
        .drain()
        .filter_map(|(name, task)| {
            if task.is_finished() {
                if let Ok(file) = task.get_storage() {
                    let hash = generate_hash_for(&file);
                    if resources.iter().any(|r| r.def.meta.blake3_hash == hash) {
                        client.notifications.push(EditorNotification::Warning(
                            "A resource with identical file \
                            hash already exists."
                                .to_string(),
                        ));
                    } else {
                        load_resource(client, resources, name.as_ref(), file);
                    }
                }
                None
            } else {
                Some((name, task))
            }
        })
        .collect();

    let file_dialog = &mut panel_data.file_dialog;
    if file_dialog.state() == DialogState::Open {
        let mode = file_dialog.mode();
        if let Some(selected) = file_dialog.update(ui.ctx()).picked() {
            match mode {
                DialogMode::PickFile => {
                    let selected = selected.to_path_buf();
                    let fs = io.fs.clone();
                    panel_data.loading_tasks.insert(
                        selected.to_path_buf(),
                        io.rt
                            .spawn(async move { read_file_editor(&fs, selected.as_ref()).await }),
                    );
                }
                DialogMode::PickDirectory | DialogMode::SaveFile => {
                    panic!("")
                }
                DialogMode::PickMultiple => {
                    panic!("multi select currently isn't implemented.")
                }
            }
        }
    }
}
