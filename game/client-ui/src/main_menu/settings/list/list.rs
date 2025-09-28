use std::borrow::Cow;

use client_containers::container::ContainerItemIndexType;
use egui::{Layout, ScrollArea, Shape, epaint::RectShape};
use egui_extras::{Size, StripBuilder};
use fuzzy_matcher::FuzzyMatcher;
use math::math::vector::vec2;
use tracing::instrument;
use ui_base::{
    components::clearable_edit_field::clearable_edit_field, style::bg_frame_color,
    utils::add_margins,
};

#[instrument(level = "trace", skip_all)]
pub fn render<'a>(
    ui: &mut egui::Ui,
    entries: impl Iterator<Item = (&'a str, ContainerItemIndexType)>,
    entry_visual_size: f32,
    validation_fn: impl Fn(usize, &str) -> anyhow::Result<()>,
    is_selected_fn: impl Fn(usize, &str) -> bool,
    mut render_fn: impl FnMut(&mut egui::Ui, usize, &str, vec2, f32),
    mut on_click_fn: impl FnMut(usize, &str),
    tooltip_text: impl Fn(&str, ContainerItemIndexType) -> Option<Cow<'static, str>>,
    search: &mut String,
    right_from_search: impl FnOnce(&mut egui::Ui),
) {
    ui.style_mut().spacing.scroll.floating = false;
    let search_str = search.clone();
    StripBuilder::new(ui)
        .size(Size::remainder())
        .size(Size::exact(20.0))
        .vertical(|mut strip| {
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                ui.painter().add(Shape::Rect(RectShape::filled(
                    ui.available_rect_before_wrap(),
                    0.0,
                    bg_frame_color(),
                )));
                ScrollArea::vertical().show(ui, |ui| {
                    add_margins(ui, |ui| {
                        let style = ui.style_mut();
                        let spacing = style
                            .spacing
                            .item_spacing
                            .x
                            .max(style.spacing.item_spacing.y);
                        style.spacing.item_spacing.x = spacing;
                        style.spacing.item_spacing.y = spacing;
                        ui.with_layout(
                            Layout::left_to_right(egui::Align::Min)
                                .with_main_wrap(true)
                                .with_main_align(egui::Align::Min),
                            |ui| {
                                for (entry_index, (entry_name, ty)) in
                                    entries.enumerate().filter(|(_, (name, _))| {
                                        let matcher = fuzzy_matcher::skim::SkimMatcherV2::default().ignore_case();
                                        matcher.fuzzy_match(name, &search_str).is_some()
                                    })
                                {
                                    super::entry::render(
                                        ui,
                                        entry_index,
                                        entry_name,
                                        entry_visual_size,
                                        &validation_fn,
                                        &is_selected_fn,
                                        &mut render_fn,
                                        &mut on_click_fn,
                                        &tooltip_text(entry_name, ty)
                                            .map(|s| s.to_string())
                                            .unwrap_or_else(|| {
                                                if matches!(ty, ContainerItemIndexType::Http) {
                                                    format!(
                                                        "{entry_name}\n\
                                                        \u{f019} downloaded from the \
                                                        assets database."
                                                    )
                                                } else {
                                                    format!(
                                                    "{entry_name}\nStored as local asset in on your disk."
                                                )
                                                }
                                            }),
                                    );
                                }
                            },
                        );
                    });
                });
            });

            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                let width = ui.available_width();
                StripBuilder::new(ui)
                    .size(Size::remainder().at_most(width / 2.0))
                    .size(Size::remainder())
                    .horizontal(|mut strip| {
                        strip.cell(|ui| {
                            ui.style_mut().wrap_mode = None;
                            ui.horizontal_centered(|ui| {
                                // Search
                                ui.label("\u{1f50d}");
                                clearable_edit_field(ui, search, Some(200.0), None);
                            });
                        });
                        strip.cell(|ui| {
                            ui.style_mut().wrap_mode = None;
                            ui.with_layout(
                                Layout::right_to_left(egui::Align::Center),
                                right_from_search,
                            );
                        })
                    });
            })
        });
}
