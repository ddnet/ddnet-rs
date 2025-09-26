use config::config::ConfigPath;
use egui::{
    Color32, CornerRadius, FontId, Frame, Layout, Rect, RichText, ScrollArea, Shape, Style,
    UiBuilder, scroll_area::ScrollBarVisibility, vec2,
};
use egui_extras::{Size, StripBuilder};
use game_interface::types::{character_info::NetworkSkinInfo, render::character::TeeEye};
use graphics::handles::{
    canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle,
};
use graphics_types::{commands::TexFlags, types::GraphicsMemoryAllocationType};
use image_utils::png::load_png_image_as_rgba;
use math::math::vector::vec2;
use tracing::instrument;
use ui_base::{style::bg_frame_color, types::UiState};

use crate::{
    main_menu::{
        communities::CommunityIcon,
        constants::{
            MENU_COMMUNITY_PREFIX, MENU_EXPLORE_COMMUNITIES_NAME, MENU_FAVORITES_NAME,
            MENU_INTERNET_NAME, MENU_LAN_NAME, MENU_PROFILE_NAME, MENU_SETTINGS_NAME,
        },
        user_data::{PROFILE_SKIN_PREVIEW, ProfileSkin, UserData},
    },
    utils::{render_tee_for_ui, render_texture_for_ui},
};

fn update_communities(user_data: &mut UserData) {
    let communities = &user_data.ddnet_info.communities;

    communities.values().for_each(|c| {
        let icons = &mut *user_data.icons;
        let icon = icons.entry(c.id.clone()).or_insert_with(|| {
            let graphics_mt = user_data.graphics_mt.clone();
            let http = user_data.io.http.clone();
            let url = c.icon.url.clone();
            url.map(|url| {
                CommunityIcon::Loading(Ok(user_data.io.rt.spawn(async move {
                    let icon = http.download_binary_secure(url).await?.to_vec();

                    let mut img_mem = None;
                    let img = load_png_image_as_rgba(&icon, |width, height, _| {
                        img_mem = Some(graphics_mt.mem_alloc(
                            GraphicsMemoryAllocationType::TextureRgbaU8 {
                                width: width.try_into().unwrap(),
                                height: height.try_into().unwrap(),
                                flags: TexFlags::empty(),
                            },
                        ));
                        img_mem.as_mut().unwrap().as_mut_slice()
                    })?;

                    let width = img.width;
                    let height = img.height;
                    Ok((img_mem.unwrap(), width, height))
                })))
            })
            .unwrap_or_else(|| CommunityIcon::Loading(Err("icon url was None".to_string())))
        });

        match icon {
            CommunityIcon::Icon { .. } => {}
            CommunityIcon::Loading(task) => {
                if task.as_ref().is_ok_and(|task| task.is_finished()) {
                    let task = std::mem::replace(task, Err("loading failed.".to_string()));
                    let task = task.unwrap().get();
                    if let Ok((mem, width, height)) = task {
                        match user_data.texture_handle.load_texture_rgba_u8(mem, "icon") {
                            Ok(texture) => {
                                *icon = CommunityIcon::Icon {
                                    texture,
                                    width,
                                    height,
                                };
                            }
                            Err(err) => {
                                *icon = CommunityIcon::Loading(Err(err.to_string()));
                            }
                        }
                    }
                }
            }
        }
    });
}

enum CustomRender<'a> {
    None,
    Icon(&'a CommunityIcon),
    #[allow(clippy::type_complexity)]
    Custom(Box<dyn FnMut(&mut egui::Ui, &mut UiState, Rect, Rect) + 'a>),
}

#[instrument(level = "trace", skip_all)]
pub fn render(
    ui: &mut egui::Ui,
    user_data: &mut UserData,
    ui_state: &mut UiState,
    size: f32,
    ui_page_query_name: &str,
    fallback_query: &str,
) {
    ui_state.add_blur_rect(ui.available_rect_before_wrap(), 0.0);
    update_communities(user_data);

    let current_active = user_data
        .config
        .path()
        .query
        .get(ui_page_query_name)
        .cloned()
        .unwrap_or_else(|| fallback_query.to_string());
    fn btn_style(style: &mut Style, size: f32) {
        style.visuals.widgets.inactive.corner_radius = CornerRadius::same((size / 2.0) as u8);
        style.visuals.widgets.hovered.corner_radius = CornerRadius::same((size / 2.0) as u8);
        style.visuals.widgets.active.corner_radius = CornerRadius::same((size / 2.0) as u8);
    }
    fn round_btn(
        ui: &mut egui::Ui,
        text: &str,
        prefix: &str,
        icon: CustomRender<'_>,
        current_active: &str,
        size: f32,
        path: &mut ConfigPath,
        stream_handle: &GraphicsStreamHandle,
        canvas_handle: &GraphicsCanvasHandle,
        ui_state: &mut UiState,
        ui_page_query_name: &str,
    ) {
        let activate_text = format!("{prefix}{text}");
        let selected = activate_text.as_str() == current_active;
        ui.allocate_ui_with_layout(
            vec2(size, size),
            Layout::centered_and_justified(egui::Direction::BottomUp),
            |ui| {
                let mut rect = ui.available_rect_before_wrap();

                if selected {
                    let highlight_rect = rect
                        .translate(vec2(-(rect.width() / 2.0), 0.0))
                        .scale_from_center2(vec2(5.0 / size, 0.5));
                    ui.painter().add(Shape::rect_filled(
                        highlight_rect,
                        CornerRadius::same(4),
                        Color32::LIGHT_BLUE,
                    ));
                }

                const MARGIN: f32 = 5.0;
                rect.set_height(size - MARGIN * 2.0);
                rect.set_width(size - MARGIN * 2.0);
                rect = rect.translate(vec2(MARGIN, MARGIN));
                if ui
                    .scope_builder(UiBuilder::default().max_rect(rect), |ui| {
                        let clip_rect = ui.clip_rect();
                        let rect = ui.available_rect_before_wrap();
                        let style = ui.style_mut();
                        btn_style(style, size);
                        let text = match icon {
                            CustomRender::None => text,
                            CustomRender::Icon(icon) => {
                                if matches!(icon, CommunityIcon::Icon { .. }) {
                                    ""
                                } else {
                                    text
                                }
                            }
                            CustomRender::Custom(_) => "",
                        };
                        let clicked = ui
                            .button(RichText::new(text).font(FontId::proportional(18.0)))
                            .clicked();
                        match icon {
                            CustomRender::Icon(CommunityIcon::Icon {
                                texture,
                                width,
                                height,
                            }) => {
                                let (ratio_w, ratio_h) = if *width >= *height {
                                    (1.0, *width as f32 / *height as f32)
                                } else {
                                    (*height as f32 / *width as f32, 1.0)
                                };
                                render_texture_for_ui(
                                    stream_handle,
                                    canvas_handle,
                                    texture,
                                    ui,
                                    ui_state,
                                    ui.ctx().screen_rect(),
                                    Some(clip_rect),
                                    vec2::new(rect.center().x, rect.center().y),
                                    vec2::new(rect.width() / ratio_w, rect.height() / ratio_h),
                                    None,
                                );
                            }
                            CustomRender::Custom(mut render) => {
                                render(ui, ui_state, clip_rect, rect);
                            }
                            CustomRender::None | CustomRender::Icon(CommunityIcon::Loading(_)) => {
                                // ignore
                            }
                        }
                        clicked
                    })
                    .inner
                {
                    path.add_query((ui_page_query_name.to_string(), activate_text));
                }
            },
        );
    }
    let path = user_data.config.path();
    Frame::default().fill(bg_frame_color()).show(ui, |ui| {
        StripBuilder::new(ui)
            .size(Size::exact(40.0))
            .size(Size::remainder())
            .size(Size::exact(80.0))
            .vertical(|mut strip| {
                strip.cell(|ui| {
                    round_btn(
                        ui,
                        MENU_PROFILE_NAME,
                        "",
                        user_data
                            .profiles
                            .cur_profile()
                            .and_then(|profile| {
                                profile.user.get(PROFILE_SKIN_PREVIEW).and_then(|p| {
                                    serde_json::from_value::<ProfileSkin>(p.clone()).ok()
                                })
                            })
                            .as_ref()
                            .map(|profile| {
                                CustomRender::Custom(Box::new(|ui, ui_state, clip_rect, rect| {
                                    render_tee_for_ui(
                                        user_data.canvas_handle,
                                        user_data.skin_container,
                                        user_data.render_tee,
                                        ui,
                                        ui_state,
                                        ui.ctx().screen_rect(),
                                        Some(clip_rect),
                                        &profile.name.as_str().try_into().unwrap_or_default(),
                                        profile
                                            .color_body
                                            .zip(profile.color_feet)
                                            .map(|(body, feet)| NetworkSkinInfo::Custom {
                                                body_color: body,
                                                feet_color: feet,
                                            })
                                            .as_ref(),
                                        vec2::new(rect.center().x, rect.center().y),
                                        rect.width().min(rect.height()),
                                        TeeEye::Happy,
                                    );
                                }))
                            })
                            .unwrap_or(CustomRender::None),
                        &current_active,
                        size,
                        path,
                        user_data.stream_handle,
                        user_data.canvas_handle,
                        ui_state,
                        ui_page_query_name,
                    );
                });
                strip.cell(|ui| {
                    ScrollArea::vertical()
                        .scroll_bar_visibility(ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                            round_btn(
                                ui,
                                MENU_INTERNET_NAME,
                                "",
                                CustomRender::None,
                                &current_active,
                                size,
                                path,
                                user_data.stream_handle,
                                user_data.canvas_handle,
                                ui_state,
                                ui_page_query_name,
                            );
                            round_btn(
                                ui,
                                MENU_LAN_NAME,
                                "",
                                CustomRender::None,
                                &current_active,
                                size,
                                path,
                                user_data.stream_handle,
                                user_data.canvas_handle,
                                ui_state,
                                ui_page_query_name,
                            );
                            round_btn(
                                ui,
                                MENU_FAVORITES_NAME,
                                "",
                                CustomRender::None,
                                &current_active,
                                size,
                                path,
                                user_data.stream_handle,
                                user_data.canvas_handle,
                                ui_state,
                                ui_page_query_name,
                            );

                            for community in user_data.ddnet_info.communities.values() {
                                let icon = user_data.icons.get(&community.id);
                                round_btn(
                                    ui,
                                    &community.id,
                                    MENU_COMMUNITY_PREFIX,
                                    icon.map(CustomRender::Icon).unwrap_or(CustomRender::None),
                                    &current_active,
                                    size,
                                    path,
                                    user_data.stream_handle,
                                    user_data.canvas_handle,
                                    ui_state,
                                    ui_page_query_name,
                                );
                            }
                        });
                });
                strip.cell(|ui| {
                    round_btn(
                        ui,
                        MENU_EXPLORE_COMMUNITIES_NAME,
                        "",
                        CustomRender::None,
                        &current_active,
                        size,
                        path,
                        user_data.stream_handle,
                        user_data.canvas_handle,
                        ui_state,
                        ui_page_query_name,
                    );

                    round_btn(
                        ui,
                        MENU_SETTINGS_NAME,
                        "",
                        CustomRender::None,
                        &current_active,
                        size,
                        path,
                        user_data.stream_handle,
                        user_data.canvas_handle,
                        ui_state,
                        ui_page_query_name,
                    );
                });
            });
    });
}
