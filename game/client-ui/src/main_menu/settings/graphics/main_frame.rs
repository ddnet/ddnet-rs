use config::traits::ConfigValue;
use egui::{Button, Color32, DragValue, Grid, Id, Layout, Modal, ScrollArea, Stroke, TextEdit};
use egui_extras::{Size, StripBuilder};
use game_config::config::ConfigRender;
use graphics_types::gpu::{Gpu, GpuType};
use num_traits::FromPrimitive;
use tracing::instrument;
use ui_base::types::UiRenderPipe;

use crate::{events::UiEvent, main_menu::user_data::UserData};

fn render_settings(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>) {
    let config = &mut pipe.user_data.config.engine;
    let config_game = &mut pipe.user_data.config.game;
    let wnd = &mut config.wnd;

    Grid::new("gfx-settings").num_columns(2).show(ui, |ui| {
        ui.label("Window mode");
        egui::ComboBox::new("fullscreen_mode", "")
            .selected_text(if wnd.fullscreen {
                "fullscreen"
            } else if !wnd.fullscreen && wnd.maximized && !wnd.decorated {
                "borderless-fullscreen"
            } else {
                "windowed"
            })
            .show_ui(ui, |ui| {
                ui.vertical(|ui| {
                    if ui.add(egui::Button::new("fullscreen")).clicked() {
                        wnd.fullscreen = true;
                    }
                    if ui.add(egui::Button::new("borderless-fullscreen")).clicked() {
                        wnd.fullscreen = false;
                        wnd.decorated = false;
                        wnd.maximized = true;
                    }
                    if ui.add(egui::Button::new("windowed")).clicked() {
                        wnd.fullscreen = false;
                        wnd.decorated = true;
                    }
                })
            });
        ui.end_row();

        ui.label("Monitor");
        egui::ComboBox::new("monitor_select", "")
            .selected_text(&wnd.monitor.name)
            .show_ui(ui, |ui| {
                ui.vertical(|ui| {
                    for monitor in pipe.user_data.monitors.monitors().iter() {
                        if ui.add(egui::Button::new(&monitor.name)).clicked() {
                            wnd.monitor.name = monitor.name.clone();
                            if let Some(mode) = monitor.video_modes.first() {
                                wnd.monitor.width = mode.width;
                                wnd.monitor.height = mode.height;
                            }
                        }
                    }
                })
            });
        ui.end_row();

        ui.label("V-sync");
        if ui.checkbox(&mut config.gl.vsync, "").changed() {
            pipe.user_data.events.push(UiEvent::VsyncChanged);
        }
        ui.end_row();

        let gpus = pipe.user_data.backend_handle.gpus();
        ui.label("Msaa");
        let mut msaa_step = (config.gl.msaa_samples as f64).log2() as u32;
        let max_step = (gpus.cur.msaa_sampling_count as f64).log2() as u32;
        if ui
            .add(
                DragValue::new(&mut msaa_step)
                    .update_while_editing(false)
                    .range(0..=max_step)
                    .custom_formatter(|v, _| {
                        let samples = 2_u32.pow(v as u32);
                        if samples == 1 {
                            "off".to_string()
                        } else {
                            format!("{samples}")
                        }
                    }),
            )
            .changed()
        {
            config.gl.msaa_samples = 2_u32.pow(msaa_step);
            pipe.user_data.events.push(UiEvent::MsaaChanged);
        }
        ui.end_row();

        ui.label("Graphics card");
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        let auto_gpu_display_str = format!("auto({})", gpus.auto.name);
        egui::ComboBox::new("gpu_select", "")
            .selected_text(if config.gl.gpu == "auto" {
                &auto_gpu_display_str
            } else {
                &config.gl.gpu
            })
            .show_ui(ui, |ui| {
                ui.vertical(|ui| {
                    let gpu_list = [
                        vec![Gpu {
                            name: "auto".to_string(),
                            ty: GpuType::Invalid,
                        }],
                        gpus.gpus.clone(),
                    ]
                    .concat();
                    for gpu in gpu_list {
                        if ui
                            .add(
                                egui::Button::new(if gpu.name == "auto" {
                                    &auto_gpu_display_str
                                } else {
                                    &gpu.name
                                })
                                .selected(gpu.name == config.gl.gpu)
                                .stroke(
                                    if gpu.name == gpus.cur.name {
                                        Stroke::new(2.0, Color32::LIGHT_GREEN)
                                    } else {
                                        Stroke::NONE
                                    },
                                ),
                            )
                            .clicked()
                        {
                            config.gl.gpu = gpu.name;
                        }
                    }
                })
            });
        ui.style_mut().wrap_mode = None;
        ui.end_row();

        ui.label("Ingame aspect ratio");
        ui.checkbox(&mut config_game.cl.render.use_ingame_aspect_ratio, "");
        ui.end_row();

        if config_game.cl.render.use_ingame_aspect_ratio {
            let aspect_ratio = config_game.cl.render.ingame_aspect_ratio;
            let conf_values = ConfigRender::conf_values_structured();
            let ratio = num_rational::Rational64::from_f64(aspect_ratio);

            ui.label("");
            if let Some(ratio) = ratio {
                ui.label(format!("{} / {}", ratio.numer(), ratio.denom()));
            } else {
                ui.label(format!("{aspect_ratio}"));
            }
            ui.end_row();

            ui.label("");
            ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
                if let Some(ratio) = ratio {
                    let numer = config
                        .ui
                        .path
                        .query
                        .entry("ingame-aspect-numer".to_string())
                        .or_insert_with(|| ratio.numer().to_string());
                    ui.add_sized(egui::vec2(25.0, 20.0), TextEdit::singleline(numer));
                    let numer = numer.clone();

                    ui.label("/");

                    let denom = config
                        .ui
                        .path
                        .query
                        .entry("ingame-aspect-denom".to_string())
                        .or_insert_with(|| ratio.denom().to_string());
                    ui.add_sized(egui::vec2(25.0, 20.0), TextEdit::singleline(denom));
                    let denom = denom.clone();

                    let parsed_num_denom =
                        numer.parse::<u128>().ok().zip(denom.parse::<u128>().ok());

                    let enabled = if let Some((numer, denom)) = parsed_num_denom {
                        config_game.cl.render.ingame_aspect_ratio != (numer as f64 / denom as f64)
                    } else {
                        false
                    };

                    let clicked = ui.add_enabled(enabled, Button::new("\u{f00c}")).clicked();
                    if let Some((numer, denom)) = clicked.then_some(parsed_num_denom).flatten() {
                        if let ConfigValue::Float { min, max } = conf_values.ingame_aspect_ratio.val
                        {
                            config_game.cl.render.ingame_aspect_ratio =
                                (numer as f64 / denom as f64).clamp(min, max);
                        } else {
                            log::warn!("ingame aspect ratio suddenly isn't a float anymore");
                        }
                    }
                } else {
                    let mut ratio = aspect_ratio.to_string();
                    ui.text_edit_singleline(&mut ratio);
                    if let Ok(aspect_ratio) = ratio.parse::<f64>() {
                        config_game.cl.render.ingame_aspect_ratio = aspect_ratio;
                    }
                }
            });
            ui.end_row();
        }
    });
}

fn render_monitors(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>) {
    let config = &mut pipe.user_data.config;
    let wnd = &mut config.engine.wnd;

    const SHOW_INFO_ASPECT: &str = "show-info-ingame-aspect";

    if let Some(monitors) = pipe
        .user_data
        .monitors
        .monitors()
        .iter()
        .find(|monitor| monitor.name == wnd.monitor.name)
    {
        ui.with_layout(
            Layout::top_down(egui::Align::Min).with_cross_justify(true),
            |ui| {
                let wnd = &mut config.engine.wnd;
                fn fmt_res(w: u32, h: u32, refresh_rate_mhz: u32) -> String {
                    let g = gcd::binary_u32(w, h).max(1);
                    format!(
                        "{}x{} @{:0.2} ({}:{})",
                        w,
                        h,
                        refresh_rate_mhz as f64 / 1000.0,
                        w / g,
                        h / g
                    )
                }

                ui.label(format!(
                    "Monitor: {} - {}",
                    monitors.name,
                    if wnd.fullscreen {
                        fmt_res(
                            wnd.fullscreen_width,
                            wnd.fullscreen_height,
                            wnd.refresh_rate_mhz,
                        )
                    } else {
                        fmt_res(
                            wnd.window_width as u32,
                            wnd.window_height as u32,
                            wnd.refresh_rate_mhz,
                        )
                    }
                ));
                ui.style_mut().spacing.scroll.floating = false;
                ScrollArea::vertical().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let style = ui.style_mut();
                    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_black_alpha(50);
                    style.visuals.widgets.active.weak_bg_fill = Color32::from_black_alpha(50);
                    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_black_alpha(50);
                    for mode in &monitors.video_modes {
                        let wnd = &mut config.engine.wnd;
                        if ui
                            .add(
                                Button::new(fmt_res(
                                    mode.width,
                                    mode.height,
                                    mode.refresh_rate_mhz,
                                ))
                                .selected(
                                    wnd.fullscreen_width == mode.width
                                        && wnd.fullscreen_height == mode.height
                                        && wnd.refresh_rate_mhz == mode.refresh_rate_mhz,
                                ),
                            )
                            .clicked()
                        {
                            const INFO_NAME: &str = "info-ingame-aspect";
                            let mut ingame_aspect_info: bool = config.storage(INFO_NAME);
                            let wnd = &mut config.engine.wnd;
                            if ingame_aspect_info || !wnd.fullscreen {
                                if wnd.fullscreen {
                                    wnd.fullscreen_width = mode.width;
                                    wnd.fullscreen_height = mode.height;
                                } else {
                                    wnd.window_width = mode.width as f64;
                                    wnd.window_height = mode.height as f64;
                                }
                                wnd.refresh_rate_mhz = mode.refresh_rate_mhz;
                            } else {
                                config
                                    .path()
                                    .add_query((SHOW_INFO_ASPECT.into(), "1".into()));
                                ingame_aspect_info = true;
                            }
                            config.set_storage(INFO_NAME, &ingame_aspect_info);
                        }
                    }
                });
            },
        );
    }

    if pipe
        .user_data
        .config
        .path()
        .query
        .contains_key(SHOW_INFO_ASPECT)
    {
        let mut window = true;
        if Modal::new(Id::new("settings-ingame-aspect-modal"))
            .show(ui.ctx(), |ui| {
                ui.heading("\u{f05a} Use ingame aspect ratio!");

                ui.label(
                    "If you want to play with a custom streetched resolution,\n\
                    then try \"Ingame aspect ratio\" on the left instead.\n\
                    It does not mess with the graphics driver or GUI and works on any resolution",
                );

                if ui.button("Ok").clicked() {
                    window = false;
                }
            })
            .should_close()
        {
            window = false;
        }

        if !window {
            pipe.user_data.config.path().query.remove(SHOW_INFO_ASPECT);
        }
    }
}

#[instrument(level = "trace", skip_all)]
pub fn render(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>) {
    let wnd_old = pipe.user_data.config.engine.wnd.clone();

    ui.with_layout(Layout::top_down(egui::Align::Min), |ui| {
        StripBuilder::new(ui)
            .size(Size::remainder())
            .size(Size::exact(300.0))
            .horizontal(|mut strip| {
                strip.cell(|ui| {
                    ui.style_mut().wrap_mode = None;
                    render_settings(ui, pipe);
                });
                strip.cell(|ui| {
                    ui.style_mut().wrap_mode = None;
                    ui.set_width(ui.available_width());
                    ui.set_height(ui.available_height());
                    render_monitors(ui, pipe);
                });
            });
    });

    if wnd_old != pipe.user_data.config.engine.wnd {
        pipe.user_data.events.push(UiEvent::WindowChange);
    }
}
