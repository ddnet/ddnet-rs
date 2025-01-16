use egui_extras::TableRow;
use game_base::server_browser::{SortDir, TableSort};
use game_config::config::Config;

use crate::sort::sortable_header;

/// table header
pub fn render(header: &mut TableRow<'_, '_>, config: &mut Config) {
    let sort: TableSort = config.storage("browser_sort");
    if sort.name.is_empty() {
        config.set_storage(
            "browser_sort",
            &TableSort {
                name: "Players".to_string(),
                sort_dir: SortDir::Desc,
            },
        );
    }
    sortable_header(
        header,
        "browser_sort",
        config,
        &["", "Name", "Type", "Map", "Players", "Ping"],
    );
}
