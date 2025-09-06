use std::iter::Peekable;

use base::linked_hash_map_view::FxLinkedHashMap;
use egui::{Color32, Layout, Rect, RichText, Shape};
use egui_extras::{Size, StripBuilder};

use game_interface::types::{
    id_types::{CharacterId, StageId},
    render::{character::CharacterInfo, scoreboard::ScoreboardStageInfo},
};
use ui_base::types::{UiRenderPipe, UiState};

use crate::scoreboard::{
    content::list::definitions::{TABLE_CONTENT_FONT_SIZES, TABLE_CONTENT_ROW_HEIGHTS},
    user_data::UserData,
};

use super::entry::{FrameRect, RenderPlayer};

/// player list frame
pub fn render<'a>(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    character_infos: &FxLinkedHashMap<CharacterId, CharacterInfo>,
    players: &mut Peekable<impl Iterator<Item = RenderPlayer<'a>>>,
    players_to_render: usize,
    stages: &FxLinkedHashMap<StageId, ScoreboardStageInfo>,
    full_ui_rect: &Rect,
    font_size_index: usize,
    spacing_y: f32,
    frame_rect: &mut FxLinkedHashMap<StageId, FrameRect>,
) {
    let item_height = TABLE_CONTENT_ROW_HEIGHTS[font_size_index] + spacing_y;
    let mut strip = StripBuilder::new(ui);
    for _ in 0..players_to_render + stages.len() {
        strip = strip.size(Size::exact(item_height)).clip(true);
    }

    strip.vertical(|mut strip| {
        for _ in 0..players_to_render {
            let cur_id = players.peek().and_then(|(stage_id, _)| stage_id.copied());
            strip.cell(|ui| {
                ui.style_mut().wrap_mode = None;
                super::entry::render(
                    ui,
                    pipe,
                    ui_state,
                    character_infos,
                    players,
                    full_ui_rect,
                    font_size_index,
                    spacing_y,
                    frame_rect,
                );
            });
            let next_id = players.peek().and_then(|(id, _)| id.copied());
            if let Some(stage_id) = cur_id {
                let font_size = TABLE_CONTENT_FONT_SIZES[font_size_index];
                if cur_id != next_id
                    && let Some(stage) = cur_id.and_then(|id| stages.get(&id))
                {
                    strip.cell(|ui| {
                        ui.style_mut().wrap_mode = None;
                        let rect = ui.available_rect_before_wrap();
                        frame_rect
                            .entry(stage_id)
                            .or_insert_with_keep_order(|| FrameRect {
                                rects: Default::default(),
                                shape_id: ui.painter().add(Shape::Noop),
                            })
                            .rects
                            .push(rect);
                        ui.with_layout(Layout::left_to_right(egui::Align::Center), |ui| {
                            let team_size_str = if stage.max_size > 0 {
                                format!(" - {}/{}", stage.characters.len(), stage.max_size)
                            } else {
                                String::new()
                            };

                            ui.label(
                                RichText::new(format!(
                                    "Team: {}{}",
                                    stage.name.as_str(),
                                    team_size_str
                                ))
                                .size(font_size)
                                .color(Color32::WHITE),
                            );
                        });
                    });
                }
            }
        }
    });
}
