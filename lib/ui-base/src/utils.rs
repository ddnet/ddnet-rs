use egui::{FontId, Response, RichText, Ui};
use egui_extras::{Size, StripBuilder};

pub fn get_margin(ui: &mut Ui) -> f32 {
    let style = ui.style();
    style
        .spacing
        .item_spacing
        .x
        .max(style.spacing.item_spacing.y)
}

pub fn add_horizontal_margins(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) -> Response {
    let margin = get_margin(ui);
    let style = ui.style_mut();
    let spacing = style.spacing.item_spacing;
    style.spacing.item_spacing.x = margin;
    StripBuilder::new(ui)
        .size(Size::exact(0.0))
        .size(Size::remainder())
        .size(Size::exact(0.0))
        .horizontal(|mut strip| {
            strip.empty();
            strip.cell(|ui| {
                let style = ui.style_mut();
                style.wrap_mode = None;
                style.spacing.item_spacing = spacing;
                add_contents(ui);
            });
            strip.empty();
        })
}
pub fn add_vertical_margins(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) -> Response {
    let margin = get_margin(ui);
    let style = ui.style_mut();
    let spacing = style.spacing.item_spacing;
    style.spacing.item_spacing.y = margin;
    StripBuilder::new(ui)
        .size(Size::exact(0.0))
        .size(Size::remainder())
        .size(Size::exact(0.0))
        .vertical(|mut strip| {
            strip.empty();
            strip.cell(|ui| {
                let style = ui.style_mut();
                style.wrap_mode = None;
                style.spacing.item_spacing = spacing;
                add_contents(ui);
            });
            strip.empty();
        })
}
pub fn add_margins(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui)) -> Response {
    add_horizontal_margins(ui, |ui| {
        add_vertical_margins(ui, add_contents);
    })
}

// from https://github.com/emilk/egui/blob/3672b150ed2c63f87f2a2c8c86ba639f0bbc7d81/crates/egui_demo_lib/src/demo/toggle_switch.rs
pub fn toggle_ui(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    // Widget code can be broken up in four steps:
    //  1. Decide a size for the widget
    //  2. Allocate space for it
    //  3. Handle interactions with the widget (if any)
    //  4. Paint the widget

    // 1. Deciding widget size:
    // You can query the `ui` how much space is available,
    // but in this example we have a fixed size widget based on the height of a standard button:
    let desired_size = ui.spacing().interact_size.y * egui::vec2(2.0, 1.0);

    // 2. Allocating space:
    // This is where we get a region of the screen assigned.
    // We also tell the Ui to sense clicks in the allocated region.
    let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    // 3. Interact: Time to check for clicks!
    if response.clicked() {
        *on = !*on;
        response.mark_changed(); // report back that the value changed
    }

    // Attach some meta-data to the response which can be used by screen readers:
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, *on, false, ""));

    // 4. Paint!
    // Make sure we need to paint:
    if ui.is_rect_visible(rect) {
        // Let's ask for a simple animation from egui.
        // egui keeps track of changes in the boolean associated with the id and
        // returns an animated value in the 0-1 range for how much "on" we are.
        let how_on = ui.ctx().animate_bool(response.id, *on);
        // We will follow the current style by asking
        // "how should something that is being interacted with be painted?".
        // This will, for instance, give us different colors when the widget is hovered or clicked.
        let visuals = ui.style().interact_selectable(&response, *on);
        // All coordinates are in absolute screen coordinates so we use `rect` to place the elements.
        let rect = rect.expand(visuals.expansion);
        let radius = 0.5 * rect.height();
        ui.painter().rect(
            rect,
            radius,
            visuals.bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        // Paint the circle, animating it from left to right with `how_on`:
        let circle_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
        let center = egui::pos2(circle_x, rect.center().y);
        ui.painter()
            .circle(center, 0.75 * radius, visuals.bg_fill, visuals.fg_stroke);
    }

    // All done! Return the interaction response so the user can check what happened
    // (hovered, clicked, ...) and maybe show a tooltip:
    response
}

/// Password entry field with ability to toggle character hiding.
///
/// ## Example:
/// ``` ignore
/// password_ui(ui, &mut my_password);
/// ```
#[allow(clippy::ptr_arg)] // false positive
pub fn password_ui(ui: &mut egui::Ui, password: &mut String) -> egui::Response {
    // This widget has its own state — show or hide password characters (`show_plaintext`).
    // In this case we use a simple `bool`, but you can also declare your own type.
    // It must implement at least `Clone` and be `'static`.
    // If you use the `persistence` feature, it also must implement `serde::{Deserialize, Serialize}`.

    // Generate an id for the state
    let state_id = ui.id().with("show_plaintext");

    // Get state for this widget.
    // You should get state by value, not by reference to avoid borrowing of [`Memory`].
    let mut show_plaintext = ui.data_mut(|d| d.get_temp::<bool>(state_id).unwrap_or(false));

    // Process ui, change a local copy of the state
    // We want TextEdit to fill entire space, and have button after that, so in that case we can
    // change direction to right_to_left.
    let result = ui.with_layout(
        egui::Layout::left_to_right(egui::Align::Min).with_main_justify(true),
        |ui| {
            ui.style_mut().spacing.item_spacing.x = 0.0;
            StripBuilder::new(ui)
                .size(Size::remainder())
                .size(Size::exact(20.0))
                .horizontal(|mut strip| {
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;
                        // Show the password field:
                        ui.add(egui::TextEdit::singleline(password).password(!show_plaintext));
                    });
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;
                        // Toggle the `show_plaintext` bool with a button:
                        let response = ui
                            .add(egui::SelectableLabel::new(
                                show_plaintext,
                                if show_plaintext {
                                    "\u{f070}"
                                } else {
                                    "\u{f06e}"
                                },
                            ))
                            .on_hover_text("Show/hide password");

                        if response.clicked() {
                            show_plaintext = !show_plaintext;
                        }
                    });
                })
        },
    );

    // Store the (possibly changed) state:
    ui.data_mut(|d| d.insert_temp(state_id, show_plaintext));

    // All done! Return the interaction response so the user can check what happened
    // (hovered, clicked, …) and maybe show a tooltip:
    result.response
}

// From (slightly changed): https://github.com/emilk/egui/blob/84d204246fd662534d30b955e8a7a076c8ee474a/crates/egui_demo_lib/src/demo/password.rs
// A wrapper that allows the more idiomatic usage pattern: `ui.add(…)`
/// Password entry field with ability to toggle character hiding.
///
/// ## Example:
/// ``` ignore
/// ui.add(password(&mut my_password));
/// ```
pub fn password(password: &mut String) -> impl egui::Widget + '_ {
    move |ui: &mut egui::Ui| password_ui(ui, password)
}

/// Quick way to created sized text.
pub fn text_sized(text: &str, text_size: f32) -> RichText {
    RichText::new(text).font(FontId::proportional(text_size))
}
