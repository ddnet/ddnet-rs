use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::{atomic::AtomicBool, Arc, RwLock},
    time::Duration,
};

use egui::{Color32, FontDefinitions, Modifiers, TextureId, Vec2, ViewportId};
use graphics::handles::texture::texture::TextureContainer;
use hiarc::Hiarc;

use crate::{custom_callback::CustomCallbackTrait, style::default_style};

use super::types::{UiRenderPipe, UiState};

pub fn gui_main_panel(main_panel_color: &Color32) -> egui::CentralPanel {
    let standard_frame = egui::containers::Frame {
        inner_margin: egui::Margin {
            left: 0.,
            right: 0.,
            top: 0.,
            bottom: 0.,
        },
        outer_margin: egui::Margin {
            left: 0.,
            right: 0.,
            top: 0.,
            bottom: 0.,
        },
        rounding: egui::Rounding {
            nw: 0.0,
            ne: 0.0,
            sw: 0.0,
            se: 0.0,
        },
        shadow: egui::epaint::Shadow::NONE,
        fill: *main_panel_color,
        stroke: egui::Stroke::NONE,
    };
    egui::CentralPanel::default().frame(standard_frame)
}

#[derive(Debug, Hiarc, Clone, Default)]
pub struct UiContext {
    pub egui_ctx: egui::Context,
    pub textures: Rc<RefCell<HashMap<TextureId, TextureContainer>>>,
}

#[derive(Default, PartialEq)]
pub struct UiCachedProps {
    window_width: u32,
    window_height: u32,
    window_pixels_per_point: f32,
}

#[derive(Clone)]
pub struct UiCachedOutput {
    pub rect: egui::Rect,
    pub output: egui::FullOutput,
    pub zoom_level: f32,
    pub custom_paints: HashMap<u64, Rc<dyn CustomCallbackTrait>>,
}

pub type RepaintListeners = Arc<RwLock<HashMap<ViewportId, Arc<AtomicBool>>>>;

fn load_proportional_fonts(egui_ctx: &egui::Context, font_definitions: &FontDefinitions) {
    // by default only copy the proportional fonts
    let mut apply_font_definitions = FontDefinitions::empty();
    if let Some(font_names) = font_definitions
        .families
        .get(&egui::FontFamily::Proportional)
    {
        for font_name in font_names {
            let Some(font) = font_definitions.font_data.get(font_name) else {
                continue;
            };
            apply_font_definitions
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push(font_name.clone());
            apply_font_definitions
                .font_data
                .insert(font_name.clone(), font.clone());
        }
    }
    egui_ctx.set_fonts(apply_font_definitions);
}

/// Helps to create UIs in efficient matter
#[derive(Debug)]
pub struct UiCreator {
    pub font_definitions: Option<FontDefinitions>,
    pub context: UiContext,
    pub id_gen: Cell<u64>,
    pub repaint_listeners: RepaintListeners,
    pub zoom_level: Rc<Cell<Option<f32>>>,
    pub has_monospace_fonts: Rc<Cell<bool>>,
}

impl Default for UiCreator {
    fn default() -> Self {
        let repaint_listeners =
            Arc::new(RwLock::new(<HashMap<ViewportId, Arc<AtomicBool>>>::new()));
        let context = egui::Context::default();
        context.options_mut(|option| option.zoom_with_keyboard = false);
        let repaint_listeners_cb = repaint_listeners.clone();
        context.set_request_repaint_callback(move |cb| {
            if let Some(should_repaint) = repaint_listeners_cb.read().unwrap().get(&cb.viewport_id)
            {
                should_repaint.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        });
        context.options_mut(|option| {
            option.zoom_with_keyboard = false;
            option.reduce_texture_memory = true;
        });
        let vis = egui::style::Visuals::dark();
        context.set_visuals(vis.clone());
        context.set_embed_viewports(true);
        let context = UiContext {
            egui_ctx: context,
            ..Default::default()
        };

        Self {
            font_definitions: Default::default(),
            context,
            id_gen: Default::default(),
            repaint_listeners,
            zoom_level: Default::default(),
            has_monospace_fonts: Default::default(),
        }
    }
}

impl UiCreator {
    pub fn load_font(&mut self, font_definitions: &FontDefinitions) {
        load_proportional_fonts(&self.context.egui_ctx, font_definitions);

        self.font_definitions = Some(font_definitions.clone());
    }

    pub fn set_zoom_level(&self, zoom_level: f32) {
        self.zoom_level.set(Some(zoom_level));
    }
}

/// UI is not a client component, it should be cleanly separated from any game logic (but can read it)
pub struct UiContainer {
    pub context: UiContext,
    pub stencil_context: UiContext,
    pub viewport_id: ViewportId,
    pub stencil_viewport_id: ViewportId,

    pub ui_state: UiState,

    pub main_panel_color: Color32,

    should_repaint: Arc<AtomicBool>,
    cached_props: UiCachedProps,
    cached_output: Option<UiCachedOutput>,
    pub last_clipped_primitives: Vec<egui::ClippedPrimitive>,
    pub last_clipped_primitives_stencil: Vec<egui::ClippedPrimitive>,
    /// should tesselate clipped primitives this frame
    pub should_tesselate: bool,
    pub should_tesselate_stencil: bool,

    /// The zoom level is shared with all UI that has the same context
    pub zoom_level: Rc<Cell<Option<f32>>>,
    has_monospace_fonts: Rc<Cell<bool>>,

    pub font_definitions: Option<FontDefinitions>,
}

impl UiContainer {
    pub fn new(creator: &UiCreator) -> Self {
        let should_repaint: Arc<AtomicBool> = Default::default();

        let context = creator.context.clone();
        let stencil_context = creator.context.clone();

        let viewport_id = ViewportId::from_hash_of(creator.id_gen.get() + 1);
        creator.id_gen.set(creator.id_gen.get() + 1);
        let stencil_viewport_id = ViewportId::from_hash_of(creator.id_gen.get() + 1);
        creator.id_gen.set(creator.id_gen.get() + 1);

        creator
            .repaint_listeners
            .write()
            .unwrap()
            .insert(viewport_id, should_repaint.clone());

        Self {
            context,
            stencil_context,
            viewport_id,
            stencil_viewport_id,

            ui_state: UiState::default(),

            main_panel_color: Color32::TRANSPARENT,

            should_repaint,
            cached_props: Default::default(),
            cached_output: Default::default(),
            last_clipped_primitives: Default::default(),
            last_clipped_primitives_stencil: Default::default(),
            should_tesselate: false,
            should_tesselate_stencil: false,

            zoom_level: creator.zoom_level.clone(),
            has_monospace_fonts: creator.has_monospace_fonts.clone(),
            font_definitions: creator.font_definitions.clone(),
        }
    }

    /// If the implementation needs monofonts, it must call this function.
    ///
    /// This function can be called multiple times without additional performance
    /// overhead.
    pub fn load_monospace_fonts(&self) {
        if self.has_monospace_fonts.get() {
            return;
        }
        if let Some(font_definitions) = &self.font_definitions {
            self.context.egui_ctx.set_fonts(font_definitions.clone());
            self.has_monospace_fonts.set(true);
        }
    }

    pub fn set_main_panel_color(&mut self, main_panel_color: &Color32) {
        self.main_panel_color = *main_panel_color;
    }

    /// returns the canvas rect, full output and current zoom level
    pub fn render<U>(
        &mut self,
        window_width: u32,
        window_height: u32,
        window_pixels_per_point: f32,
        mut render_func: impl FnMut(&mut egui::Ui, &mut UiRenderPipe<U>, &mut UiState),
        pipe: &mut UiRenderPipe<U>,
        mut input: egui::RawInput,
        as_stencil: bool,
    ) -> (egui::Rect, egui::FullOutput, f32) {
        // tell our rendering engine to tesselate again
        if as_stencil {
            self.should_tesselate_stencil = true;
        } else {
            self.should_tesselate = true;
        }

        let viewport_id = if as_stencil {
            self.stencil_viewport_id
        } else {
            self.viewport_id
        };
        let egui_ctx = if as_stencil {
            &self.stencil_context.egui_ctx
        } else {
            &self.context.egui_ctx
        };
        let mut zoom_level = self.zoom_level.get().unwrap_or(window_pixels_per_point);

        let zoom_diff = zoom_level / window_pixels_per_point;

        // first go through all events
        let mut hint_has_text_input = false;
        // scale the input events down
        input.events.retain_mut(|ev| match ev {
            egui::Event::PointerMoved(ev) => {
                *ev = egui::pos2(ev.x, ev.y) / zoom_diff;
                true
            }
            egui::Event::PointerButton {
                pos,
                button: _,
                pressed: _,
                modifiers: _,
            } => {
                *pos = egui::pos2(pos.x, pos.y) / zoom_diff;
                true
            }
            egui::Event::Text(_) => {
                hint_has_text_input = true;
                true
            }
            egui::Event::MouseWheel {
                delta:
                    Vec2 {
                        y: extra_zoom_level,
                        ..
                    },
                modifiers: Modifiers { ctrl: true, .. },
                ..
            }
            | egui::Event::Zoom(extra_zoom_level) => {
                let incr_val = if *extra_zoom_level > 0.0 {
                    if zoom_level < 1.5 {
                        0.25
                    } else {
                        0.5
                    }
                } else if *extra_zoom_level < 0.0 {
                    if zoom_level > 1.5 {
                        -0.5
                    } else {
                        -0.25
                    }
                } else {
                    0.0
                };
                zoom_level = (zoom_level + incr_val)
                    .clamp(window_pixels_per_point - 0.5, window_pixels_per_point + 1.0);
                false
            }
            _ => true,
        });
        self.ui_state.hint_had_input = hint_has_text_input;

        let screen_rect = egui::Rect {
            min: egui::Pos2 { x: 0.0, y: 0.0 },
            max: egui::Pos2 {
                x: window_width as f32 / zoom_level,
                y: window_height as f32 / zoom_level,
            },
        };
        input.screen_rect = if screen_rect.width() > 0.0 && screen_rect.height() > 0.0 {
            Some(screen_rect)
        } else {
            None
        };
        let cur_time_secs =
            pipe.cur_time.as_nanos() as f64 / (Duration::from_secs(1).as_nanos() as f64);
        input.time = Some(cur_time_secs);

        input.viewport_id = viewport_id;
        input.focused = true;
        input.viewports.insert(
            input.viewport_id,
            egui::ViewportInfo {
                parent: Default::default(),
                title: Default::default(),
                events: Default::default(),
                native_pixels_per_point: Some(zoom_level),
                monitor_size: Default::default(),
                inner_rect: Default::default(),
                outer_rect: Default::default(),
                minimized: Default::default(),
                maximized: Default::default(),
                fullscreen: Default::default(),
                focused: Default::default(),
            },
        );
        if zoom_level == window_pixels_per_point {
            self.zoom_level.set(None);
        } else {
            self.zoom_level.set(Some(zoom_level));
        }
        if zoom_level != egui_ctx.pixels_per_point() {
            // This is insanely hacky, but https://github.com/emilk/egui/issues/3556
            // Also this will most likely break if egui won't call fonts.clear()
            // anymore if a new font definition arrives
            egui_ctx.input_mut(|i| {
                i.pixels_per_point = 50.0;
            });
            if self.has_monospace_fonts.get() {
                egui_ctx.set_fonts(self.font_definitions.clone().unwrap_or_default());
            } else {
                load_proportional_fonts(
                    egui_ctx,
                    &self.font_definitions.clone().unwrap_or_default(),
                );
            }
            egui_ctx.input_mut(|i| {
                i.pixels_per_point = zoom_level;
            });
        }
        (
            screen_rect,
            egui_ctx.run(input, |egui_ctx| {
                egui_ctx.set_style(default_style());
                gui_main_panel(&self.main_panel_color)
                    .show(egui_ctx, |ui| render_func(ui, pipe, &mut self.ui_state));
            }),
            zoom_level,
        )
    }

    /// Like [`Self::render`], but it remembers if any input
    /// changed (window props, raw input etc.).
    /// If nothing changed the `render_func` is never called.
    /// This is useful if you plan to not rerender every frame.
    /// Note that this only works if the UI does not rely on immediate changes
    /// by variables that are passed by a user (basically any state
    /// this function can not know about).
    /// Additionally it only checks for events in the `input` variable,
    /// all other props are ignored.
    /// returns the canvas rect, full output and current zoom level
    pub fn render_cached<U>(
        &mut self,
        window_width: u32,
        window_height: u32,
        window_pixels_per_point: f32,
        render_func: impl FnMut(&mut egui::Ui, &mut UiRenderPipe<U>, &mut UiState),
        pipe: &mut UiRenderPipe<U>,
        input: egui::RawInput,
        as_stencil: bool,
        force_rerender: bool,
    ) -> (egui::Rect, egui::FullOutput, f32) {
        let new_cached = UiCachedProps {
            window_width,
            window_height,
            window_pixels_per_point,
        };
        if self.cached_props != new_cached
            || self.cached_output.is_none()
            || !input.events.is_empty()
            || self
                .should_repaint
                .swap(false, std::sync::atomic::Ordering::Relaxed)
            || force_rerender
        {
            let (rect, output, zoom_level) = self.render(
                window_width,
                window_height,
                window_pixels_per_point,
                render_func,
                pipe,
                input.clone(),
                as_stencil,
            );
            self.cached_output = Some(UiCachedOutput {
                rect,
                output,
                zoom_level,
                custom_paints: self.ui_state.custom_paints.clone(),
            });
        }
        self.cached_props = new_cached;
        let output = self.cached_output.clone().unwrap();
        if let Some(cached_output) = self.cached_output.as_mut() {
            // never do a texture delta twice
            cached_output.output.textures_delta.clear();
            // also set the custom paints to the cached ones
            self.ui_state
                .custom_paints
                .clone_from(&cached_output.custom_paints);
        }
        (output.rect, output.output, output.zoom_level)
    }
}
