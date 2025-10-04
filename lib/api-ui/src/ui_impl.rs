use std::{cell::RefCell, time::Duration};

use api::{GRAPHICS, GRAPHICS_BACKEND, read_param_from_host, upload_return_val};

use graphics_types::types::WindowProps;
use ui_base::{
    types::{BlurShape, GlassShape, RawInputWrapper, RawOutputWrapper, UiFonts, UiRenderPipe},
    ui::UiContainer,
    ui_render::render_ui,
};
use ui_generic::traits::UiPageInterface;

unsafe extern "Rust" {
    /// returns an instance of the game state and the game tick speed
    fn mod_ui_new() -> Box<dyn UiPageInterface<()>>;
}

type U = ();

thread_local! {
static API_UI: once_cell::unsync::Lazy<RefCell<UiContainer>> =
    once_cell::unsync::Lazy::new(|| RefCell::new(UiContainer::new(&Default::default())));

static API_UI_USER: once_cell::unsync::Lazy<RefCell<Box<dyn UiPageInterface<U>>>> =
    once_cell::unsync::Lazy::new(|| RefCell::new(unsafe { mod_ui_new() }));
}

#[unsafe(no_mangle)]
pub fn ui_new() {
    let fonts = read_param_from_host::<UiFonts>(0);
    API_UI.with(|g| {
        let mut ui = g.borrow_mut();
        if let Some(font_definitions) = fonts.fonts.as_ref() {
            ui.context.egui_ctx.set_fonts(font_definitions.clone());
        }
        ui.font_definitions = fonts.fonts;
    });
}

/// returns platform output and zoom level
fn ui_run_impl(
    cur_time: Duration,
    window_props: WindowProps,
    inp: RawInputWrapper,
    zoom_level: Option<f32>,
    mut user_data: U,
) -> (egui::PlatformOutput, Vec<BlurShape>, Vec<GlassShape>) {
    API_UI.with(|g| g.borrow_mut().zoom_level.set(zoom_level));
    GRAPHICS.with(|g| g.resized(window_props));

    let (screen_rect, full_output, zoom_level) = API_UI.with(|g| {
        g.borrow_mut().render(
            GRAPHICS.with(|g| g.canvas_handle.canvas_width()),
            GRAPHICS.with(|g| g.canvas_handle.canvas_height()),
            GRAPHICS.with(|g| g.canvas_handle.pixels_per_point()),
            |ui, pipe, ui_state| {
                API_UI_USER.with(|g| g.borrow_mut().render(ui, pipe, ui_state));
            },
            &mut UiRenderPipe::new(cur_time, &mut user_data),
            inp.input,
            false,
        )
    });

    let platform_output = {
        let graphics = GRAPHICS.with(|g| (*g).clone());
        API_UI.with(|g| {
            render_ui(
                &mut g.borrow_mut(),
                full_output,
                &screen_rect,
                zoom_level,
                &graphics.backend_handle,
                &graphics.texture_handle,
                &graphics.stream_handle,
                false,
            )
        })
    };

    GRAPHICS_BACKEND.with(|g| g.actual_run_cmds.set(false));
    GRAPHICS.with(|g| {
        g.backend_handle
            .run_backend_buffer(g.stream_handle.stream_data())
    });
    GRAPHICS_BACKEND.with(|g| g.actual_run_cmds.set(true));

    (
        platform_output,
        API_UI.with(|ui| std::mem::take(&mut ui.borrow_mut().ui_state.blur_shapes)),
        API_UI.with(|ui| std::mem::take(&mut ui.borrow_mut().ui_state.glass_shapes)),
    )
}

#[unsafe(no_mangle)]
pub fn ui_mount() {
    API_UI_USER.with(|g| g.borrow_mut().mount());
}

#[unsafe(no_mangle)]
pub fn ui_unmount() {
    API_UI_USER.with(|g| g.borrow_mut().unmount());
}

#[unsafe(no_mangle)]
pub fn ui_run() {
    let cur_time = read_param_from_host::<Duration>(0);
    let window_props = read_param_from_host::<WindowProps>(1);
    let inp = read_param_from_host::<RawInputWrapper>(2);
    let zoom_level = read_param_from_host::<Option<f32>>(3);

    let (output, blur_shapes, glass_shapes) =
        ui_run_impl(cur_time, window_props, inp, zoom_level, ());
    upload_return_val(RawOutputWrapper {
        output,
        blur_shapes,
        glass_shapes,
        zoom_level: API_UI.with(|g| g.borrow().zoom_level.get()),
    });
}
