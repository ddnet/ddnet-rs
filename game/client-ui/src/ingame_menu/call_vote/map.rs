use std::{cmp::Ordering, collections::BTreeSet};

use base::network_string::NetworkString;
use client_containers::container::{ContainerItemIndexType, ContainerKey};
use egui::{Align2, Button, Color32, ComboBox, FontId, Frame, Layout, ScrollArea, Sense, Shadow};
use egui_extras::{Column, Size, StripBuilder, TableBuilder};
use game_base::server_browser::{SortDir, TableSort};
use game_config::config::Config;
use game_interface::votes::{
    MapCategoryVoteKey, MapDifficulty, MapVote, MapVoteDetails, MapVoteKey, RandomUnfinishedMapKey,
};
use math::math::{vector::vec2, RngSlice};
use serde::{Deserialize, Serialize};
use ui_base::{
    components::{
        clearable_edit_field::clearable_edit_field,
        menu_top_button::{menu_top_button, MenuTopButtonProps},
    },
    style::{bg_frame_color, topbar_buttons},
    types::{UiRenderPipe, UiState},
    utils::{add_margins, get_margin},
};

use crate::{
    events::UiEvent, ingame_menu::user_data::UserData, sort::sortable_header,
    time_display::TimeDisplay, utils::render_texture_for_ui,
};

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
enum ListView {
    #[default]
    Images,
    List,
}

pub fn stars_text(diff: MapDifficulty) -> String {
    let mut stars = String::new();
    let mut added_stars = 0;
    // full stars
    for _ in 0..diff.get() / 2 {
        stars.push('\u{f005}');
        added_stars += 1;
    }
    // eventually add half star
    if diff.get() % 2 != 0 {
        stars.push('\u{f5c0}');
        added_stars += 1;
    }
    // then fill with empty stars
    for _ in 0..5u32.saturating_sub(added_stars) {
        stars.push('☆');
    }
    stars
}

fn authors_text<const LEN: usize>(authors: &[NetworkString<LEN>]) -> String {
    let t = authors
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let t: String = t.chars().rev().collect();

    t.replacen(" ,", " & ", 1).chars().rev().collect()
}

const MAP_VOTE_DIR_STORAGE_NAME: &str = "map-vote-sort-dir";

fn render_table(
    ui: &mut egui::Ui,
    map_infos: &[(usize, &(MapVoteKey, MapVote))],
    index: usize,
    config: &mut Config,
    has_ddrace: bool,
    has_vanilla: bool,
) {
    let mut table = TableBuilder::new(ui).auto_shrink([false, false]);
    table = table.column(Column::auto().at_least(150.0));

    if has_ddrace {
        table = table.column(Column::auto().at_least(40.0));
        table = table.column(Column::auto().at_least(40.0));
        table = table.column(Column::remainder().at_least(40.0).clip(true));
        table = table.column(Column::auto().at_least(40.0));
    }
    if has_vanilla {
        table = table.column(Column::auto().at_least(20.0));
    }
    table
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .striped(true)
        .sense(Sense::click())
        .header(30.0, |mut row| {
            let mut names = Vec::new();
            names.push("Name");
            if has_ddrace {
                names.push("Difficulty");
                names.push("Points");
                names.push("Authors");
                names.push("Release date");
            }
            if has_vanilla {
                names.push("\u{f24e}");
            }
            sortable_header(&mut row, MAP_VOTE_DIR_STORAGE_NAME, config, &names);
        })
        .body(|body| {
            body.rows(25.0, map_infos.len(), |mut row| {
                let (original_index, (map, info)) = &map_infos[row.index()];
                row.set_selected(index == *original_index);
                row.col(|ui| {
                    ui.label(map.name.as_str());
                });
                match &info.details {
                    MapVoteDetails::None => {
                        if has_ddrace {
                            row.col(|_| {});
                            row.col(|_| {});
                            row.col(|_| {});
                            row.col(|_| {});
                        }
                        if has_vanilla {
                            row.col(|_| {});
                        }
                    }
                    MapVoteDetails::Ddrace {
                        points_reward,
                        difficulty,
                        release_date,
                        authors,
                    } => {
                        row.col(|ui| {
                            ui.label(stars_text(*difficulty));
                        });
                        row.col(|ui| {
                            ui.label(format!("{points_reward}"));
                        });
                        row.col(|ui| {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                            ui.label(authors_text(authors));
                        });
                        row.col(|ui| {
                            ui.label(release_date.to_local_time_string(true));
                        });
                        if has_vanilla {
                            row.col(|_| {});
                        }
                    }
                    MapVoteDetails::Vanilla { sided_friendly } => {
                        if has_ddrace {
                            row.col(|_| {});
                            row.col(|_| {});
                            row.col(|_| {});
                            row.col(|_| {});
                        }
                        row.col(|ui| {
                            if *sided_friendly {
                                ui.label("\u{f00c}");
                            } else {
                                ui.label("");
                            }
                        });
                    }
                }
                if row.response().clicked() {
                    config
                        .engine
                        .ui
                        .path
                        .query
                        .insert("vote-map-index".to_string(), original_index.to_string());
                }
            })
        });
}

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    let config = &mut *pipe.user_data.browser_menu.config;

    let mut sort_dir = config.storage::<TableSort>(MAP_VOTE_DIR_STORAGE_NAME);
    let prev_sort_dir = sort_dir.clone();
    let list_view: ListView = config.storage("vote-map-list-view");

    let path = &mut config.engine.ui.path;

    let mut map_search = path
        .query
        .entry("vote-map-search".to_string())
        .or_default()
        .clone();

    let mut category = path
        .query
        .entry("vote-map-category".to_string())
        .or_default()
        .as_str()
        .try_into()
        .unwrap_or_default();

    pipe.user_data.votes.request_map_votes();
    let mut map_votes = pipe.user_data.votes.collect_map_votes();
    let has_unfinished_map_votes = pipe.user_data.votes.has_unfinished_map_votes();

    let mut categories: Vec<_> = map_votes.keys().cloned().collect();
    categories.sort();
    let mut vote_category = map_votes.remove(&category);

    if vote_category.is_none() {
        if let Some((name, votes)) = categories.first().and_then(|c| map_votes.remove_entry(c)) {
            category = name;
            vote_category = Some(votes);
        }
    }

    let mut map_infos: Vec<(_, _)> = vote_category
        .map(|votes| votes.into_iter().collect())
        .unwrap_or_default();

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum MapSorting {
        Name,
        ReleaseDate,
        Difficulty,
        RewardPoints,
        Authors,
        SidedFriendly,
    }

    let mut has_ddrace = false;
    let mut has_vanilla = false;

    let mut sortings: BTreeSet<MapSorting> = Default::default();
    sortings.insert(MapSorting::Name);
    map_infos.iter().for_each(|(_, val)| {
        match val.details {
            MapVoteDetails::None => {
                // ignore
            }
            MapVoteDetails::Ddrace { .. } => {
                sortings.insert(MapSorting::Difficulty);
                sortings.insert(MapSorting::ReleaseDate);
                sortings.insert(MapSorting::RewardPoints);
                sortings.insert(MapSorting::Authors);
                has_ddrace = true;
            }
            MapVoteDetails::Vanilla { .. } => {
                sortings.insert(MapSorting::SidedFriendly);
                has_vanilla = true;
            }
        }
    });

    let cur_sort = match sort_dir.name.as_str() {
        "Release date" => MapSorting::ReleaseDate,
        "Difficulty" => MapSorting::Difficulty,
        "Authors" => MapSorting::Authors,
        "Points" => MapSorting::RewardPoints,
        "\u{f24e}" => MapSorting::SidedFriendly,
        // "Name" & rest
        _ => MapSorting::Name,
    };

    map_infos.sort_by(|(i1k, i1v), (i2k, i2v)| {
        let cmp = match cur_sort {
            MapSorting::Name => i1k.name.cmp(&i2k.name),
            MapSorting::ReleaseDate => {
                if let (
                    MapVoteDetails::Ddrace {
                        release_date: r1, ..
                    },
                    MapVoteDetails::Ddrace {
                        release_date: r2, ..
                    },
                ) = (&i1v.details, &i2v.details)
                {
                    r1.cmp(r2)
                } else if matches!(i1v.details, MapVoteDetails::Ddrace { .. }) {
                    Ordering::Greater
                } else if matches!(i2v.details, MapVoteDetails::Ddrace { .. }) {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            }
            MapSorting::Difficulty => {
                if let (
                    MapVoteDetails::Ddrace {
                        release_date: r1, ..
                    },
                    MapVoteDetails::Ddrace {
                        release_date: r2, ..
                    },
                ) = (&i1v.details, &i2v.details)
                {
                    r1.cmp(r2)
                } else if matches!(i1v.details, MapVoteDetails::Ddrace { .. }) {
                    Ordering::Greater
                } else if matches!(i2v.details, MapVoteDetails::Ddrace { .. }) {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            }
            MapSorting::RewardPoints => {
                if let (
                    MapVoteDetails::Ddrace {
                        points_reward: r1, ..
                    },
                    MapVoteDetails::Ddrace {
                        points_reward: r2, ..
                    },
                ) = (&i1v.details, &i2v.details)
                {
                    r1.cmp(r2)
                } else if matches!(i1v.details, MapVoteDetails::Ddrace { .. }) {
                    Ordering::Greater
                } else if matches!(i2v.details, MapVoteDetails::Ddrace { .. }) {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            }
            MapSorting::Authors => {
                if let (
                    MapVoteDetails::Ddrace { authors: r1, .. },
                    MapVoteDetails::Ddrace { authors: r2, .. },
                ) = (&i1v.details, &i2v.details)
                {
                    r1.cmp(r2)
                } else if matches!(i1v.details, MapVoteDetails::Ddrace { .. }) {
                    Ordering::Greater
                } else if matches!(i2v.details, MapVoteDetails::Ddrace { .. }) {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            }
            MapSorting::SidedFriendly => {
                if let (
                    MapVoteDetails::Vanilla {
                        sided_friendly: r1, ..
                    },
                    MapVoteDetails::Vanilla {
                        sided_friendly: r2, ..
                    },
                ) = (&i1v.details, &i2v.details)
                {
                    r1.cmp(r2)
                } else if matches!(i1v.details, MapVoteDetails::Vanilla { .. }) {
                    Ordering::Greater
                } else if matches!(i2v.details, MapVoteDetails::Vanilla { .. }) {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            }
        };
        if matches!(sort_dir.sort_dir, SortDir::Desc) {
            cmp.reverse()
        } else {
            cmp
        }
    });

    let has_preview_thumbnail = map_infos
        .iter()
        .any(|(_, v)| v.thumbnail_resource.is_some());
    let list_view = if has_preview_thumbnail {
        list_view
    } else {
        ListView::List
    };

    let category = category.to_string();

    let index_entry = path
        .query
        .entry("vote-map-index".to_string())
        .or_default()
        .clone();
    let index: usize = index_entry.parse().unwrap_or_default();

    pipe.user_data
        .map_vote_thumbnail_container
        .set_resource_download_url_and_clear_on_change(
            pipe.user_data
                .votes
                .thumbnail_server_resource_download_url(),
        );

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
                                                "vote-map-category".to_string(),
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
                        match list_view {
                            ListView::Images => {
                                // show as list
                                use super::super::super::main_menu::settings::list;
                                list::list::render(
                                    ui,
                                    map_infos.iter().map(|(key, _)| {
                                        (key.name.as_str(), ContainerItemIndexType::Disk)
                                    }),
                                    300.0,
                                    |_, _| Ok(()),
                                    |i, _| index == i,
                                    |ui, i, _, pos, size| {
                                        let (_, info) = &map_infos[i];
                                        let key =
                                            info.thumbnail_resource.map(|hash| ContainerKey {
                                                name: "map".try_into().unwrap(),
                                                hash: Some(hash),
                                            });
                                        let thumbnail = pipe
                                            .user_data
                                            .map_vote_thumbnail_container
                                            .get_or_default_opt(key.as_ref());
                                        ui.painter().text(
                                            egui::pos2(pos.x, pos.y - size / 2.0 + 9.0),
                                            Align2::CENTER_TOP,
                                            match &info.details {
                                                MapVoteDetails::None => "".to_string(),
                                                MapVoteDetails::Ddrace {
                                                    points_reward,
                                                    difficulty,
                                                    ..
                                                } => format!(
                                                    "Difficulty: {}, Points: {}",
                                                    stars_text(*difficulty),
                                                    points_reward
                                                ),
                                                MapVoteDetails::Vanilla { sided_friendly } => {
                                                    if *sided_friendly {
                                                        "\u{f24e} Team friendly".to_string()
                                                    } else {
                                                        "".to_string()
                                                    }
                                                }
                                            },
                                            FontId::proportional(12.0),
                                            Color32::WHITE,
                                        );
                                        // draw thumbnail
                                        let width = thumbnail.width as f32;
                                        let height = thumbnail.height as f32;
                                        let draw_width = size;
                                        let draw_height = size - 30.0 * 2.0;
                                        let w_scale = draw_width / width;
                                        let h_scale = draw_height / height;
                                        let scale = w_scale.min(h_scale).min(1.0);
                                        render_texture_for_ui(
                                            pipe.user_data.browser_menu.stream_handle,
                                            pipe.user_data.browser_menu.canvas_handle,
                                            &thumbnail.thumbnail,
                                            ui,
                                            ui_state,
                                            ui.ctx().screen_rect(),
                                            Some(ui.clip_rect()),
                                            pos,
                                            vec2::new(width * scale, height * scale),
                                        );
                                        ui.painter().text(
                                            egui::pos2(pos.x, pos.y + size / 2.0 - 9.0),
                                            Align2::CENTER_BOTTOM,
                                            match &info.details {
                                                MapVoteDetails::None
                                                | MapVoteDetails::Vanilla { .. } => "".to_string(),
                                                MapVoteDetails::Ddrace {
                                                    release_date,
                                                    authors,
                                                    ..
                                                } => format!(
                                                    "By: {}, On: {}",
                                                    authors_text(authors),
                                                    release_date.to_local_time_string(true)
                                                ),
                                            },
                                            FontId::proportional(12.0),
                                            Color32::WHITE,
                                        );
                                    },
                                    |i, _| {
                                        config
                                            .engine
                                            .ui
                                            .path
                                            .query
                                            .insert("vote-map-index".to_string(), i.to_string());
                                    },
                                    |_, _| Some("".into()),
                                    &mut map_search,
                                    |ui| {
                                        ui.horizontal(|ui| {
                                            if ui
                                                .button(match sort_dir.sort_dir {
                                                    SortDir::Asc => "\u{f160}",
                                                    SortDir::Desc => "\u{f161}",
                                                })
                                                .clicked()
                                            {
                                                sort_dir.sort_dir = match sort_dir.sort_dir {
                                                    SortDir::Asc => SortDir::Desc,
                                                    SortDir::Desc => SortDir::Asc,
                                                };
                                            }
                                            fn sort_to_text(sort: MapSorting) -> String {
                                                match sort {
                                                    MapSorting::Name => "Name",
                                                    MapSorting::ReleaseDate => "Release date",
                                                    MapSorting::Difficulty => "Difficulty",
                                                    MapSorting::RewardPoints => "Points",
                                                    MapSorting::Authors => "Authors",
                                                    MapSorting::SidedFriendly => "\u{f24e}",
                                                }
                                                .into()
                                            }
                                            ComboBox::new("map-vote-sorting-list-imgs", "")
                                                .selected_text(sort_to_text(cur_sort))
                                                .show_ui(ui, |ui| {
                                                    for sorting in sortings {
                                                        let name = sort_to_text(sorting);
                                                        if ui.button(&name).clicked() {
                                                            sort_dir.name = name;
                                                        }
                                                    }
                                                });
                                        });
                                    },
                                );
                            }
                            ListView::List => {
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
                                                let map_infos: Vec<_> = map_infos
                                                    .iter()
                                                    .enumerate()
                                                    .filter(|(_, (key, _))| {
                                                        key.name
                                                            .as_str()
                                                            .to_lowercase()
                                                            .contains(&map_search.to_lowercase())
                                                    })
                                                    .collect();
                                                render_table(
                                                    ui,
                                                    &map_infos,
                                                    index,
                                                    config,
                                                    has_ddrace,
                                                    has_vanilla,
                                                );
                                            });
                                        });
                                        strip.cell(|ui| {
                                            ui.style_mut().wrap_mode = None;
                                            ui.horizontal_centered(|ui| {
                                                // Search
                                                ui.label("\u{1f50d}");
                                                clearable_edit_field(
                                                    ui,
                                                    &mut map_search,
                                                    Some(200.0),
                                                    None,
                                                );
                                            });
                                        });
                                    });
                            }
                        }
                    });
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;
                        ui.horizontal(|ui| {
                            if ui.button("Change").clicked() {
                                if let Some((map, _)) = map_infos.get(index) {
                                    pipe.user_data.browser_menu.events.push(UiEvent::VoteMap(
                                        MapCategoryVoteKey {
                                            category: category.as_str().try_into().unwrap(),
                                            map: map.clone(),
                                        },
                                    ));
                                }
                            }

                            if has_unfinished_map_votes {
                                ui.menu_button("\u{f522} Random unfinished", |ui| {
                                    for i in 0..10 {
                                        if ui
                                            .button(stars_text(MapDifficulty::new(i).unwrap()))
                                            .clicked()
                                        {
                                            pipe.user_data.browser_menu.events.push(
                                                UiEvent::VoteRandomUnfinishedMap(
                                                    RandomUnfinishedMapKey {
                                                        category: category
                                                            .as_str()
                                                            .try_into()
                                                            .unwrap(),
                                                        difficulty: Some(
                                                            MapDifficulty::new(i).unwrap(),
                                                        ),
                                                    },
                                                ),
                                            );
                                        }
                                    }
                                    if ui.button("Any").clicked() {
                                        pipe.user_data.browser_menu.events.push(
                                            UiEvent::VoteRandomUnfinishedMap(
                                                RandomUnfinishedMapKey {
                                                    category: category.as_str().try_into().unwrap(),
                                                    difficulty: None,
                                                },
                                            ),
                                        );
                                    }
                                });
                            }

                            if !map_infos.is_empty() && ui.button("\u{f522} Random").clicked() {
                                let (map, _) = map_infos.random_entry(pipe.user_data.rng);
                                pipe.user_data.browser_menu.events.push(UiEvent::VoteMap(
                                    MapCategoryVoteKey {
                                        category: category.as_str().try_into().unwrap(),
                                        map: map.clone(),
                                    },
                                ));
                            }

                            if has_preview_thumbnail {
                                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                                    // image view
                                    if ui
                                        .add(
                                            Button::new("\u{f03e}")
                                                .selected(matches!(list_view, ListView::Images)),
                                        )
                                        .clicked()
                                    {
                                        config.set_storage("vote-map-list-view", &ListView::Images);
                                    }
                                    // list view
                                    if ui
                                        .add(
                                            Button::new("\u{f03a}")
                                                .selected(matches!(list_view, ListView::List)),
                                        )
                                        .clicked()
                                    {
                                        config.set_storage("vote-map-list-view", &ListView::List);
                                    }
                                });
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
        .insert("vote-map-search".to_string(), map_search);

    if sort_dir != prev_sort_dir {
        config.set_storage(MAP_VOTE_DIR_STORAGE_NAME, &sort_dir);
    }
}
