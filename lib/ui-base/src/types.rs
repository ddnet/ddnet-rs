use std::{collections::HashMap, rc::Rc, sync::Arc, time::Duration};

use egui::{Color32, CornerRadius, FontDefinitions, Pos2, Rect};
use serde::{Deserialize, Serialize};

use crate::custom_callback::CustomCallbackTrait;

pub struct UiRenderPipe<'a, U: 'a> {
    pub cur_time: Duration,
    pub user_data: &'a mut U,
}

impl<'a, U: 'a> UiRenderPipe<'a, U> {
    pub fn new(cur_time: Duration, user_data: &'a mut U) -> Self {
        Self {
            cur_time,
            user_data,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlurRect {
    pub rect: Rect,
    pub rounding: CornerRadius,
    pub color: Color32,
}

impl BlurRect {
    pub fn new(rect: Rect, rounding: impl Into<CornerRadius>) -> Self {
        Self {
            rect,
            rounding: rounding.into(),
            color: Color32::BLACK,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlurCircle {
    pub center: Pos2,
    pub radius: f32,
    pub color: Color32,
}

impl BlurCircle {
    pub fn new(center: Pos2, radius: f32) -> Self {
        Self {
            center,
            radius,
            color: Color32::BLACK,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum BlurShape {
    Rect(BlurRect),
    Circle(BlurCircle),
}

#[derive(Debug)]
pub struct UiState {
    pub is_ui_open: bool,
    pub hint_had_input: bool,

    pub custom_paints: HashMap<u64, Rc<dyn CustomCallbackTrait>>,
    pub custom_paint_id: u64,

    /// blur shapes of this frame, if empty, then
    /// bluring is skipped.
    pub blur_shapes: Vec<BlurShape>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            is_ui_open: true,
            hint_had_input: false,

            custom_paints: Default::default(),
            custom_paint_id: 0,

            blur_shapes: Default::default(),
        }
    }
}

impl UiState {
    pub fn add_custom_paint(
        &mut self,
        ui: &mut egui::Ui,
        render_rect: Rect,
        custom_paint: Rc<dyn CustomCallbackTrait>,
    ) {
        let id = self.custom_paint_id;
        self.custom_paint_id += 1;
        self.custom_paints.insert(id, custom_paint);
        ui.painter().add(egui::PaintCallback {
            rect: render_rect,
            callback: Arc::new(id),
        });
    }

    pub fn add_blur_rect(&mut self, rect: Rect, rounding: impl Into<CornerRadius>) {
        self.blur_shapes
            .push(BlurShape::Rect(BlurRect::new(rect, rounding)));
    }

    pub fn add_blur_circle(&mut self, center: Pos2, radius: f32) {
        self.blur_shapes
            .push(BlurShape::Circle(BlurCircle::new(center, radius)));
    }
}

/// for encode and decode
#[derive(Debug, Serialize, Deserialize)]
pub struct RawInputWrapper {
    pub input: egui::RawInput,
}

/// for encode and decode
#[derive(Serialize, Deserialize)]
pub struct RawOutputWrapper {
    pub output: egui::PlatformOutput,
    pub blur_shapes: Vec<BlurShape>,
    pub zoom_level: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UiFonts {
    pub fonts: Option<FontDefinitions>,
}
