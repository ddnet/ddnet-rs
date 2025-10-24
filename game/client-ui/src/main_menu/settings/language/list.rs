use std::collections::{BTreeMap, HashMap};

use client_containers::container::ContainerItemIndexType;
use game_interface::types::resource_key::NetworkResourceKey;
use ui_base::types::{UiRenderPipe, UiState};

use crate::{main_menu::user_data::UserData, utils::render_flag_for_ui};

struct Language {
    flag: String,
    code: String,
}

pub fn lang_list(ui: &mut egui::Ui, pipe: &mut UiRenderPipe<UserData>, ui_state: &mut UiState) {
    let languages = [(
        "english".to_string(),
        Language {
            flag: "us".to_string(),
            code: "en".to_string(),
        },
    )]
    .into_iter()
    .collect::<HashMap<String, Language>>();
    let entries_sorted = languages
        .keys()
        .map(|lang| (lang.to_string(), ContainerItemIndexType::Disk))
        .collect::<BTreeMap<_, _>>();
    let setting = &mut pipe.user_data.config.game.cl.language;
    let search_str = pipe
        .user_data
        .config
        .engine
        .ui
        .path
        .query
        .entry("lang-search".to_string())
        .or_default();
    let mut next_name = None;
    super::super::list::list::render(
        ui,
        entries_sorted.iter().map(|(name, &ty)| (name.as_str(), ty)),
        50.0,
        |_, name| {
            let valid: Result<NetworkResourceKey<32>, _> = name.try_into();
            valid.map(|_| ()).map_err(|err| err.into())
        },
        |_, name| setting == name,
        |s| s,
        |ui, _, name, pos, asset_size| {
            let flag = pipe.user_data.flags_container.default_key.clone();
            let name = &languages.get(name).unwrap().flag;
            render_flag_for_ui(
                pipe.user_data.stream_handle,
                pipe.user_data.canvas_handle,
                pipe.user_data.flags_container,
                ui,
                ui_state,
                ui.ctx().screen_rect(),
                Some(ui.clip_rect()),
                &flag,
                name,
                pos,
                asset_size,
            );
        },
        |_, name| {
            next_name = Some(languages.get(name).unwrap().code.to_string());
        },
        |_, _| None,
        search_str,
        |_| {},
    );
    if let Some(next_name) = next_name.take() {
        *setting = next_name;
    }
}
