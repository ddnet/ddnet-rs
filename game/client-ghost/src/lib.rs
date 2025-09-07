use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use base::{
    hash::{Hash, fmt_hash},
    reduced_ascii_str::ReducedAsciiString,
    steady_clock::SteadyClock,
};
use base_io::{io::Io, runtime::IoRuntimeTask};
use base_io_traits::fs_traits::FileSystemEntryTy;
use client_demo::DemoViewer;
use client_render_game::render_game::RenderGameInput;
use config::config::ConfigEngine;
use game_config::config::ConfigGame;
use game_interface::types::{
    game::GameEntityId,
    id_gen::IdGenerator,
    render::{game::GameRenderInfo, stage::StageRenderInfo, world::WorldRenderInfo},
};
use graphics::graphics::graphics::Graphics;
use graphics_backend::backend::GraphicsBackend;
use pool::datatypes::PoolFxLinkedHashMap;
use sound::sound::SoundManager;
use sound_backend::sound_backend::SoundBackend;
use ui_base::{font_data::FontDefinitions, ui::UiCreator};

struct GhostIds {
    usable_ids: VecDeque<GameEntityId>,
    next_ids: VecDeque<GameEntityId>,
    id_generator: IdGenerator,
}

impl GhostIds {
    pub fn next_id(&mut self, is_id_ok: impl Fn(GameEntityId) -> bool) -> GameEntityId {
        let id = loop {
            let id = self
                .usable_ids
                .pop_front()
                .unwrap_or_else(|| self.id_generator.next_id());

            if is_id_ok(id) {
                break id;
            }
        };

        self.next_ids.push_back(id);
        id
    }

    pub fn swap(&mut self) {
        self.next_ids.append(&mut self.usable_ids);
        std::mem::swap(&mut self.usable_ids, &mut self.next_ids);
    }
}

pub struct GhostViewer {
    ghosts: HashMap<String, DemoViewer>,

    sound: SoundManager,
    graphics: Graphics,
    backend: Rc<GraphicsBackend>,
    sound_backend: Rc<SoundBackend>,
    time: SteadyClock,

    ids: GhostIds,

    base_path: PathBuf,
    fonts: FontDefinitions,

    task: Option<IoRuntimeTask<Vec<String>>>,
    io: Io,
    tp: Arc<rayon::ThreadPool>,
}

impl GhostViewer {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        io: &Io,
        tp: &Arc<rayon::ThreadPool>,
        sound: &SoundManager,
        graphics: &Graphics,
        backend: &Rc<GraphicsBackend>,
        sound_backend: &Rc<SoundBackend>,
        time: &SteadyClock,
        map_name: &ReducedAsciiString,
        map_hash: Hash,
        fonts: FontDefinitions,
    ) -> Self {
        let id_generator = IdGenerator::new();
        id_generator.reverse();

        let fs = io.fs.clone();
        let map_name = map_name.to_string();
        let base_path = format!("ghosts/{}_{}", map_name, fmt_hash(&map_hash));
        let base_path_task = base_path.clone();
        let task = io.rt.spawn(async move {
            let entries = fs.entries_in_dir(base_path_task.as_ref()).await?;
            Ok(entries
                .into_iter()
                .filter_map(|(name, ty)| match ty {
                    FileSystemEntryTy::File { .. } => Some(name),
                    FileSystemEntryTy::Directory => None,
                })
                .collect())
        });

        Self {
            ghosts: Default::default(),
            sound: sound.clone(),
            graphics: graphics.clone(),
            backend: backend.clone(),
            sound_backend: sound_backend.clone(),
            time: time.clone(),

            ids: GhostIds {
                usable_ids: Default::default(),
                next_ids: Default::default(),
                id_generator,
            },

            base_path: base_path.into(),
            fonts,

            task: Some(task),

            io: io.clone(),
            tp: tp.clone(),
        }
    }

    pub fn update(
        &mut self,
        config: &ConfigEngine,
        config_game: &ConfigGame,
        ui_creator: &UiCreator,
        race_time: Duration,
        input: &mut RenderGameInput,
    ) {
        if self.task.as_ref().is_some_and(|task| task.is_finished()) {
            let task = self.task.take().unwrap();
            match task.get() {
                Ok(ghosts) => {
                    self.ghosts.extend(ghosts.into_iter().map(|ghost| {
                        let demo_path = self.base_path.join(&ghost);
                        (
                            ghost,
                            DemoViewer::new(
                                &self.io,
                                &self.tp,
                                demo_path.as_ref(),
                                self.fonts.clone(),
                                None,
                            ),
                        )
                    }));
                }
                Err(err) => {
                    log::error!("failed to fetch ghosts: {err}")
                }
            }
        }

        self.ghosts.retain(|_, ghost| {
            if let Err(err) = ghost.continue_loading(
                &self.sound,
                &self.graphics,
                &self.backend,
                &self.sound_backend,
                config,
                config_game,
                &self.time,
                ui_creator,
            ) {
                log::warn!("failed to render ghost: {err}");
                return false;
            }
            if let Some(demo_viewer) = ghost.try_get_mut() {
                match demo_viewer.get_render_input_for_time(race_time) {
                    Ok(mut render_input) => {
                        let stage_id = self
                            .ids
                            .next_id(|id| !input.stages.contains_key(&id.into()));
                        let render_stage = input
                            .stages
                            .entry(stage_id.into())
                            .or_insert_with_keep_order(|| StageRenderInfo {
                                world: WorldRenderInfo {
                                    projectiles: PoolFxLinkedHashMap::new_without_pool(),
                                    ctf_flags: PoolFxLinkedHashMap::new_without_pool(),
                                    lasers: PoolFxLinkedHashMap::new_without_pool(),
                                    pickups: PoolFxLinkedHashMap::new_without_pool(),
                                    characters: PoolFxLinkedHashMap::new_without_pool(),
                                },
                                game: GameRenderInfo::Race {},
                                game_ticks_passed: 0,
                            });

                        for (real_id, char_info) in render_input.character_infos.drain() {
                            let id = self
                                .ids
                                .next_id(|id| !input.character_infos.contains_key(&id.into()));
                            if let Some(stage) = char_info
                                .stage_id
                                .and_then(|stage_id| render_input.stages.get_mut(&stage_id))
                                && let Some(render_char) = stage.world.characters.remove(&real_id)
                            {
                                render_stage.world.characters.insert(id.into(), render_char);
                            }
                            input.character_infos.insert(id.into(), char_info);
                        }
                    }
                    Err(err) => {
                        log::warn!("cancelled ghost rendering: {err}");
                        return false;
                    }
                }
            }
            true
        });
        self.ids.swap();
    }
}
