use egui::{text::LayoutJob, Color32, FontId, RichText, TextFormat};
use map::skeleton::groups::layers::design::MapLayerSkeleton;

use crate::map::{EditorGroup, EditorLayer, EditorPhysicsLayer, EditorResources};

pub fn group_name(group: &EditorGroup, index: usize) -> String {
    if group.name.is_empty() {
        format!("Group #{index}")
    } else {
        format!("Group \"{}\"", group.name)
    }
}

pub fn layer_name(
    ui: &egui::Ui,
    resources: &EditorResources,
    layer: &EditorLayer,
    index: usize,
) -> (RichText, LayoutJob) {
    let (icon, icon_clr) = match layer {
        MapLayerSkeleton::Abritrary(_) => ("\u{f057}", Color32::WHITE),
        MapLayerSkeleton::Tile(_) => ("\u{f00a}", Color32::LIGHT_YELLOW),
        MapLayerSkeleton::Quad(_) => ("\u{f61f}", Color32::LIGHT_BLUE),
        MapLayerSkeleton::Sound(_) => ("\u{1f3b5}", Color32::LIGHT_RED),
    };
    let icon = RichText::new(icon).color(icon_clr);
    let text_color = ui.style().visuals.text_color();
    if !layer.name().is_empty() {
        (
            icon,
            LayoutJob::simple(layer.name().into(), Default::default(), text_color, 0.0),
        )
    } else if let Some(text) = match layer {
        MapLayerSkeleton::Abritrary(_) => Some("\u{f057} unsupported".to_string()),
        MapLayerSkeleton::Tile(layer) => layer.layer.attr.image_array.map(|image| {
            format!(
                "\u{f302} {}",
                resources.image_arrays[image].def.name.as_str()
            )
        }),
        MapLayerSkeleton::Quad(layer) => layer
            .layer
            .attr
            .image
            .map(|image| format!("\u{f03e} {}", resources.images[image].def.name.as_str())),
        MapLayerSkeleton::Sound(layer) => layer
            .layer
            .attr
            .sound
            .map(|sound| format!("\u{1f3b5} {}", resources.sounds[sound].def.name.as_str())),
    } {
        let mut job = LayoutJob::simple("\"".into(), Default::default(), text_color, 0.0);
        job.append(
            &text,
            0.0,
            TextFormat::simple(
                FontId::new(10.0, egui::FontFamily::Proportional),
                text_color,
            ),
        );
        job.append("\"", 0.0, TextFormat::simple(FontId::default(), text_color));
        (icon, job)
    } else {
        (
            icon,
            LayoutJob::simple(format!("#{index}"), Default::default(), text_color, 0.0),
        )
    }
}

pub fn layer_name_phy(layer: &EditorPhysicsLayer, index: usize) -> String {
    let layer_name = match layer {
        EditorPhysicsLayer::Arbitrary(_) => {
            todo!()
        }
        EditorPhysicsLayer::Game(_) => "Game",
        EditorPhysicsLayer::Front(_) => "Front",
        EditorPhysicsLayer::Tele(_) => "Tele",
        EditorPhysicsLayer::Speedup(_) => "Speedup",
        EditorPhysicsLayer::Switch(_) => "Switch",
        EditorPhysicsLayer::Tune(_) => "Tune",
    };
    format!("#{index} {layer_name}")
}
