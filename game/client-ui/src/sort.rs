use egui_extras::TableRow;
use game_base::server_browser::{SortDir, TableSort};
use game_config::config::Config;

pub fn sortable_header(
    header: &mut TableRow<'_, '_>,
    storage_name: &str,
    config: &mut Config,
    names: &[&str],
) {
    let sort: TableSort = config.storage(storage_name);
    let mut item = |name: &str| {
        let is_selected = name == sort.name;
        header.set_selected(is_selected);
        let mut clicked = false;
        clicked |= header
            .col(|ui| {
                ui.horizontal(|ui| {
                    clicked |= ui.strong(name).clicked();
                    if is_selected {
                        clicked |= ui
                            .strong(match sort.sort_dir {
                                SortDir::Asc => "\u{f0de}",
                                SortDir::Desc => "\u{f0dd}",
                            })
                            .clicked();
                    }
                });
            })
            .1
            .clicked();

        if clicked {
            config.set_storage::<TableSort>(
                storage_name,
                &TableSort {
                    name: name.to_string(),
                    sort_dir: if is_selected {
                        match sort.sort_dir {
                            SortDir::Asc => SortDir::Desc,
                            SortDir::Desc => SortDir::Asc,
                        }
                    } else {
                        Default::default()
                    },
                },
            );
        }
    };

    for name in names {
        item(name);
    }
}
