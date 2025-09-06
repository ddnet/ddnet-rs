//! copied and modified from egui/src/containers/frame.rs

use egui::{Frame, Rect, Response, Sense, Shape, Ui, UiBuilder, layers::ShapeIdx};

pub struct Prepared {
    /// The frame that was prepared.
    ///
    /// The margin has already been read and used,
    /// but the rest of the fields may be modified.
    pub frame: Frame,

    /// This is where we will insert the frame shape so it ends up behind the content.
    where_to_put_background: ShapeIdx,

    /// Add your widgets to this UI so it ends up within the frame.
    pub content_ui: Ui,
}

impl Prepared {
    fn content_with_margin(&self) -> Rect {
        self.content_ui.min_rect() + (self.frame.inner_margin + self.frame.outer_margin)
    }

    /// Allocate the the space that was used by [`Self::content_ui`].
    ///
    /// This MUST be called, or the parent ui will not know how much space this widget used.
    ///
    /// This can be called before or after [`Self::paint`].
    pub fn allocate_space(&self, ui: &mut Ui) -> Response {
        ui.allocate_rect(self.content_with_margin(), Sense::hover())
    }

    /// Paint the frame.
    ///
    /// This can be called before or after [`Self::allocate_space`].
    pub fn paint(self, ui: &Ui) -> Rect {
        let paint_rect = self.content_ui.min_rect();

        if ui.is_rect_visible(paint_rect) {
            let shape = self.frame.paint(paint_rect);
            ui.painter().set(self.where_to_put_background, shape);
        }
        paint_rect
    }
}

pub trait BetterFrame {
    /// Begin a dynamically colored frame.
    ///
    /// This is a more advanced API.
    /// Usually you want to use [`Self::show`] instead.
    ///
    /// See docs for [`Frame`] for an example.
    fn begin_better(self, ui: &mut Ui) -> Prepared;
}

impl BetterFrame for egui::Frame {
    fn begin_better(self, ui: &mut Ui) -> Prepared {
        let where_to_put_background = ui.painter().add(Shape::Noop);
        let outer_rect_bounds = ui.available_rect_before_wrap();

        let mut inner_rect = outer_rect_bounds - (self.inner_margin + self.outer_margin);

        // Make sure we don't shrink to the negative:
        inner_rect.max.x = inner_rect.max.x.max(inner_rect.min.x);
        inner_rect.max.y = inner_rect.max.y.max(inner_rect.min.y);

        let content_ui = ui.new_child(UiBuilder::new().max_rect(inner_rect).layout(*ui.layout()));

        // content_ui.set_clip_rect(outer_rect_bounds.shrink(self.stroke.width * 0.5)); // Can't do this since we don't know final size yet

        Prepared {
            frame: self,
            where_to_put_background,
            content_ui,
        }
    }
}
