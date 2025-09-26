#![allow(clippy::too_many_arguments)]

pub mod ui;

use std::{
    borrow::Cow,
    collections::BTreeMap,
    ops::RangeBounds,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use anyhow::anyhow;
pub use av_encoder::types::EncoderSettings;
use av_encoder::{AvEncoder, traits::AudioVideoEncoder};
use base::steady_clock::SteadyClock;
use base_io::{io::Io, runtime::IoRuntimeTask};
use client_map::client_map::{ClientMapFile, ClientMapLoading, GameMap, RenderGameWasmManager};
use client_render_base::map::render_pipe::GameTimeInfo;
use client_render_game::render_game::{
    RenderForPlayer, RenderGameCreateOptions, RenderGameForPlayer, RenderGameInput,
    RenderGameInterface, RenderGameSettings, RenderModTy, RenderPlayerCameraMode,
};
use client_ui::demo_player::user_data::{DemoViewerEvent, DemoViewerUiState, UserData};
use config::config::ConfigEngine;
use demo::{
    ChunkHeader, DemoEvent, DemoEvents, DemoHeader, DemoHeaderExt, DemoSnapshot, DemoTail,
    recorder::{DemoRecorder, DemoRecorderCreateProps, DemoRecorderCreatePropsBase},
    utils::{decomp, deser, deser_ex},
};
use egui::{FontDefinitions, Rect};
use game_base::{assets_url::HTTP_RESOURCE_URL, game_types::intra_tick_time_to_ratio};
use game_config::config::{ConfigGame, ConfigMap, ConfigRender, ConfigSoundRender};
use game_interface::{interface::GameStateInterface, types::game::GameTickType};
use graphics::{
    graphics::graphics::Graphics,
    handles::{
        canvas::canvas::{GraphicsCanvasHandle, GraphicsCanvasMode, OffscreenCanvas},
        stream::stream::GraphicsStreamHandle,
        stream_types::StreamedQuad,
        texture::texture::TextureType,
    },
};
use graphics_backend::backend::GraphicsBackend;
use graphics_types::rendering::{BlendType, ColorMaskMode, State};
use math::math::vector::{ffixed, ubvec4, vec2};
use pool::datatypes::{
    PoolBTreeMap, PoolFxLinkedHashMap, PoolFxLinkedHashSet, PoolVec, PoolVecDeque,
};
use pool::mt_datatypes::PoolCow as MtPoolCow;
use serde::de::DeserializeOwned;
use sound::{
    commands::{SceneAirMode, SoundSceneCreateProps},
    sound::SoundManager,
};
use sound_backend::sound_backend::SoundBackend;
use tracing::instrument;
use ui::render::{DemoPlayerUiRender, DemoPlayerUiRenderPipe};
use ui_base::ui::UiCreator;

#[derive(Debug, Clone)]
pub struct DemoContainer {
    /// The const sized header
    pub header: DemoHeader,
    /// Dynamic sized header
    pub header_ext: DemoHeaderExt,
    /// Demo chunks, still untouched (compressed, serialized)
    pub demo_chunks: Vec<u8>,
    pub tail: DemoTail,
}

#[derive(Debug)]
pub struct DemoViewerInner {
    demo: DemoContainer,
    cur_snapshots: BTreeMap<u64, DemoSnapshot>,
    cur_events: BTreeMap<u64, DemoEvents>,
    cur_time: Duration,
    is_closed: bool,
    is_paused: bool,
    speed: ffixed,
}

impl DemoViewerInner {
    pub fn is_finished(&self) -> bool {
        self.cur_time >= self.demo.header.len
    }

    pub fn is_closed(&self) -> bool {
        self.is_closed
    }

    pub fn is_paused(&self) -> bool {
        self.is_paused
    }

    pub fn set_time_and_clear_chunks(&mut self, time: Duration) {
        self.cur_time = time;
        self.cur_snapshots.clear();
        self.cur_events.clear();
    }

    fn read_chunks<A: DeserializeOwned>(
        demo: &DemoContainer,
        offset: usize,
    ) -> anyhow::Result<BTreeMap<u64, A>> {
        let file = &demo.demo_chunks[offset..];

        // unpack all chunks
        let mut data: Vec<u8> = Default::default();

        anyhow::ensure!(
            file.len() >= std::mem::size_of::<u64>(),
            "file not huge enough to read u64 for compressed size"
        );
        let chunks_size = u64::from_le_bytes(file[0..std::mem::size_of::<u64>()].try_into()?);
        let file = &file[std::mem::size_of::<u64>()..];

        decomp(&file[0..chunks_size as usize], &mut data)?;
        let file = data.as_slice();

        // read item count in this chunk
        let (len, read_size) = deser::<u64>(file)?;

        let mut file = &file[read_size..];

        let mut res: BTreeMap<u64, A> = Default::default();

        let mut last_data: Option<Vec<u8>> = None;

        let mut writer: Vec<u8> = Default::default();

        for _ in 0..len {
            let (header, read_size) = deser::<ChunkHeader>(file)?;
            file = &file[read_size..];

            let data = if header.size > 0 {
                let data_slice = &file[..header.size as usize];
                let res = if let Some(last_data) = &last_data {
                    writer.clear();
                    bin_patch::patch(last_data, data_slice, &mut writer)?;
                    writer.as_slice()
                } else {
                    decomp(data_slice, &mut writer)?;
                    writer.as_slice()
                };
                last_data = Some(res.to_vec());
                let (data, _) = deser(res)?;
                file = &file[header.size as usize..];
                Some(data)
            } else {
                None
            };

            if let Some(data) = data {
                res.insert(header.monotonic_tick, data);
            }
        }

        Ok(res)
    }

    fn time_to_tick_impl(&self, time: Duration) -> u64 {
        // IMPORTANT: We intentionally ignore the events index here
        // without snapshots the game does not work at all, events
        // are kinda optional
        (time.as_nanos()
            / (Duration::from_secs(1).as_nanos()
                / self.demo.header_ext.ticks_per_second.get() as u128)) as u64
            + (self
                .demo
                .tail
                .snapshots_index
                .first_key_value()
                .map(|(tick, _)| *tick)
                .unwrap_or(u64::MAX))
    }

    fn time_to_tick(&self) -> u64 {
        self.time_to_tick_impl(self.cur_time)
    }

    fn intra_tick_time(
        &self,
        monotonic_tick: u64,
        prev_monotonic_tick: u64,
        next_monotonic_tick: u64,
    ) -> Duration {
        let monotonic_tick = monotonic_tick.clamp(prev_monotonic_tick, next_monotonic_tick);
        let tick_diff = next_monotonic_tick - prev_monotonic_tick;
        if tick_diff == 0 {
            Duration::ZERO
        } else {
            let nanos_per_tick = Duration::from_secs(1).as_nanos()
                / self.demo.header_ext.ticks_per_second.get() as u128;
            let time_in_tick = self.cur_time.as_nanos() % nanos_per_tick;
            (Duration::from_nanos(time_in_tick as u64)
                + Duration::from_nanos(
                    (monotonic_tick - prev_monotonic_tick) * nanos_per_tick as u64,
                ))
                / tick_diff as u32
        }
    }

    fn try_load_chunks<R: RangeBounds<u64>, A: DeserializeOwned>(
        demo: &DemoContainer,
        tick_range: R,
        rev: bool,
        cur_data: &mut BTreeMap<u64, A>,
        index: &BTreeMap<u64, u64>,
    ) {
        let mut it = index.range(tick_range);
        if let Some((_, chunk_byte_offset)) = if rev { it.next_back() } else { it.next() }
            && let Ok(mut chunks) = Self::read_chunks::<A>(demo, *chunk_byte_offset as usize)
        {
            cur_data.append(&mut chunks);
        }
    }

    fn check_chunks<A: DeserializeOwned>(
        demo: &DemoContainer,
        cur_data: &mut BTreeMap<u64, A>,
        index: &BTreeMap<u64, u64>,
        monotonic_tick: u64,
    ) {
        // try to load chunks if needed
        let first_tick = cur_data
            .first_key_value()
            .map_or(u64::MAX, |(&tick, _)| tick);
        if first_tick > monotonic_tick {
            Self::try_load_chunks(demo, ..=monotonic_tick, true, cur_data, index)
        }
        let last_tick = cur_data.last_key_value().map_or(0, |(&tick, _)| tick);
        if last_tick < monotonic_tick + 1 {
            Self::try_load_chunks(demo, last_tick + 1.., false, cur_data, index)
        }

        // we want exactly one chunk before the current tick
        // this allows the second chunks to be used for the tick
        // after this one.
        while cur_data.range(0..=monotonic_tick).count() > 1 {
            cur_data.pop_first();
        }
    }
}

struct DemoEncoder {
    enc: AvEncoder,
    settings: DemoVideoEncodeProperties,

    // offscreen params come last
    offscreen_canvas: OffscreenCanvas,
    _offair_id: u128,
}

pub struct DemoStaticData {
    canvas_handle: GraphicsCanvasHandle,
    stream_handle: GraphicsStreamHandle,

    offscreen_canvas: OffscreenCanvas,

    av_encoder: Option<DemoEncoder>,

    config_map: ConfigMap,
}

#[derive(Debug, Clone)]
pub struct DemoViewerSettings {
    pub render: ConfigRender,
    pub snd: ConfigSoundRender,
    pub global_sound_volume: f64,
}

#[derive(Debug, Clone)]
pub struct DemoVideoEncodeProperties {
    pub file_name: PathBuf,
    pub pixels_per_point: f64,
    pub encoder_settings: EncoderSettings,
    pub settings: DemoViewerSettings,
}

pub struct DemoViewerImpl {
    data: DemoStaticData,

    client_map: ClientMapLoading,
    preview_client_map: ClientMapLoading,

    inner: DemoViewerInner,
    preview: DemoViewerInner,
    should_show_preview: Option<Rect>,

    sub_dir: PathBuf,
    demo_name: String,

    io: Io,

    time: SteadyClock,
    last_time: Option<Duration>,
    last_monotonic_tick: Option<GameTickType>,

    demo_ui: DemoPlayerUiRender,

    events: Vec<DemoViewerEvent>,
    ui_state: DemoViewerUiState,
}

impl DemoViewerImpl {
    fn new(
        client_map: ClientMapLoading,
        preview_client_map: ClientMapLoading,
        graphics: &Graphics,
        backend: &Rc<GraphicsBackend>,
        sound_backend: &Rc<SoundBackend>,
        demo: DemoContainer,
        time: &SteadyClock,
        io: Io,
        ui_creator: &UiCreator,
        encode_to_video: Option<DemoVideoEncodeProperties>,
        sub_dir: PathBuf,
        name: String,
        encoder_offair_id: Option<u128>,
    ) -> Self {
        Self {
            data: DemoStaticData {
                canvas_handle: graphics.canvas_handle.clone(),
                stream_handle: graphics.stream_handle.clone(),

                offscreen_canvas: { graphics.canvas_handle.offscreen_canvas(64, 64, 0.25, None) },

                av_encoder: encode_to_video.zip(encoder_offair_id).map(
                    |(props, encoder_offair_id)| {
                        let file_name = io.fs.get_save_path().join(&props.file_name);
                        let offscreen_canvas = graphics.canvas_handle.offscreen_canvas(
                            props.encoder_settings.width,
                            props.encoder_settings.height,
                            props.pixels_per_point,
                            None,
                        );

                        DemoEncoder {
                            enc: AvEncoder::new(
                                offscreen_canvas.get_index_unsafe(),
                                encoder_offair_id,
                                &file_name,
                                backend,
                                sound_backend,
                                props.encoder_settings.clone(),
                            )
                            .unwrap(),
                            settings: props,
                            offscreen_canvas,
                            _offair_id: encoder_offair_id,
                        }
                    },
                ),
                config_map: Default::default(),
            },

            client_map,
            preview_client_map,

            sub_dir,
            demo_name: name,

            io: io.clone(),
            time: time.clone(),
            last_time: None,
            last_monotonic_tick: None,

            preview: DemoViewerInner {
                demo: demo.clone(),

                cur_snapshots: Default::default(),
                cur_events: Default::default(),

                cur_time: Duration::ZERO,

                is_closed: false,
                // Always paused
                is_paused: true,
                speed: ffixed::from_num(1.0),
            },
            should_show_preview: None,
            inner: DemoViewerInner {
                demo,

                cur_snapshots: Default::default(),
                cur_events: Default::default(),

                cur_time: Duration::ZERO,

                is_closed: false,
                is_paused: false,
                speed: ffixed::from_num(1.0),
            },

            demo_ui: DemoPlayerUiRender::new(graphics, ui_creator),

            events: Default::default(),
            ui_state: Default::default(),
        }
    }

    pub fn is_finished(&self) -> bool {
        self.inner.is_finished()
    }

    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }

    fn set_time_and_reset_state(
        client_map: &mut ClientMapLoading,
        inner: &mut DemoViewerInner,
        time: Duration,
    ) {
        inner.set_time_and_clear_chunks(time);

        let map = client_map.try_get_mut().unwrap();

        let ClientMapFile::Game(map) = map else {
            panic!("not a game map")
        };
        let GameMap { render, .. } = map.as_mut();
        render.clear_render_state();
    }

    fn prepare_render_input<'a>(
        viewer: &mut DemoViewerInner,
        data: &mut DemoStaticData,
        client_map: &'a mut ClientMapLoading,
        cur_time: Duration,
        last_time: Duration,
        last_monotonic_tick: &mut Option<GameTickType>,
        for_video_encode: bool,
        config_render: &ConfigRender,
        config_snd: &ConfigSoundRender,
        global_sound_volume: f64,
    ) -> anyhow::Result<(&'a mut RenderGameWasmManager, RenderGameInput)> {
        if !viewer.is_paused() && !viewer.is_finished() {
            viewer.cur_time += Duration::from_secs_f64(
                (cur_time.saturating_sub(last_time).as_secs_f64() * viewer.speed.to_num::<f64>())
                    .clamp(0.0, f64::MAX),
            );
        }

        let monotonic_tick = viewer.time_to_tick();

        DemoViewerInner::check_chunks(
            &viewer.demo,
            &mut viewer.cur_snapshots,
            &viewer.demo.tail.snapshots_index,
            monotonic_tick,
        );
        DemoViewerInner::check_chunks(
            &viewer.demo,
            &mut viewer.cur_events,
            &viewer.demo.tail.events_index,
            monotonic_tick,
        );

        let map = client_map.try_get_mut().unwrap();

        let ClientMapFile::Game(map) = map else {
            panic!("not a game map")
        };
        let GameMap { render, game, .. } = map.as_mut();

        let Some((local_players, prev_tick, next_tick)) = (if !viewer.cur_snapshots.is_empty() {
            let mut it = viewer.cur_snapshots.iter();

            if let Some((&prev_tick, snap)) = it.next() {
                let mut snapshot = MtPoolCow::new_without_pool();
                snapshot.to_mut().extend(snap.iter());
                game.build_from_snapshot_for_prev(&snapshot);
                let (local_players, next_tick) = if let Some((&next_tick, snap)) = it.next() {
                    let mut snapshot = MtPoolCow::new_without_pool();
                    snapshot.to_mut().extend(snap.iter());
                    (game.build_from_snapshot(&snapshot), next_tick)
                } else {
                    (game.build_from_snapshot(&snapshot), prev_tick)
                };
                Some((local_players, prev_tick, next_tick))
            } else {
                None
            }
        } else {
            None
        }) else {
            return Err(anyhow!("no snapshots inside this demo"));
        };

        let (player_id, _) = local_players.iter().next().unwrap();
        let intra_tick_time = viewer.intra_tick_time(monotonic_tick, prev_tick, next_tick);

        let render_for_player = RenderForPlayer {
            chat_info: None,
            emote_wheel_input: None,
            spectator_selection_input: None,
            scoreboard_active: false,
            chat_show_all: false,

            local_player_info: game.collect_character_local_render_info(player_id),

            zoom: 1.0,
            cam_mode: RenderPlayerCameraMode::Default,
        };

        let game_time_info = GameTimeInfo {
            ticks_per_second: game.game_tick_speed(),
            intra_tick_time,
        };
        let intra_tick_ratio =
            intra_tick_time_to_ratio(intra_tick_time, viewer.demo.header_ext.ticks_per_second);

        let character_infos = game.collect_characters_info();

        let stages = game.all_stages(intra_tick_ratio);

        let scoreboard_info = None; // game.collect_scoreboard_info();

        let load_events = !last_monotonic_tick.is_some_and(|tick| tick == monotonic_tick);
        last_monotonic_tick.replace(monotonic_tick);
        let (events, chat_msgs) = if load_events {
            let demo_events = viewer
                .cur_events
                .get(&monotonic_tick)
                .cloned()
                .unwrap_or_default();

            let mut events = PoolBTreeMap::new_without_pool();
            let mut chat_msgs = PoolVecDeque::new_without_pool();
            for demo_event in demo_events {
                match demo_event {
                    DemoEvent::Game(evs) => {
                        events.insert((monotonic_tick, false), evs);
                    }
                    DemoEvent::Chat(msg) => {
                        chat_msgs.push_back(*msg);
                    }
                    DemoEvent::Marker => {
                        // ignore
                    }
                }
            }
            (events, chat_msgs)
        } else {
            (
                PoolBTreeMap::new_without_pool(),
                PoolVecDeque::new_without_pool(),
            )
        };

        let mut render_game_input = RenderGameInput {
            players: PoolFxLinkedHashMap::new_without_pool(),
            dummies: PoolFxLinkedHashSet::new_without_pool(),
            events,
            vote: None,
            character_infos,
            stages,
            scoreboard_info,
            chat_msgs,
            game_time_info,
            settings: if let Some(DemoEncoder {
                settings: encoder_game_settings,
                ..
            }) = for_video_encode
                .then_some(data.av_encoder.as_ref())
                .flatten()
            {
                RenderGameSettings::new(
                    &encoder_game_settings.settings.render,
                    &encoder_game_settings.settings.snd,
                    encoder_game_settings.pixels_per_point as f32,
                    viewer.speed.to_num::<f64>(),
                    false,
                    encoder_game_settings.settings.global_sound_volume,
                )
            } else {
                RenderGameSettings::new(
                    config_render,
                    config_snd,
                    data.canvas_handle.pixels_per_point(),
                    viewer.speed.to_num::<f64>(),
                    false,
                    global_sound_volume,
                )
            },
            ext: game.collect_render_ext(),
        };

        let render_for_player = RenderGameForPlayer {
            render_for_player,
            observed_players: PoolVec::new_without_pool(),
            observed_anchored_size_props: Default::default(),
        };

        render_game_input
            .players
            .insert(*player_id, render_for_player);

        if let Some(canvas) = for_video_encode
            .then(|| {
                data.av_encoder.as_ref().map(
                    |DemoEncoder {
                         offscreen_canvas, ..
                     }| offscreen_canvas,
                )
            })
            .flatten()
        {
            data.canvas_handle
                .switch_canvas(GraphicsCanvasMode::Offscreen {
                    offscreen_canvas: canvas.clone(),
                });
        }

        Ok((render, render_game_input))
    }

    fn render_game(
        viewer: &mut DemoViewerInner,
        data: &mut DemoStaticData,
        client_map: &mut ClientMapLoading,
        cur_time: Duration,
        last_time: Duration,
        last_monotonic_tick: &mut Option<GameTickType>,
        for_video_encode: bool,
        conf_render: &ConfigRender,
        conf_snd: &ConfigSoundRender,
        global_sound_volume: f64,
    ) -> anyhow::Result<()> {
        let (render, render_game_input) = Self::prepare_render_input(
            viewer,
            data,
            client_map,
            cur_time,
            last_time,
            last_monotonic_tick,
            for_video_encode,
            conf_render,
            conf_snd,
            global_sound_volume,
        )?;

        render.render(&data.config_map, &viewer.cur_time, render_game_input);

        if let Some(settings) = for_video_encode
            .then(|| data.av_encoder.as_ref().map(|e| &e.settings))
            .flatten()
        {
            data.canvas_handle
                .switch_canvas(GraphicsCanvasMode::Onscreen);

            let num_samples = settings.encoder_settings.sample_rate / settings.encoder_settings.fps;
            render.render_offair_sound(num_samples);
        }

        Ok(())
    }

    #[instrument(level = "trace", skip_all)]
    pub fn render(
        &mut self,
        input: egui::RawInput,
        render: &ConfigRender,
        snd: &ConfigSoundRender,
        global_sound_volume: f64,
    ) -> anyhow::Result<()> {
        let do_encoding = self.data.av_encoder.is_some();
        let (cur_time, last_time) = if let Some(DemoEncoder {
            enc,
            settings,
            offscreen_canvas,
            ..
        }) = &self.data.av_encoder
        {
            // skip this frame
            if enc.overloaded() {
                offscreen_canvas.skip_fetching_once();
                return Ok(());
            }
            let cur_time = self.last_time.unwrap_or_default()
                + Duration::from_nanos(
                    (Duration::from_secs(1).as_nanos() / settings.encoder_settings.fps as u128)
                        as u64,
                );
            (
                cur_time,
                self.last_time.replace(cur_time).unwrap_or_default(),
            )
        } else {
            let cur_time = self.time.now();
            (
                cur_time,
                self.last_time.replace(cur_time).unwrap_or(cur_time),
            )
        };
        Self::render_game(
            &mut self.inner,
            &mut self.data,
            &mut self.client_map,
            cur_time,
            last_time,
            &mut self.last_monotonic_tick,
            do_encoding,
            render,
            snd,
            global_sound_volume,
        )?;
        self.demo_ui.render(
            &mut DemoPlayerUiRenderPipe {
                cur_time: &self.time.now(),
                player_info: UserData {
                    stream_handle: &self.data.stream_handle,
                    canvas_handle: &self.data.canvas_handle,
                    is_paused: &self.inner.is_paused(),
                    cur_duration: &self.inner.cur_time,
                    max_duration: &self.inner.demo.header.len,
                    speed: &self.inner.speed,
                    events: &mut self.events,
                    state: &mut self.ui_state,
                    name: &self.demo_name,
                },
            },
            input,
        );

        self.should_show_preview = None;

        // handle events after demo
        for event in self.events.drain(..) {
            match event {
                DemoViewerEvent::ResumeToggle => {
                    self.inner.is_paused = !self.inner.is_paused;
                }
                DemoViewerEvent::Stop => {
                    self.inner.is_paused = true;
                    Self::set_time_and_reset_state(
                        &mut self.client_map,
                        &mut self.inner,
                        Duration::ZERO,
                    );
                }
                DemoViewerEvent::BackwardFast => {
                    Self::set_time_and_reset_state(
                        &mut self.client_map,
                        &mut self.inner,
                        Duration::ZERO,
                    );
                }
                DemoViewerEvent::ForwardFast => {
                    let time = self.inner.demo.header.len;
                    Self::set_time_and_reset_state(&mut self.client_map, &mut self.inner, time);
                }
                DemoViewerEvent::BackwardStep => {
                    self.inner.cur_time = self.inner.cur_time.saturating_sub(Duration::from_nanos(
                        (Duration::from_secs(1).as_nanos()
                            / self.inner.demo.header_ext.ticks_per_second.get() as u128)
                            as u64,
                    ));
                }
                DemoViewerEvent::ForwardStep => {
                    self.inner.cur_time = self.inner.cur_time.saturating_add(Duration::from_nanos(
                        (Duration::from_secs(1).as_nanos()
                            / self.inner.demo.header_ext.ticks_per_second.get() as u128)
                            as u64,
                    ));
                }
                DemoViewerEvent::Backward => todo!(),
                DemoViewerEvent::Forward => todo!(),
                DemoViewerEvent::SpeedSlower => {
                    self.inner.speed /= ffixed::from_num(2);
                    self.inner.speed = self
                        .inner
                        .speed
                        .clamp(ffixed::from_num(0.03125), ffixed::from_num(4096.0));
                }
                DemoViewerEvent::SpeedFaster => {
                    self.inner.speed *= ffixed::from_num(2);
                    self.inner.speed = self
                        .inner
                        .speed
                        .clamp(ffixed::from_num(0.03125), ffixed::from_num(4096.0));
                }
                DemoViewerEvent::SpeedReset => {
                    self.inner.speed = ffixed::from_num(1.0);
                }
                DemoViewerEvent::Export(data) => {
                    let demo = &self.inner.demo;
                    let ext = &demo.header_ext;
                    let mut recorder = DemoRecorder::new(
                        DemoRecorderCreateProps {
                            base: DemoRecorderCreatePropsBase {
                                map: ext.map.clone(),
                                map_hash: ext.map_hash,
                                game_options: ext.game_options.clone(),
                                required_resources: ext.required_resources.clone(),
                                client_local_infos: ext.client_local_infos.clone(),
                                physics_module: ext.physics_mod.clone(),
                                render_module: ext.render_mod.clone(),
                                physics_group_name: ext.physics_group_name.clone(),
                            },
                            io: self.io.clone(),
                            in_memory: None,
                        },
                        ext.ticks_per_second,
                        Some(&self.sub_dir),
                        Some(data.name),
                    );
                    self.preview.set_time_and_clear_chunks(data.left);

                    let last_monotonic_tick = self.preview.time_to_tick_impl(data.right);
                    let mut monotonic_tick = self.preview.time_to_tick();

                    while monotonic_tick <= last_monotonic_tick {
                        DemoViewerInner::check_chunks(
                            &self.preview.demo,
                            &mut self.preview.cur_snapshots,
                            &self.preview.demo.tail.snapshots_index,
                            monotonic_tick,
                        );
                        DemoViewerInner::check_chunks(
                            &self.preview.demo,
                            &mut self.preview.cur_events,
                            &self.preview.demo.tail.events_index,
                            monotonic_tick,
                        );

                        if let Some(snapshot) = self.preview.cur_snapshots.get(&monotonic_tick) {
                            recorder.add_snapshot(monotonic_tick, snapshot.clone());
                        }
                        if let Some(events) = self.preview.cur_events.get(&monotonic_tick) {
                            for event in events
                                .iter()
                                .filter(|ev| !matches!(ev, DemoEvent::Chat(_)) || !data.remove_chat)
                            {
                                recorder.add_event(monotonic_tick, event.clone());
                            }
                        }

                        monotonic_tick += 1;
                    }
                }
                DemoViewerEvent::SkipTo { time } => {
                    Self::set_time_and_reset_state(&mut self.client_map, &mut self.inner, time);
                }
                DemoViewerEvent::PreviewAt { rect, time } => {
                    Self::set_time_and_reset_state(
                        &mut self.preview_client_map,
                        &mut self.preview,
                        time,
                    );
                    self.should_show_preview = Some(rect);
                }
                DemoViewerEvent::Close => {
                    self.inner.is_closed = true;
                }
            }
        }

        if let Some(rect) = self.should_show_preview {
            let w = self.data.offscreen_canvas.width();
            let h = self.data.offscreen_canvas.height();
            let ppp = self.data.offscreen_canvas.pixels_per_point();

            if w != rect.width() as u32 || h != rect.height() as u32 || ppp != 0.25 {
                self.data.offscreen_canvas = self.data.canvas_handle.offscreen_canvas(
                    rect.width() as u32,
                    rect.height() as u32,
                    0.25,
                    None,
                );
            }

            self.data
                .canvas_handle
                .switch_canvas(GraphicsCanvasMode::Offscreen {
                    offscreen_canvas: self.data.offscreen_canvas.clone(),
                });

            let mut render = render.clone();
            render.ingame_ui_min_pixels_per_point = 0.1;
            render.ingame_ui_scale = 1.0;

            Self::render_game(
                &mut self.preview,
                &mut self.data,
                &mut self.preview_client_map,
                Duration::ZERO,
                Duration::ZERO,
                &mut Default::default(),
                false,
                &render,
                snd,
                global_sound_volume,
            )?;
            self.data
                .canvas_handle
                .switch_canvas(GraphicsCanvasMode::Onscreen);

            let mut state = State::new();
            state.map_canvas(
                0.0,
                0.0,
                self.data.canvas_handle.canvas_width() as f32,
                self.data.canvas_handle.canvas_height() as f32,
            );
            state.set_color_mask(ColorMaskMode::WriteColorOnly);
            state.blend(BlendType::None);

            let rect = &rect;
            let offscreen_canvas = &self.data.offscreen_canvas;
            self.data.stream_handle.render_quads(
                &[StreamedQuad::default()
                    .from_pos_and_size(
                        vec2::new(rect.left_top().x, rect.left_top().y),
                        vec2::new(rect.width(), rect.height()),
                    )
                    .color(ubvec4::new(255, 255, 255, 255))
                    .tex_free_form(
                        vec2::new(0.0, 0.0),
                        vec2::new(1.0, 0.0),
                        vec2::new(1.0, 1.0),
                        vec2::new(0.0, 1.0),
                    )],
                state,
                TextureType::ColorAttachmentOfOffscreen(offscreen_canvas.clone()),
            );
        }

        // video encoding finished.
        if self.data.av_encoder.is_some() && self.is_finished() {
            self.inner.is_closed = true;
        }

        Ok(())
    }

    /// Simply set's the time to the given time.
    ///
    /// Does not reset any rendering state or clears unused chunks.
    fn set_time_dirty(&mut self, time: Duration) {
        self.inner.cur_time = time;
    }

    pub fn get_render_input_for_time(&mut self, time: Duration) -> anyhow::Result<RenderGameInput> {
        self.set_time_dirty(time);
        let (_, render_input) = Self::prepare_render_input(
            &mut self.inner,
            &mut self.data,
            &mut self.client_map,
            time,
            time,
            &mut self.last_monotonic_tick,
            false,
            &Default::default(),
            &Default::default(),
            0.3,
        )?;
        Ok(render_input)
    }

    /// Whether an encoder is active in this demo viewer
    pub fn is_encoder(&self) -> bool {
        self.data.av_encoder.is_some()
    }
}

pub struct DemoViewerLoading {
    pub task: IoRuntimeTask<DemoContainer>,
    pub io: Io,
    pub thread_pool: Arc<rayon::ThreadPool>,
    pub fonts: FontDefinitions,
    encode_to_video: Option<DemoVideoEncodeProperties>,
    sub_dir: PathBuf,
    name: String,
}

pub struct DemoViewerLoadingComponents {
    pub client_map: ClientMapLoading,
    pub preview_client_map: ClientMapLoading,
    pub demo: DemoContainer,
    pub io: Io,
    encode_to_video: Option<DemoVideoEncodeProperties>,
    sub_dir: PathBuf,
    name: String,

    encoder_offair_id: Option<u128>,
}

pub enum DemoViewer {
    Loading(Box<DemoViewerLoading>),
    LoadingComponents(Box<DemoViewerLoadingComponents>),
    Rendering(Box<DemoViewerImpl>),
    None,
}

impl DemoViewer {
    fn new_impl(
        io: &Io,
        thread_pool: &Arc<rayon::ThreadPool>,
        sub_dir: PathBuf,
        name: String,
        fonts: FontDefinitions,
        encode_to_video: Option<DemoVideoEncodeProperties>,
        read_demo: IoRuntimeTask<Vec<u8>>,
    ) -> Self {
        let read_demo = io.rt.then(read_demo, |demo| async move {
            let mut writer: Vec<u8> = Default::default();

            // read header
            let (header, file_off): (DemoHeader, usize) = deser_ex(&demo, true)?;
            let demo = &demo[file_off..];

            // read header ext
            let (header_ext, _): (DemoHeaderExt, usize) =
                deser(decomp(&demo[0..header.size_ext as usize], &mut writer)?)?;

            let demo = &demo[header.size_ext as usize..];
            let chunks = &demo[0..header.size_chunks as usize];
            let tail = &demo[header.size_chunks as usize..];

            // read tail
            let (tail, _): (DemoTail, usize) = deser(decomp(tail, &mut writer)?)?;
            anyhow::ensure!(
                !tail.snapshots_index.is_empty(),
                "no snapshot index found in demo tail."
            );

            if tail
                .events_index
                .first_key_value()
                .zip(tail.snapshots_index.first_key_value())
                .is_some_and(|((events_index, _), (snapshot_index, _))| {
                    *events_index + header_ext.ticks_per_second.get() < *snapshot_index
                })
                || tail
                    .events_index
                    .last_key_value()
                    .zip(tail.snapshots_index.last_key_value())
                    .is_some_and(|((events_index, _), (snapshot_index, _))| {
                        *events_index > *snapshot_index + header_ext.ticks_per_second.get()
                    })
            {
                log::warn!(
                    "Events index & snapshots index are far apart.\
                    The demo might have been recorded while having huge lags.",
                )
            }

            // read all chunks
            Ok(DemoContainer {
                header,
                header_ext,
                demo_chunks: chunks.to_vec(),
                tail,
            })
        });
        Self::Loading(Box::new(DemoViewerLoading {
            task: read_demo,
            io: io.clone(),
            thread_pool: thread_pool.clone(),
            fonts,
            encode_to_video,
            sub_dir,
            name,
        }))
    }

    pub fn new(
        io: &Io,
        thread_pool: &Arc<rayon::ThreadPool>,
        demo_path: &Path,
        fonts: FontDefinitions,
        encode_to_video: Option<DemoVideoEncodeProperties>,
    ) -> Self {
        let fs = io.fs.clone();
        let demo_path_thread = demo_path.to_path_buf();
        let read_demo = io
            .rt
            .spawn(async move { Ok(fs.read_file(&demo_path_thread).await?) });

        let demo_name = demo_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let sub_dir = demo_path
            .parent()
            .map(|p| {
                let mut comp = p.components().peekable();
                if comp.peek().is_some_and(|c| c.as_os_str() == "demos") {
                    comp.next();
                }
                comp.collect()
            })
            .unwrap_or_default();

        Self::new_impl(
            io,
            thread_pool,
            sub_dir,
            demo_name,
            fonts,
            encode_to_video,
            read_demo,
        )
    }

    pub fn new_from_file(
        io: &Io,
        thread_pool: &Arc<rayon::ThreadPool>,
        name: Cow<str>,
        fonts: FontDefinitions,
        encode_to_video: Option<DemoVideoEncodeProperties>,
        demo: Vec<u8>,
    ) -> Self {
        let read_demo = io.rt.spawn(async move { Ok(demo) });
        Self::new_impl(
            io,
            thread_pool,
            Default::default(),
            name.to_string(),
            fonts,
            encode_to_video,
            read_demo,
        )
    }

    pub fn try_get(&self) -> Option<&DemoViewerImpl> {
        if let Self::Rendering(viewer) = self {
            Some(viewer)
        } else {
            None
        }
    }

    pub fn try_get_mut(&mut self) -> Option<&mut DemoViewerImpl> {
        if let Self::Rendering(viewer) = self {
            Some(viewer)
        } else {
            None
        }
    }

    #[instrument(level = "trace", skip_all)]
    pub fn continue_loading(
        &mut self,
        sound: &SoundManager,
        graphics: &Graphics,
        backend: &Rc<GraphicsBackend>,
        sound_backend: &Rc<SoundBackend>,
        config: &ConfigEngine,
        config_game: &ConfigGame,
        time: &SteadyClock,
        ui_creator: &UiCreator,
    ) -> anyhow::Result<Option<&DemoViewerImpl>> {
        let mut dummy = DemoViewer::None;
        std::mem::swap(self, &mut dummy);
        match dummy {
            DemoViewer::Loading(loading) => {
                let DemoViewerLoading {
                    task,
                    io,
                    thread_pool,
                    fonts,
                    encode_to_video,
                    sub_dir,
                    name,
                } = *loading;
                if task.is_finished() {
                    let demo_container = task.get()?;
                    let gen_client_map = |sound_props: SoundSceneCreateProps| {
                        ClientMapLoading::new(
                            sound,
                            graphics,
                            backend,
                            time,
                            "map/maps/".as_ref(),
                            &demo_container.header_ext.map,
                            Some(demo_container.header_ext.map_hash),
                            &io,
                            &thread_pool,
                            demo_container.header_ext.physics_mod.clone(),
                            false,
                            &config.dbg,
                            demo_container.header_ext.game_options.clone(),
                            RenderGameCreateOptions {
                                physics_group_name: demo_container
                                    .header_ext
                                    .physics_group_name
                                    .clone(),
                                resource_http_download_url: Some(
                                    HTTP_RESOURCE_URL.try_into().unwrap(),
                                ),
                                resource_download_server: None,
                                fonts: fonts.clone(),
                                sound_props,
                                render_mod: RenderModTy::render_mod(
                                    &demo_container.header_ext.render_mod,
                                    config_game,
                                ),
                                required_resources: demo_container
                                    .header_ext
                                    .required_resources
                                    .clone(),
                                client_local_infos: demo_container
                                    .header_ext
                                    .client_local_infos
                                    .clone(),
                            },
                            Default::default(),
                        )
                    };

                    let demo_offair_id = sound.scene_handle.next_offair_id();
                    let encoder_offair_id = encode_to_video
                        .is_some()
                        .then(|| sound.scene_handle.next_offair_id());
                    *self = Self::LoadingComponents(Box::new(DemoViewerLoadingComponents {
                        client_map: gen_client_map(
                            if let Some((settings, encoder_offair_id)) =
                                encode_to_video.as_ref().zip(encoder_offair_id)
                            {
                                SoundSceneCreateProps {
                                    air_mode: SceneAirMode::OffAir {
                                        id: encoder_offair_id,
                                        sample_rate: settings.encoder_settings.sample_rate,
                                    },
                                }
                            } else {
                                SoundSceneCreateProps::default()
                            },
                        ),
                        preview_client_map: gen_client_map(SoundSceneCreateProps {
                            air_mode: SceneAirMode::OffAir {
                                id: demo_offair_id,
                                sample_rate: 48000,
                            },
                        }),
                        demo: demo_container,
                        io,
                        encode_to_video,
                        sub_dir,
                        name,

                        encoder_offair_id,
                    }));
                } else {
                    *self = Self::Loading(Box::new(DemoViewerLoading {
                        task,
                        io,
                        thread_pool,
                        fonts,
                        encode_to_video,
                        sub_dir,
                        name,
                    }));
                }
            }
            DemoViewer::LoadingComponents(loading) => {
                let DemoViewerLoadingComponents {
                    mut client_map,
                    mut preview_client_map,
                    demo,
                    io,
                    encode_to_video,
                    sub_dir,
                    name,
                    encoder_offair_id,
                } = *loading;
                if client_map.continue_loading().is_some()
                    && preview_client_map.continue_loading().is_some()
                {
                    // finished loading client
                    *self = Self::Rendering(Box::new(DemoViewerImpl::new(
                        client_map,
                        preview_client_map,
                        graphics,
                        backend,
                        sound_backend,
                        demo,
                        time,
                        io,
                        ui_creator,
                        encode_to_video,
                        sub_dir,
                        name,
                        encoder_offair_id,
                    )));
                } else {
                    *self = Self::LoadingComponents(Box::new(DemoViewerLoadingComponents {
                        client_map,
                        preview_client_map,
                        demo,
                        io,
                        encode_to_video,
                        sub_dir,
                        name,
                        encoder_offair_id,
                    }));
                }
            }
            DemoViewer::Rendering(viewer) => {
                *self = Self::Rendering(viewer);
            }
            DemoViewer::None => {}
        }
        Ok(self.try_get())
    }

    /// Whether encoder is active in this demo viewer
    pub fn is_encoder(&self) -> bool {
        if let Self::Rendering(viewer) = self {
            viewer.is_encoder()
        } else {
            false
        }
    }
}
