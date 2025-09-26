use client_types::actionfeed::ActionKill;
use egui::{Color32, Layout, Rect};
use game_interface::{events::GameWorldActionKillWeapon, types::render::character::TeeEye};
use math::math::vector::vec2;
use tracing::instrument;
use ui_base::types::{UiRenderPipe, UiState};
use vanilla::weapons::definitions::weapon_def::get_weapon_visual_scale;

use crate::{
    actionfeed::shared::entry_frame,
    utils::{render_tee_for_ui, render_texture_for_ui, render_weapon_for_ui},
};

use super::user_data::{RenderTeeInfo, UserData};

/// one actionfeed entry
#[instrument(level = "trace", skip_all)]
pub fn render(
    ui: &mut egui::Ui,
    pipe: &mut UiRenderPipe<UserData>,
    ui_state: &mut UiState,
    kill: &ActionKill,
    full_rect: &Rect,
) {
    entry_frame(ui, |ui| {
        let tee_size = 20.0;
        let margin_from_tee = 2.0;

        let render_tees = &mut *pipe.user_data.render_tee_helper;
        render_tees.clear();

        ui.with_layout(Layout::right_to_left(egui::Align::Min), |ui| {
            ui.style_mut().spacing.item_spacing.x = 4.0;
            ui.style_mut().spacing.item_spacing.y = 0.0;
            ui.horizontal(|ui| {
                for victim in &kill.victims {
                    ui.colored_label(Color32::WHITE, &victim.name);
                    ui.add_space(tee_size + margin_from_tee);
                    let rect = ui.available_rect_before_wrap();
                    render_tees.push(RenderTeeInfo {
                        skin: victim.skin.clone(),
                        skin_info: victim.skin_info,
                        pos: vec2::new(
                            rect.max.x + tee_size / 2.0 + margin_from_tee / 2.0,
                            rect.min.y + rect.height() / 2.0,
                        ),
                    });
                    ui.add_space(5.0);
                }
                match kill.weapon {
                    GameWorldActionKillWeapon::Weapon { weapon } => {
                        let weapon_size = tee_size / 2.0 * get_weapon_visual_scale(&weapon);
                        ui.add_space(margin_from_tee + weapon_size);
                        let rect = ui.available_rect_before_wrap();
                        let default_key = pipe.user_data.weapon_container.default_key.clone();
                        render_weapon_for_ui(
                            pipe.user_data.canvas_handle,
                            pipe.user_data.weapon_container,
                            pipe.user_data.toolkit_render,
                            ui,
                            ui_state,
                            *full_rect,
                            Some(ui.clip_rect()),
                            if let Some(killer) = &kill.killer {
                                &killer.weapon
                            } else {
                                &default_key
                            },
                            weapon,
                            vec2::new(
                                rect.max.x + weapon_size / 2.0 + margin_from_tee / 2.0,
                                rect.min.y + rect.height() / 2.0,
                            ),
                            tee_size / 2.0,
                        );
                        ui.add_space(5.0);
                    }
                    GameWorldActionKillWeapon::Ninja => {
                        ui.add_space(margin_from_tee + tee_size * 3.0 / 2.0);
                        let rect = ui.available_rect_before_wrap();
                        let ninja = pipe
                            .user_data
                            .ninja_container
                            .get_or_default_opt(kill.killer.as_ref().map(|killer| &killer.weapon));
                        render_texture_for_ui(
                            pipe.user_data.stream_handle,
                            pipe.user_data.canvas_handle,
                            &ninja.weapon,
                            ui,
                            ui_state,
                            *full_rect,
                            Some(ui.clip_rect()),
                            vec2::new(
                                rect.max.x + (tee_size * 3.0 / 2.0) / 2.0 + margin_from_tee / 2.0,
                                rect.min.y + rect.height() / 2.0,
                            ),
                            vec2::new(tee_size * 3.0 / 2.0, tee_size / 2.0),
                            None,
                        );
                        ui.add_space(5.0);
                    }
                    GameWorldActionKillWeapon::World => {
                        // nothing to render
                    }
                }
                if let Some(killer) = &kill.killer {
                    ui.colored_label(Color32::WHITE, &killer.name);
                    ui.add_space(tee_size + margin_from_tee);
                    let rect = ui.available_rect_before_wrap();
                    render_tees.push(RenderTeeInfo {
                        skin: killer.skin.clone(),
                        skin_info: killer.skin_info,
                        pos: vec2::new(
                            rect.max.x + tee_size / 2.0 + margin_from_tee / 2.0,
                            rect.min.y + rect.height() / 2.0,
                        ),
                    });
                    ui.add_space(5.0);
                }
            });
        });

        for render_tee in render_tees {
            render_tee_for_ui(
                pipe.user_data.canvas_handle,
                pipe.user_data.skin_container,
                pipe.user_data.render_tee,
                ui,
                ui_state,
                *full_rect,
                Some(ui.clip_rect()),
                &render_tee.skin,
                Some(&render_tee.skin_info),
                render_tee.pos,
                tee_size,
                TeeEye::Normal,
            );
        }
    });
}
