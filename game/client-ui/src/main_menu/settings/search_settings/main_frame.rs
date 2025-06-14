use config::{
    config::ConfigEngine,
    parsing::{parse_conf_values_as_str_list, AddFeedback},
    traits::{ConfigFromStrOperation, ConfigInterface, ConfigValue},
    types::ConfRgb,
};
use egui::{CollapsingHeader, Color32, ComboBox, DragValue, Grid, ScrollArea, TextEdit};
use game_config::config::ConfigGame;
use ui_base::{components::clearable_edit_field::clearable_edit_field, types::UiRenderPipe};

use crate::main_menu::user_data::UserData;

#[derive(Debug, Clone)]
struct UiConfigVal {
    config: AddFeedback,
    val: ConfigValue,
}

enum ModifierTy<'a> {
    None,
    Array(&'a str),
    Key(&'a str),
}

fn render_conf_val(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    mut values: Vec<UiConfigVal>,
    search: &str,
    modifier: ModifierTy,
    prefix: &str,
) {
    fn get(path: &str, game: &mut ConfigGame, engine: &mut ConfigEngine) -> String {
        game.try_set_from_str(
            path.to_string(),
            None,
            None,
            None,
            ConfigFromStrOperation::Set,
        )
        .or_else(|_| {
            engine.try_set_from_str(
                path.to_string(),
                None,
                None,
                None,
                ConfigFromStrOperation::Set,
            )
        })
        .unwrap_or_default()
    }
    fn set(path: &str, game: &mut ConfigGame, engine: &mut ConfigEngine, val: String) {
        if game
            .try_set_from_str(
                path.to_string(),
                None,
                Some(val.clone()),
                None,
                ConfigFromStrOperation::Set,
            )
            .is_err()
        {
            let _ = engine.try_set_from_str(
                path.to_string(),
                None,
                Some(val),
                None,
                ConfigFromStrOperation::Set,
            );
        }
    }

    let mut keep_values: Vec<_> = Default::default();
    values.retain_mut(|v| {
        match modifier {
            ModifierTy::None => {}
            ModifierTy::Array(modifier) => {
                v.config.name = v
                    .config
                    .name
                    .replacen("$INDEX$", &format!("[{modifier}]"), 1);
            }
            ModifierTy::Key(modifier) => {
                v.config.name = v.config.name.replacen("$KEY$", &format!("[{modifier}]"), 1);
            }
        }

        let keep = v.config.name.starts_with(prefix)
            && !v.config.name.contains("$INDEX$")
            && !v.config.name.contains("$KEY$");

        if !keep {
            keep_values.push(v.clone());
        }

        keep
    });

    for value in values.into_iter().filter(|val| {
        val.config
            .name
            .to_lowercase()
            .contains(&search.to_lowercase())
            || val
                .config
                .description
                .to_lowercase()
                .contains(&search.to_lowercase())
    }) {
        if matches!(&value.val, ConfigValue::Struct { .. }) {
            continue;
        }

        ui.label(&value.config.name)
            .on_hover_text(&value.config.description);
        match value.val {
            ConfigValue::Boolean => {
                let mut val: bool = get(
                    &value.config.name,
                    &mut pipe.user_data.config.game,
                    &mut pipe.user_data.config.engine,
                )
                .parse()
                .unwrap_or_default();
                if ui.checkbox(&mut val, "").changed() {
                    set(
                        &value.config.name,
                        &mut pipe.user_data.config.game,
                        &mut pipe.user_data.config.engine,
                        val.to_string(),
                    );
                }
            }
            ConfigValue::Int { min, max } => {
                let mut val: i64 = get(
                    &value.config.name,
                    &mut pipe.user_data.config.game,
                    &mut pipe.user_data.config.engine,
                )
                .parse()
                .unwrap_or_default();
                if ui
                    .add(
                        DragValue::new(&mut val)
                            .update_while_editing(false)
                            .range(min..=max.clamp(0, i64::MAX as u64) as i64),
                    )
                    .changed()
                {
                    set(
                        &value.config.name,
                        &mut pipe.user_data.config.game,
                        &mut pipe.user_data.config.engine,
                        val.to_string(),
                    );
                }
            }
            ConfigValue::Float { min, max } => {
                let mut val: f64 = get(
                    &value.config.name,
                    &mut pipe.user_data.config.game,
                    &mut pipe.user_data.config.engine,
                )
                .parse()
                .unwrap_or_default();
                if ui
                    .add(
                        DragValue::new(&mut val)
                            .update_while_editing(false)
                            .range(min..=max),
                    )
                    .changed()
                {
                    set(
                        &value.config.name,
                        &mut pipe.user_data.config.game,
                        &mut pipe.user_data.config.engine,
                        val.to_string(),
                    );
                }
            }
            ConfigValue::String {
                min_length,
                max_length,
            } => {
                let mut val = get(
                    &value.config.name,
                    &mut pipe.user_data.config.game,
                    &mut pipe.user_data.config.engine,
                );
                if ui
                    .add(TextEdit::singleline(&mut val).char_limit(max_length))
                    .changed()
                {
                    while val.chars().count() < min_length {
                        val.push(' ');
                    }
                    set(
                        &value.config.name,
                        &mut pipe.user_data.config.game,
                        &mut pipe.user_data.config.engine,
                        val,
                    );
                }
            }
            ConfigValue::Color => {
                let val: ConfRgb = ConfRgb::from_display(&get(
                    &value.config.name,
                    &mut pipe.user_data.config.game,
                    &mut pipe.user_data.config.engine,
                ))
                .unwrap_or_default();
                let mut val = [val.r, val.g, val.b];
                if ui.color_edit_button_srgb(&mut val).changed() {
                    let val = ConfRgb {
                        r: val[0],
                        g: val[1],
                        b: val[2],
                    };
                    set(
                        &value.config.name,
                        &mut pipe.user_data.config.game,
                        &mut pipe.user_data.config.engine,
                        val.to_string(),
                    );
                }
            }
            ConfigValue::StringOfList { allowed_values } => {
                let mut val = get(
                    &value.config.name,
                    &mut pipe.user_data.config.game,
                    &mut pipe.user_data.config.engine,
                );
                let mut changed = false;
                ComboBox::new(&value.config.name, "")
                    .selected_text(&val)
                    .show_ui(ui, |ui| {
                        for allowed_value in allowed_values {
                            if ui.button(&allowed_value).clicked() {
                                val = allowed_value;
                                changed = true;
                            }
                        }
                    });
                if changed {
                    set(
                        &value.config.name,
                        &mut pipe.user_data.config.game,
                        &mut pipe.user_data.config.engine,
                        val,
                    );
                }
            }
            ConfigValue::Array { .. } => {
                let val = get(
                    &value.config.name,
                    &mut pipe.user_data.config.game,
                    &mut pipe.user_data.config.engine,
                );
                CollapsingHeader::new("This is an array of values")
                    .id_salt(format!("conf-val-array-{}", value.config.name))
                    .default_open(false)
                    .show(ui, |ui| {
                        let Ok(serde_json::Value::Array(array)) = serde_json::from_str(&val) else {
                            return;
                        };
                        for (index, _) in array.into_iter().enumerate() {
                            let modifier = format!("{index}");
                            CollapsingHeader::new(&modifier)
                                .id_salt(format!("conf-val-array-{}-{}", value.config.name, index))
                                .default_open(false)
                                .show(ui, |ui| {
                                    let values = keep_values
                                        .iter()
                                        .filter(|&v| v.config.name.starts_with(&value.config.name))
                                        .cloned()
                                        .collect();
                                    render_conf_val(
                                        ui,
                                        pipe,
                                        values,
                                        search,
                                        ModifierTy::Array(&modifier),
                                        &value.config.name,
                                    );
                                });
                        }

                        ui.horizontal(|ui| {
                            let game = &mut pipe.user_data.config.game;
                            let engine = &mut pipe.user_data.config.engine;
                            if ui.button("\u{f055} Push").clicked() {
                                let _ = game
                                    .try_set_from_str(
                                        value.config.name.to_string(),
                                        None,
                                        None,
                                        None,
                                        ConfigFromStrOperation::Push,
                                    )
                                    .or_else(|_| {
                                        engine.try_set_from_str(
                                            value.config.name.to_string(),
                                            None,
                                            None,
                                            None,
                                            ConfigFromStrOperation::Push,
                                        )
                                    });
                            }

                            if ui.button("\u{f056} Pop").clicked() {
                                let _ = game
                                    .try_set_from_str(
                                        value.config.name.to_string(),
                                        None,
                                        None,
                                        None,
                                        ConfigFromStrOperation::Pop,
                                    )
                                    .or_else(|_| {
                                        engine.try_set_from_str(
                                            value.config.name.to_string(),
                                            None,
                                            None,
                                            None,
                                            ConfigFromStrOperation::Pop,
                                        )
                                    });
                            }
                        });
                    });
            }
            ConfigValue::JsonLikeRecord { .. } => {
                let val = get(
                    &value.config.name,
                    &mut pipe.user_data.config.game,
                    &mut pipe.user_data.config.engine,
                );
                CollapsingHeader::new("This is a json-like record of values")
                    .id_salt(format!("conf-val-record-{}", value.config.name))
                    .default_open(false)
                    .show(ui, |ui| {
                        let Ok(serde_json::Value::Object(record)) = serde_json::from_str(&val)
                        else {
                            return;
                        };
                        for (key, _) in record.into_iter() {
                            let modifier = key;

                            CollapsingHeader::new(&modifier)
                                .id_salt(format!(
                                    "conf-val-record-{}-{}",
                                    value.config.name, modifier
                                ))
                                .default_open(false)
                                .show(ui, |ui| {
                                    let values = keep_values
                                        .iter()
                                        .filter(|&v| v.config.name.starts_with(&value.config.name))
                                        .cloned()
                                        .collect();
                                    render_conf_val(
                                        ui,
                                        pipe,
                                        values,
                                        search,
                                        ModifierTy::Key(&modifier),
                                        &value.config.name,
                                    );
                                });

                            let game = &mut pipe.user_data.config.game;
                            let engine = &mut pipe.user_data.config.engine;
                            if ui.button("\u{f2ed} Delete").clicked() {
                                let _ = game
                                    .try_set_from_str(
                                        format!("{}[{}]", value.config.name, modifier),
                                        None,
                                        None,
                                        None,
                                        ConfigFromStrOperation::Rem,
                                    )
                                    .or_else(|_| {
                                        engine.try_set_from_str(
                                            format!("{}[{}]", value.config.name, modifier),
                                            None,
                                            None,
                                            None,
                                            ConfigFromStrOperation::Rem,
                                        )
                                    });
                            }
                        }
                        ui.horizontal(|ui| {
                            let game = &mut pipe.user_data.config.game;
                            let engine = &mut pipe.user_data.config.engine;

                            const RECORD_INSERT_STR: &str = "search-settings-insert-record-str";
                            let mut modifier = engine
                                .ui
                                .path
                                .query
                                .entry(RECORD_INSERT_STR.to_string())
                                .or_default()
                                .clone();

                            if modifier
                                .chars()
                                .next()
                                .is_some_and(|c| c.is_ascii_digit() || c == '-' || c == '+')
                            {
                                ui.vertical(|ui| {
                                    ui.text_edit_singleline(&mut modifier);
                                    ui.colored_label(
                                        Color32::RED,
                                        "record key must not start with an digit",
                                    );
                                });
                            } else {
                                ui.text_edit_singleline(&mut modifier);
                                if ui.button("\u{f055} Insert").clicked() {
                                    let _ = game
                                        .try_set_from_str(
                                            format!("{}[{}]", value.config.name, modifier),
                                            None,
                                            Some("".to_string()),
                                            None,
                                            ConfigFromStrOperation::Set,
                                        )
                                        .or_else(|_| {
                                            engine.try_set_from_str(
                                                format!("{}[{}]", value.config.name, modifier),
                                                None,
                                                Some("".to_string()),
                                                None,
                                                ConfigFromStrOperation::Set,
                                            )
                                        });
                                }
                            }

                            engine
                                .ui
                                .path
                                .query
                                .insert(RECORD_INSERT_STR.to_string(), modifier);
                        });
                    });
            }
            ConfigValue::Struct { .. } => {
                ui.label("This is an structure of values");
            }
        }
        ui.end_row();
    }
}

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>) {
    ui.label("Here is a searchable list of all config values.\nHover over the names to get a description.");

    let mut values = Vec::default();

    let val = ConfigGame::conf_value();
    parse_conf_values_as_str_list(
        "".to_string(),
        &mut |entry, val| {
            values.push(UiConfigVal {
                config: entry,
                val: val.clone(),
            });
        },
        val,
        "".into(),
        Default::default(),
    );
    let val = ConfigEngine::conf_value();
    parse_conf_values_as_str_list(
        "".to_string(),
        &mut |entry, val| {
            values.push(UiConfigVal {
                config: entry,
                val: val.clone(),
            });
        },
        val,
        "".into(),
        Default::default(),
    );

    const CONF_VAL_SEARCH: &str = "config-all-overview-search";
    let mut search: String = pipe.user_data.config.storage(CONF_VAL_SEARCH);

    ui.horizontal(|ui| {
        ui.label("\u{1f50d}");
        clearable_edit_field(ui, &mut search, Some(200.0), None);
    });

    ScrollArea::vertical()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .show(ui, |ui| {
            Grid::new("config-all-overview")
                .num_columns(2)
                .show(ui, |ui| {
                    render_conf_val(ui, pipe, values, &search, ModifierTy::None, "");
                });
        });

    pipe.user_data.config.set_storage(CONF_VAL_SEARCH, &search);
}
