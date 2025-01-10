use std::collections::BTreeSet;

use egui::{Frame, ScrollArea, Sense, Shadow};
use egui_extras::{Column, Size, StripBuilder, TableBuilder};
use game_base::server_browser::{SortDir, TableSort};
use game_config::config::Config;
use game_interface::votes::{MiscVote, MiscVoteCategoryKey, MiscVoteKey};
use ui_base::{
    components::{
        clearable_edit_field::clearable_edit_field,
        menu_top_button::{menu_top_button, MenuTopButtonProps},
    },
    style::{bg_frame_color, topbar_buttons},
    types::UiRenderPipe,
    utils::{add_margins, get_margin},
};

use crate::{events::UiEvent, ingame_menu::user_data::UserData, sort::sortable_header};

const MISC_VOTE_DIR_STORAGE_NAME: &str = "misc-vote-sort-dir";

fn render_table(
    ui: &mut egui::Ui,
    misc_infos: &[(usize, &(MiscVoteKey, MiscVote))],
    index: usize,
    config: &mut Config,
) {
    let mut table = TableBuilder::new(ui).auto_shrink([false, false]);
    table = table.column(Column::auto().at_least(150.0));

    table
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .striped(true)
        .sense(Sense::click())
        .header(30.0, |mut row| {
            let names = vec!["Name"];
            sortable_header(&mut row, MISC_VOTE_DIR_STORAGE_NAME, config, &names);
        })
        .body(|body| {
            body.rows(25.0, misc_infos.len(), |mut row| {
                let (original_index, (misc, _)) = &misc_infos[row.index()];
                row.set_selected(index == *original_index);
                row.col(|ui| {
                    ui.label(misc.display_name.as_str());
                });
                if row.response().clicked() {
                    config
                        .engine
                        .ui
                        .path
                        .query
                        .insert("vote-misc-index".to_string(), original_index.to_string());
                }
            })
        });
}

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>) {
    let config = &mut *pipe.user_data.browser_menu.config;

    let sort_dir = config.storage::<TableSort>(MISC_VOTE_DIR_STORAGE_NAME);

    let path = &mut config.engine.ui.path;

    let mut misc_search = path
        .query
        .entry("vote-misc-search".to_string())
        .or_default()
        .clone();

    let mut category = path
        .query
        .entry("vote-misc-category".to_string())
        .or_default()
        .as_str()
        .try_into()
        .unwrap_or_default();

    pipe.user_data.votes.request_misc_votes();
    let mut misc_votes = pipe.user_data.votes.collect_misc_votes();

    let mut categories: Vec<_> = misc_votes.keys().cloned().collect();
    categories.sort();
    let mut vote_category = misc_votes.remove(&category);

    if vote_category.is_none() {
        if let Some((name, votes)) = categories.first().and_then(|c| misc_votes.remove_entry(c)) {
            category = name;
            vote_category = Some(votes);
        }
    }

    let mut misc_infos: Vec<(_, _)> = vote_category
        .map(|votes| votes.into_iter().collect())
        .unwrap_or_default();

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum MiscSorting {
        Name,
    }

    let mut sortings: BTreeSet<MiscSorting> = Default::default();
    sortings.insert(MiscSorting::Name);

    let cur_sort = MiscSorting::Name;

    misc_infos.sort_by(|(i1k, _), (i2k, _)| {
        let cmp = match cur_sort {
            MiscSorting::Name => i1k.display_name.cmp(&i2k.display_name),
        };
        if matches!(sort_dir.sort_dir, SortDir::Desc) {
            cmp.reverse()
        } else {
            cmp
        }
    });

    let category = category.to_string();

    let index_entry = path
        .query
        .entry("vote-misc-index".to_string())
        .or_default()
        .clone();
    let index: usize = index_entry.parse().unwrap_or_default();

    Frame::default()
        .fill(bg_frame_color())
        .inner_margin(get_margin(ui))
        .shadow(Shadow::NONE)
        .show(ui, |ui| {
            let mut builder = StripBuilder::new(ui);

            let has_multi_categories = categories.len() > 1;
            if has_multi_categories {
                builder = builder.size(Size::exact(20.0));
                builder = builder.size(Size::exact(2.0));
            }

            builder
                .size(Size::remainder())
                .size(Size::exact(20.0))
                .vertical(|mut strip| {
                    if has_multi_categories {
                        strip.cell(|ui| {
                            ui.style_mut().wrap_mode = None;
                            ScrollArea::horizontal().show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.set_style(topbar_buttons());
                                    for category_name in categories {
                                        if menu_top_button(
                                            ui,
                                            |_, _| None,
                                            MenuTopButtonProps::new(
                                                &category_name,
                                                &Some(category.clone()),
                                            ),
                                        )
                                        .clicked()
                                        {
                                            config.engine.ui.path.query.insert(
                                                "vote-misc-category".to_string(),
                                                category_name.to_string(),
                                            );
                                        }
                                    }
                                });
                            });
                        });
                        strip.empty();
                    }
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;
                        ui.style_mut().spacing.item_spacing.y = 0.0;
                        StripBuilder::new(ui)
                            .size(Size::remainder())
                            .size(Size::exact(20.0))
                            .vertical(|mut strip| {
                                strip.cell(|ui| {
                                    ui.style_mut().wrap_mode = None;
                                    ui.painter().rect_filled(
                                        ui.available_rect_before_wrap(),
                                        0.0,
                                        bg_frame_color(),
                                    );
                                    ui.set_clip_rect(ui.available_rect_before_wrap());
                                    add_margins(ui, |ui| {
                                        let misc_infos: Vec<_> = misc_infos
                                            .iter()
                                            .enumerate()
                                            .filter(|(_, (key, _))| {
                                                key.display_name
                                                    .as_str()
                                                    .to_lowercase()
                                                    .contains(&misc_search.to_lowercase())
                                            })
                                            .collect();
                                        render_table(ui, &misc_infos, index, config);
                                    });
                                });
                                strip.cell(|ui| {
                                    ui.style_mut().wrap_mode = None;
                                    ui.horizontal_centered(|ui| {
                                        // Search
                                        ui.label("\u{1f50d}");
                                        clearable_edit_field(
                                            ui,
                                            &mut misc_search,
                                            Some(200.0),
                                            None,
                                        );
                                    });
                                });
                            });
                    });
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;
                        ui.horizontal(|ui| {
                            if ui.button("Vote").clicked() {
                                if let Some((vote_key, _)) = misc_infos.get(index) {
                                    pipe.user_data.browser_menu.events.push(UiEvent::VoteMisc(
                                        MiscVoteCategoryKey {
                                            category: category.as_str().try_into().unwrap(),
                                            vote_key: vote_key.clone(),
                                        },
                                    ));
                                }
                            }
                        });
                    });
                });
        });

    config
        .engine
        .ui
        .path
        .query
        .insert("vote-misc-search".to_string(), misc_search);
}
