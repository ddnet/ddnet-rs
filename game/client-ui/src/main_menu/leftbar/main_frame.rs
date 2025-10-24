use base::{hash::decode_hash, reduced_ascii_str::ReducedAsciiString};
use config::config::ConfigPath;
use egui::{
    Color32, CornerRadius, FontId, Frame, Layout, Rect, RichText, ScrollArea, Shape, Style,
    UiBuilder, scroll_area::ScrollBarVisibility, vec2,
};
use egui_extras::{Size, StripBuilder};
use game_interface::types::{
    character_info::NetworkSkinInfo, render::character::TeeEye, resource_key::ResourceKey,
};
use graphics::handles::{
    canvas::canvas::GraphicsCanvasHandle, stream::stream::GraphicsStreamHandle,
};
use math::math::vector::vec2;
use tracing::instrument;
use ui_base::{style::bg_frame_color, types::UiState};

use crate::{
    main_menu::{
        communities::IconUrlHash,
        constants::{
            MENU_COMMUNITY_PREFIX, MENU_EXPLORE_COMMUNITIES_NAME, MENU_FAVORITES_NAME,
            MENU_INTERNET_NAME, MENU_LAN_NAME, MENU_PROFILE_NAME, MENU_SETTINGS_NAME,
        },
        user_data::{PROFILE_SKIN_PREVIEW, ProfileSkin, UserData},
    },
    thumbnail_container::Thumbnail,
    utils::{render_tee_for_ui, render_texture_for_ui},
};

enum CustomRender<'a> {
    None,
    Icon(&'a Thumbnail),
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
                            CustomRender::None => text.chars().next().unwrap_or('U').to_string(),
                            CustomRender::Icon(_) | CustomRender::Custom(_) => "".to_string(),
                        };
                        let clicked = ui
                            .button(RichText::new(text).font(FontId::proportional(18.0)))
                            .clicked();
                        match icon {
                            CustomRender::Icon(Thumbnail {
                                thumbnail,
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
                                    thumbnail,
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
                            CustomRender::None => {
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
                                let key = ResourceKey {
                                    name: ReducedAsciiString::from_str_lossy(&community.id),
                                    hash: if let IconUrlHash::Blake3 { blake3: hash } =
                                        &community.icon.hash
                                    {
                                        decode_hash(hash)
                                    } else {
                                        None
                                    },
                                };
                                let is_loaded = user_data.icons.is_loaded(&key);
                                let icon = user_data.icons.get_or_default(&key);
                                round_btn(
                                    ui,
                                    &community.id,
                                    MENU_COMMUNITY_PREFIX,
                                    if is_loaded {
                                        CustomRender::Icon(icon)
                                    } else {
                                        CustomRender::None
                                    },
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
