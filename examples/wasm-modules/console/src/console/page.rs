use std::rc::Rc;

use api_ui_game::render::create_skin_container;
use client_containers::skins::SkinContainer;
use client_render_base::render::tee::RenderTee;
use client_types::console::{ConsoleEntry, ConsoleEntryCmd};
use client_ui::console::user_data::UserData;
use egui::Color32;
use graphics::graphics::graphics::Graphics;
use ui_base::types::{UiRenderPipe, UiState};
use ui_generic::traits::UiPageInterface;

#[derive(Debug)]
pub struct Console {
    skin_container: SkinContainer,
    render_tee: RenderTee,
}

impl Console {
    pub fn new(graphics: &Graphics) -> Self {
        Self {
            skin_container: create_skin_container(),
            render_tee: RenderTee::new(graphics),
        }
    }

    fn render_impl(
        &mut self,
        ui: &mut egui::Ui,
        pipe: &mut UiRenderPipe<()>,
        ui_state: &mut UiState,
    ) {
        let mut logs = String::new();
        for i in 0..100 {
            logs.push_str(&format!("test {i}\ntestr2\n"));
        }
        client_ui::console::main_frame::render(
            ui,
            &mut UiRenderPipe {
                cur_time: pipe.cur_time,
                user_data: &mut UserData {
                    entries: &vec![
                        ConsoleEntry::Cmd(ConsoleEntryCmd {
                            name: "test".to_string(),
                            usage: "test".to_string(),
                            description: "test".to_string(),
                            cmd: Rc::new(|_, _, _, _| Ok("".to_string())),
                            args: vec![],
                            allows_partial_cmds: false,
                        }),
                        ConsoleEntry::Cmd(ConsoleEntryCmd {
                            name: "test2".to_string(),
                            usage: "test2".to_string(),
                            description: "test2".to_string(),
                            cmd: Rc::new(|_, _, _, _| Ok("".to_string())),
                            args: vec![],
                            allows_partial_cmds: false,
                        }),
                        ConsoleEntry::Cmd(ConsoleEntryCmd {
                            name: "test3".to_string(),
                            usage: "test3".to_string(),
                            description: "test3".to_string(),
                            cmd: Rc::new(|_, _, _, _| Ok("".to_string())),
                            args: vec![],
                            allows_partial_cmds: false,
                        }),
                    ],
                    config: &mut Default::default(),
                    msgs: &mut logs,
                    msg: &mut "te".to_string(),
                    msg_history: &mut Default::default(),
                    msg_history_index: &mut Default::default(),
                    cursor: &mut 0,
                    select_index: &mut Some(0),
                    cache: &mut Default::default(),
                    can_change_client_config: true,
                    custom_matches: &|_| None,
                    render_custom_matches: &|_, _, _, _, _, _| {},
                    skin_container: &mut self.skin_container,
                    render_tee: &self.render_tee,
                },
            },
            ui_state,
            Color32::from_rgba_unmultiplied(0, 0, 0, 150),
        )
    }
}

impl UiPageInterface<()> for Console {
    fn render(&mut self, ui: &mut egui::Ui, pipe: &mut UiRenderPipe<()>, ui_state: &mut UiState) {
        self.render_impl(ui, pipe, ui_state)
    }
}
