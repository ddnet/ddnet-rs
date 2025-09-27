use egui_extras::{Size, StripBuilder};

use tracing::instrument;
use ui_base::types::{UiRenderPipe, UiState};

use super::{
    constants::{MENU_INTERNET_NAME, MENU_UI_PAGE_QUERY},
    user_data::UserData,
};

fn render_content_impl(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    ui_page_query_name: &str,
) {
    let cur_page = pipe
        .user_data
        .config
        .engine
        .ui
        .path
        .query
        .get(ui_page_query_name)
        .map(|path| path.as_ref())
        .unwrap_or("")
        .to_string();
    super::content::main_frame::render(ui, pipe, ui_state, &cur_page);
    super::settings::main_frame::render(ui, pipe, ui_state, &cur_page);
    super::demo::main_frame::render(ui, ui_state, pipe, &cur_page);
    super::profile::main_frame::render(
        ui,
        pipe,
        ui_state,
        &cur_page,
        ui_page_query_name != MENU_UI_PAGE_QUERY,
    );
}

/// big square, rounded edges
pub fn render_left_bar_and_content<'a, U, F>(
    ui: &mut egui::Ui,
    pipe: &'a mut UiRenderPipe<'a, U>,
    ui_state: &mut UiState,
    ui_page_query_name: &str,
    fallback_query: &str,
    content: F,
) where
    F: FnOnce(&mut egui::Ui, &mut UiRenderPipe<U>, &mut UiState, &str),
    U: AsMut<UserData<'a>>,
{
    let style = ui.style_mut();
    let x = style.spacing.item_spacing.x;
    style.spacing.item_spacing.x = 0.0;
    const LEFT_BAR_WIDTH: f32 = 40.0;
    StripBuilder::new(ui)
        .size(Size::exact(LEFT_BAR_WIDTH))
        .size(Size::remainder())
        .horizontal(|mut strip| {
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                ui.style_mut().spacing.item_spacing.x = x;
                super::leftbar::main_frame::render(
                    ui,
                    pipe.user_data.as_mut(),
                    ui_state,
                    LEFT_BAR_WIDTH,
                    ui_page_query_name,
                    fallback_query,
                );
            });
            strip.cell(|ui| {
                ui.style_mut().spacing.item_spacing.x = x;
                content(ui, pipe, ui_state, ui_page_query_name);
            });
        });
}

pub fn render_content(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    ui_page_query_name: &str,
) {
    StripBuilder::new(ui)
        .size(Size::exact(20.0))
        .size(Size::exact(10.0))
        .size(Size::remainder())
        .size(Size::exact(10.0))
        .vertical(|mut strip| {
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                super::topbar::main_frame::render(ui, ui_state, pipe, ui_page_query_name);
            });
            strip.empty();
            strip.strip(|builder| {
                builder
                    .size(Size::exact(10.0))
                    .size(Size::remainder())
                    .size(Size::exact(10.0))
                    .horizontal(|mut strip| {
                        strip.empty();
                        strip.cell(|ui| {
                            ui.style_mut().wrap_mode = None;
                            render_content_impl(ui, pipe, ui_state, ui_page_query_name);
                        });
                        strip.empty();
                    });
            });
            strip.empty();
        });
}

#[instrument(level = "trace", skip_all)]
pub fn render<'a>(
    ui: &mut egui::Ui,
    pipe: &'a mut UiRenderPipe<'a, UserData<'a>>,
    ui_state: &mut UiState,
) {
    render_left_bar_and_content(
        ui,
        pipe,
        ui_state,
        MENU_UI_PAGE_QUERY,
        MENU_INTERNET_NAME,
        render_content,
    );
}
