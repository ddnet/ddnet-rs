use std::{collections::hash_map::Entry, time::Duration};

use base::hash::fmt_hash;
use egui::{Color32, ComboBox, CornerRadius, Grid, Label, Layout, ScrollArea, Slider};
use game_config::config::{ConfigSpatialChat, ConfigSpatialChatPerPlayerOptions};
use game_interface::types::player_info::PlayerUniqueId;
use ui_base::types::UiRenderPipe;

use crate::main_menu::{
    settings::sound::utils::{db_to_ratio, ratio_to_db},
    spatial_chat::SpatialChatEntity,
    user_data::UserData,
};

const EXPLAIN_SPATIAL_RISKS: &str = "\
Thanks for testing out spatial voice chat.  \n\
Before you continue, here are some important\n\
things that you need to know:\n\
\n\
- Spatial chat only works on servers that enable support for it\n\
    This is because spatial chat generates lots of extra network traffic.\n\
    **Don't annoy server owners about if they activate it.**\n\
- Spatial chat is opt-in by design. Moderation of the spatial chat\n\
    is almost impossible. **The server hosters are not responsible for\n\
    moderation.** You can mute annoying players client side.\n\
- Spatial chat sends microphone data to the server, by enabling this\n\
    you accept potential abuse of your voice data. **Use at your own risk!**\n\
- You are over 13+ years old.\n\
\n\
If you still want to try out spatial chat write  \n\
`I read and understand the warnings about spatial chat`  \n\
in the following text box:\
";

const EXPLAIN_NON_ACCOUNT_USERS: &str = "\
Non-account users cannot be **permanently** muted,
only at best effort.
";

pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>) {
    ui.with_layout(Layout::top_down(egui::Align::Min), |ui| {
        let config = &mut pipe.user_data.config.game;
        let path = &mut pipe.user_data.config.engine.ui.path;

        if !config.cl.spatial_chat.read_warning {
            let mut cache = egui_commonmark::CommonMarkCache::default();
            egui_commonmark::CommonMarkViewer::new().show(ui, &mut cache, EXPLAIN_SPATIAL_RISKS);

            let text = path
                .query
                .entry("spatial-chat-warning".to_string())
                .or_default();
            ui.with_layout(
                Layout::left_to_right(egui::Align::Min).with_main_justify(true),
                |ui| {
                    ui.text_edit_singleline(text);
                },
            );

            if text == "I read and understand the warnings about spatial chat" {
                config.cl.spatial_chat.read_warning = true;
                config.cl.spatial_chat.activated = true;
            }
        } else {
            let chat = &pipe.user_data.spatial_chat;
            ui.label("Settings for the microphone for the spatial voice chat.");
            ui.label(format!(
                "Your current server {}",
                if chat.get_support() {
                    "supports spatial voice chat."
                } else {
                    "does not support spatial voice chat."
                }
            ));

            if ui
                .checkbox(
                    &mut config.cl.spatial_chat.activated,
                    "Activate spatial chat support",
                )
                .clicked()
            {
                chat.set_changed();
            }
            if !config.cl.spatial_chat.activated {
                return;
            }

            ui.label("You should hear yourself now, if not select a different audio device");
            chat.set_active();

            let settings = &mut config.cl.spatial_chat;
            let old_settings = settings.clone();

            let hosts = chat.get_hosts();

            let loudest_report = chat.get_loudest() as f64;

            let min_sound_db = -70.0;
            let max_sound_db = 0.0;
            let min_sound = db_to_ratio(min_sound_db);
            let max_sound = db_to_ratio(max_sound_db);
            let loudest_db_real = ratio_to_db(loudest_report.clamp(0.00001, f64::MAX));
            let loudest_db = ratio_to_db(loudest_report.clamp(min_sound, max_sound));
            let loudest_ratio = (loudest_db - min_sound_db) / (max_sound_db - min_sound_db);

            let gate_open_db = settings
                .filter
                .noise_gate
                .open_threshold
                .clamp(min_sound_db, max_sound_db);
            let gate_open_ratio = (gate_open_db - min_sound_db) / (max_sound_db - min_sound_db);

            let gate_close_db = settings
                .filter
                .noise_gate
                .close_threshold
                .clamp(min_sound_db, max_sound_db);
            let gate_close_ratio = (gate_close_db - min_sound_db) / (max_sound_db - min_sound_db);

            Grid::new("spatial-select-grid")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Sound drivers:");
                    ComboBox::new("spatial-host-select", "")
                        .selected_text(if settings.host.is_empty() {
                            "auto"
                        } else {
                            &settings.host
                        })
                        .show_ui(ui, |ui| {
                            for (host, devices) in &hosts.hosts {
                                if ui.button(host).clicked() {
                                    settings.host = host.to_string();
                                    settings.device = devices.default.clone().unwrap_or_default();
                                }
                            }
                        });
                    ui.end_row();

                    ui.label("Sound cards:");
                    let cur_host = hosts
                        .hosts
                        .get(&settings.host)
                        .or_else(|| hosts.hosts.get(&hosts.default));
                    ComboBox::new("spatial-device-select", "")
                        .selected_text(if settings.device.is_empty() {
                            "auto".to_string()
                        } else {
                            cur_host
                                .and_then(|host| {
                                    host.devices
                                        .iter()
                                        .find(|dev| dev.as_str() == settings.device)
                                        .or(host.default.as_ref())
                                })
                                .cloned()
                                .unwrap_or_default()
                        })
                        .show_ui(ui, |ui| {
                            let Some(cur_host) = cur_host else { return };
                            for device in &cur_host.devices {
                                if ui.button(device).clicked() {
                                    settings.device = device.clone();
                                }
                            }
                        });
                    ui.end_row();

                    ui.label("Stereo");
                    ui.checkbox(&mut settings.spatial, "");
                    ui.end_row();

                    ui.add(Label::new("Sound from non-account users"))
                        .on_hover_ui(|ui| {
                            let mut cache = egui_commonmark::CommonMarkCache::default();
                            egui_commonmark::CommonMarkViewer::new().show(
                                ui,
                                &mut cache,
                                EXPLAIN_NON_ACCOUNT_USERS,
                            );
                        });
                    ui.checkbox(&mut settings.from_non_account_users, "");
                    ui.end_row();

                    ui.label("Noise filter");
                    ui.end_row();

                    ui.label("Use noise filter");
                    ui.checkbox(&mut settings.filter.use_nf, "");
                    ui.end_row();

                    let mut attenuation_slider = chat.get_attenuation_slider();
                    let mut processing_threshold_slider = chat.get_processing_threshold_slider();
                    let mut boost_slider = chat.get_boost_slider();

                    let cur_time = pipe.cur_time;

                    attenuation_slider.val = if attenuation_slider.changed_at != Duration::MAX
                        && cur_time.saturating_sub(attenuation_slider.changed_at)
                            < Duration::from_secs(2)
                    {
                        attenuation_slider.val
                    } else {
                        settings.filter.nf.attenuation
                    };
                    processing_threshold_slider.val = if processing_threshold_slider.changed_at
                        != Duration::MAX
                        && cur_time.saturating_sub(processing_threshold_slider.changed_at)
                            < Duration::from_secs(2)
                    {
                        processing_threshold_slider.val
                    } else {
                        settings.filter.nf.processing_threshold
                    };
                    boost_slider.val = if boost_slider.changed_at != Duration::MAX
                        && cur_time.saturating_sub(boost_slider.changed_at) < Duration::from_secs(2)
                    {
                        boost_slider.val
                    } else {
                        settings.filter.boost
                    };

                    ui.label("Attenuation in db");
                    if ui
                        .add(Slider::new(&mut attenuation_slider.val, 0.0..=100.0))
                        .changed()
                    {
                        attenuation_slider.changed_at = cur_time;
                    }
                    ui.end_row();

                    ui.label("Processing threshold in db");
                    if ui
                        .add(Slider::new(
                            &mut processing_threshold_slider.val,
                            -15.0..=35.0,
                        ))
                        .changed()
                    {
                        processing_threshold_slider.changed_at = cur_time;
                    }
                    ui.end_row();

                    ui.label("Microphone boost in db");
                    if ui
                        .add(Slider::new(&mut boost_slider.val, -35.0..=35.0))
                        .changed()
                    {
                        boost_slider.changed_at = cur_time;
                    }
                    ui.end_row();

                    if attenuation_slider.changed_at != Duration::MAX
                        && cur_time.saturating_sub(attenuation_slider.changed_at)
                            >= Duration::from_secs(1)
                    {
                        attenuation_slider.changed_at = Duration::MAX;
                        settings.filter.nf.attenuation = attenuation_slider.val;
                    }
                    if processing_threshold_slider.changed_at != Duration::MAX
                        && cur_time.saturating_sub(processing_threshold_slider.changed_at)
                            >= Duration::from_secs(1)
                    {
                        processing_threshold_slider.changed_at = Duration::MAX;
                        settings.filter.nf.processing_threshold = processing_threshold_slider.val;
                    }
                    if boost_slider.changed_at != Duration::MAX
                        && cur_time.saturating_sub(boost_slider.changed_at)
                            >= Duration::from_secs(1)
                    {
                        boost_slider.changed_at = Duration::MAX;
                        settings.filter.boost = boost_slider.val;
                    }

                    chat.set_attenuation_slider(attenuation_slider);
                    chat.set_processing_threshold_slider(processing_threshold_slider);
                    chat.set_boost_slider(boost_slider);

                    ui.label("Noise gate:");
                    ui.label(format!(
                        "{:.2} / {:.2}% / {}",
                        loudest_report,
                        loudest_ratio * 100.0,
                        if loudest_report < 0.00001 {
                            "silent".to_string()
                        } else {
                            format!("{} db", loudest_db_real)
                        }
                    ));
                    ui.end_row();
                });
            Grid::new("spatial-noise-gate-grid")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("");
                    let mut rect = ui.available_rect_before_wrap();
                    rect.set_height(15.0);
                    let mut ui_rect = rect;
                    ui.painter()
                        .rect_filled(ui_rect, CornerRadius::default(), Color32::GREEN);
                    ui_rect.set_width(ui_rect.width() * gate_open_ratio as f32);
                    ui.painter()
                        .rect_filled(ui_rect, CornerRadius::default(), Color32::YELLOW);
                    let mut ui_rect = rect;
                    ui_rect.set_width(ui_rect.width() * gate_close_ratio as f32);
                    ui.painter()
                        .rect_filled(ui_rect, CornerRadius::default(), Color32::RED);
                    let mut ui_rect = rect;
                    ui_rect.set_width(ui_rect.width() * loudest_ratio as f32);
                    ui.painter().rect_filled(
                        ui_rect,
                        CornerRadius::default(),
                        Color32::from_white_alpha(150),
                    );
                    ui.end_row();

                    let mut gate_open_slider = chat.get_gate_open_slider();
                    let mut gate_close_slider = chat.get_gate_close_slider();

                    let cur_time = pipe.cur_time;

                    gate_open_slider.val = if gate_open_slider.changed_at != Duration::MAX
                        && cur_time.saturating_sub(gate_open_slider.changed_at)
                            < Duration::from_secs(2)
                    {
                        gate_open_slider.val
                    } else {
                        gate_open_db
                    };
                    gate_close_slider.val = if gate_close_slider.changed_at != Duration::MAX
                        && cur_time.saturating_sub(gate_close_slider.changed_at)
                            < Duration::from_secs(2)
                    {
                        gate_close_slider.val
                    } else {
                        gate_close_db
                    };

                    ui.label("Gate open:");
                    ui.with_layout(
                        Layout::left_to_right(egui::Align::Min).with_main_justify(true),
                        |ui| {
                            ui.style_mut().spacing.slider_width = ui.available_width();
                            if ui
                                .add(
                                    Slider::new(
                                        &mut gate_open_slider.val,
                                        min_sound_db..=max_sound_db,
                                    )
                                    .show_value(false),
                                )
                                .changed()
                            {
                                gate_open_slider.changed_at = pipe.cur_time;
                            }
                        },
                    );
                    ui.end_row();

                    ui.label("Gate close:");
                    ui.with_layout(
                        Layout::left_to_right(egui::Align::Min).with_main_justify(true),
                        |ui| {
                            ui.style_mut().spacing.slider_width = ui.available_width();
                            if ui
                                .add(
                                    Slider::new(
                                        &mut gate_close_slider.val,
                                        min_sound_db..=max_sound_db,
                                    )
                                    .show_value(false),
                                )
                                .changed()
                            {
                                gate_close_slider.changed_at = pipe.cur_time;
                            }
                        },
                    );
                    ui.end_row();

                    if gate_open_slider.changed_at != Duration::MAX
                        && cur_time.saturating_sub(gate_open_slider.changed_at)
                            >= Duration::from_secs(1)
                    {
                        gate_open_slider.changed_at = Duration::MAX;
                        settings.filter.noise_gate.open_threshold = gate_open_slider.val;
                    }
                    if gate_close_slider.changed_at != Duration::MAX
                        && cur_time.saturating_sub(gate_close_slider.changed_at)
                            >= Duration::from_secs(1)
                    {
                        gate_close_slider.changed_at = Duration::MAX;
                        settings.filter.noise_gate.close_threshold = gate_close_slider.val;
                    }

                    chat.set_gate_open_slider(gate_open_slider);
                    chat.set_gate_close_slider(gate_close_slider);
                });

            if old_settings != *settings {
                chat.set_changed();
            }

            // player list
            ScrollArea::vertical().show(ui, |ui| {
                Grid::new("player-list-grid").num_columns(2).show(ui, |ui| {
                    let entities = chat.get_entities();

                    for player in entities.into_values() {
                        ui.label(format!(
                            "{} {}",
                            if matches!(player.unique_id, PlayerUniqueId::Account(_)) {
                                "\u{f007}"
                            } else {
                                ""
                            },
                            &player.name,
                        ));
                        ui.horizontal(|ui| {
                            fn conf_player<'a>(
                                settings: &'a mut ConfigSpatialChat,
                                player: &SpatialChatEntity,
                            ) -> Entry<'a, String, ConfigSpatialChatPerPlayerOptions>
                            {
                                let acc_players = &mut settings.account_players;
                                let acc_certs = &mut settings.account_certs;
                                match player.unique_id {
                                    PlayerUniqueId::Account(account_id) => {
                                        acc_players.entry(format!("acc_{}", account_id))
                                    }
                                    PlayerUniqueId::CertFingerprint(hash) => {
                                        acc_certs.entry(format!("cert_{}", fmt_hash(&hash)))
                                    }
                                }
                            }
                            fn map<T>(
                                conf_player: &Entry<'_, String, ConfigSpatialChatPerPlayerOptions>,
                                f: impl FnOnce(&ConfigSpatialChatPerPlayerOptions) -> T,
                            ) -> Option<T> {
                                match conf_player {
                                    Entry::Occupied(e) => Some(f(e.get())),
                                    Entry::Vacant(_) => None,
                                }
                            }
                            let entry = conf_player(settings, &player);
                            let mut muted = map(&entry, |p| p.muted).unwrap_or_default();
                            if ui.checkbox(&mut muted, "muted").changed() {
                                let entry = entry.or_default();
                                entry.muted = muted;
                            }

                            let entry = conf_player(settings, &player);
                            let mut boost = map(&entry, |p| p.boost).unwrap_or_default();
                            if ui.add(Slider::new(&mut boost, -35.0..=35.0)).changed() {
                                let entry = entry.or_default();
                                entry.boost = boost;
                            };

                            if ui.button("\u{f0e2}").clicked() {
                                let acc_players = &mut settings.account_players;
                                let acc_certs = &mut settings.account_certs;
                                match player.unique_id {
                                    PlayerUniqueId::Account(account_id) => {
                                        acc_players.remove(&format!("acc_{}", account_id));
                                    }
                                    PlayerUniqueId::CertFingerprint(hash) => {
                                        acc_certs.remove(&format!("cert_{}", fmt_hash(&hash)));
                                    }
                                }
                            }
                        });
                        ui.end_row();
                    }
                });
            });
        }
    });
}
