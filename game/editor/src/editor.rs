use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use anyhow::anyhow;
use base::{
    hash::{fmt_hash, Hash},
    join_all,
    linked_hash_map_view::FxLinkedHashMap,
    system::{System, SystemTimeInterface},
};
use base_io::{io::Io, runtime::IoRuntimeTask};
use base_io_traits::fs_traits::FileSystemInterface;
use client_containers::entities::{EntitiesContainer, ENTITIES_CONTAINER_PATH};
use client_notifications::overlay::ClientNotifications;
use client_render_base::map::{
    map::{ForcedTexture, RenderMap},
    map_buffered::{
        ClientMapBufferQuadLayer, MapBufferPhysicsTileLayer, MapBufferTileLayer, SoundLayerSounds,
    },
    render_pipe::Camera,
};
use config::config::ConfigEngine;
use ed25519_dalek::pkcs8::spki::der::Encode;
use egui::{pos2, vec2, FontDefinitions, InputState, OutputCommand, Pos2, Rect};
use game_config::config::ConfigMap;
use game_interface::types::game::GameTickType;
use graphics::{
    graphics::graphics::Graphics,
    graphics_mt::GraphicsMultiThreaded,
    handles::{
        backend::backend::GraphicsBackendHandle,
        buffer_object::buffer_object::GraphicsBufferObjectHandle,
        canvas::canvas::GraphicsCanvasHandle,
        stream::stream::GraphicsStreamHandle,
        texture::texture::{GraphicsTextureHandle, TextureContainer, TextureContainer2dArray},
    },
};
use graphics_types::{commands::TexFlags, types::GraphicsMemoryAllocationType};
use hiarc::HiarcTrait;
use image_utils::{png::load_png_image_as_rgba, utils::texture_2d_to_3d};
use map::{
    map::{
        animations::{AnimBase, AnimPoint, AnimPointCurveType},
        config::Config,
        groups::{
            layers::{
                design::{MapLayer, MapLayerQuad, MapLayerSound, MapLayerTile},
                physics::{MapLayerPhysics, MapLayerTilePhysicsBase},
                tiles::{MapTileLayerPhysicsTilesRef, TileBase},
            },
            MapGroup, MapGroupAttr, MapGroupPhysicsAttr,
        },
        metadata::Metadata,
        resources::MapResourceMetaData,
        Map,
    },
    skeleton::{
        animations::{AnimBaseSkeleton, AnimationsSkeleton},
        groups::layers::{
            design::MapLayerSkeleton,
            physics::{
                MapLayerArbitraryPhysicsSkeleton, MapLayerSwitchPhysicsSkeleton,
                MapLayerTelePhysicsSkeleton, MapLayerTilePhysicsBaseSkeleton,
                MapLayerTunePhysicsSkeleton,
            },
        },
    },
    types::NonZeroU16MinusOne,
};
use math::math::vector::{ffixed, fvec2, ubvec4, vec2};
use network::network::types::{
    NetworkClientCertCheckMode, NetworkServerCertAndKey, NetworkServerCertMode,
    NetworkServerCertModeResult,
};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sound::{
    scene_handle::SoundSceneHandle, scene_object::SceneObject, sound::SoundManager,
    sound_mt::SoundMultiThreaded,
};
use ui_base::ui::UiCreator;

use crate::{
    client::EditorClient,
    editor_ui::{EditorUiRender, EditorUiRenderPipe},
    event::EditorEventOverwriteMap,
    fs::read_file_editor,
    hotkeys::{BindsPerEvent, EditorBindsFile, EditorHotkeyEvent},
    image_store_container::{load_image_store_container, ImageStoreContainer},
    map::{
        EditorActiveAnimationProps, EditorAnimations, EditorAnimationsProps,
        EditorArbitraryLayerProps, EditorColorAnimation, EditorCommonGroupOrLayerAttr,
        EditorCommonLayerOrGroupAttrInterface, EditorConfig, EditorGroup, EditorGroupPhysics,
        EditorGroupProps, EditorGroups, EditorGroupsProps, EditorImage, EditorImage2dArray,
        EditorLayer, EditorLayerArbitrary, EditorLayerQuad, EditorLayerSound, EditorLayerTile,
        EditorLayerUnionRef, EditorMap, EditorMapInterface, EditorMapProps, EditorMetadata,
        EditorPhysicsGroupProps, EditorPhysicsLayer, EditorPhysicsLayerProps, EditorPosAnimation,
        EditorQuadLayerProps, EditorQuadLayerPropsPropsSelection, EditorResource, EditorResources,
        EditorSound, EditorSoundAnimation, EditorSoundLayerProps, EditorTileLayerProps,
        EditorTileLayerPropsSelection, ResourceSelection,
    },
    map_tools::{
        finish_design_quad_layer_buffer, finish_design_tile_layer_buffer,
        finish_physics_layer_buffer, upload_design_quad_layer_buffer,
        upload_design_tile_layer_buffer, upload_physics_layer_buffer,
    },
    notifications::{EditorNotification, EditorNotifications},
    options::EditorOptions,
    physics_layers::PhysicsLayerOverlaysDdnet,
    server::EditorServer,
    sound_store_container::{load_sound_store_container, SoundStoreContainer},
    tab::EditorTab,
    tile_overlays::TileLayerOverlaysDdnet,
    tools::{
        auto_saver::AutoSaver,
        quad_layer::{brush::QuadBrush, selection::QuadSelection},
        sound_layer::brush::SoundBrush,
        tile_layer::{
            auto_mapper::TileLayerAutoMapper, brush::TileBrush, selection::TileSelection,
        },
        tool::{
            ActiveTool, ActiveToolQuads, ActiveToolSounds, ActiveToolTiles, ToolQuadLayer,
            ToolSoundLayer, ToolTileLayer, Tools,
        },
        utils::{render_rect, render_rect_from_state, render_rect_state},
    },
    ui::user_data::{
        EditorMenuDialogMode, EditorModalDialogMode, EditorTabsRefMut, EditorUiEvent,
        EditorUiEventHostMap,
    },
    utils::{ui_pos_to_world_pos, UiCanvasSize},
};

#[derive(Debug, PartialEq, Clone, Copy)]
enum ReadFileTy {
    Image,
    Sound,
}

#[derive(Debug, Default)]
struct MapLoadWithServerOptions {
    cert: Option<NetworkServerCertMode>,
    port: Option<u16>,
    password: Option<String>,
    mapper_name: Option<String>,
    color: Option<[u8; 3]>,
    /// If `Some` allows to change server settings remotely.
    admin_password: Option<String>,
}

#[derive(Debug)]
enum MapLoadOptions {
    WithServer(MapLoadWithServerOptions),
    WithoutServer {
        server_addr: String,
        cert_hash: Hash,
        password: String,
        mapper_name: String,
        color: [u8; 3],
    },
}

impl Default for MapLoadOptions {
    fn default() -> Self {
        Self::WithServer(MapLoadWithServerOptions {
            cert: None,
            port: None,
            password: None,
            mapper_name: None,
            color: None,
            admin_password: None,
        })
    }
}

/// this is basically the editor client
pub struct Editor {
    tabs: FxLinkedHashMap<String, EditorTab>,
    active_tab: String,
    sys: System,

    ui: EditorUiRender,
    // events triggered by ui
    ui_events: Vec<EditorUiEvent>,

    quad_tile_images_container: ImageStoreContainer,
    sounds_container: SoundStoreContainer,

    // editor tool
    tools: Tools,
    auto_mapper: TileLayerAutoMapper,

    editor_options: EditorOptions,

    hotkeys: EditorBindsFile,
    cur_hotkey_events: HashSet<EditorHotkeyEvent>,
    cached_binds_per_event: Option<BindsPerEvent>,

    middle_down_pointer_pos: Option<egui::Pos2>,
    current_pointer_pos: egui::Pos2,
    current_scroll_delta: egui::Vec2,
    latest_pointer: egui::PointerState,
    latest_keys_down: HashSet<egui::Key>,
    latest_modifiers: egui::Modifiers,
    latest_canvas_rect: egui::Rect,
    latest_unused_rect: egui::Rect,
    last_time: Duration,

    // notifications
    notifications: EditorNotifications,
    notifications_overlay: ClientNotifications,

    // graphics
    graphics_mt: GraphicsMultiThreaded,
    buffer_object_handle: GraphicsBufferObjectHandle,
    backend_handle: GraphicsBackendHandle,
    texture_handle: GraphicsTextureHandle,
    canvas_handle: GraphicsCanvasHandle,
    stream_handle: GraphicsStreamHandle,

    // sound
    sound_mt: SoundMultiThreaded,
    scene_handle: SoundSceneHandle,
    container_scene: SceneObject,

    entities_container: EntitiesContainer,
    fake_texture_array: TextureContainer2dArray,
    fake_texture: TextureContainer,
    tile_textures: Rc<TileLayerOverlaysDdnet>,

    // misc
    io: Io,
    thread_pool: Arc<rayon::ThreadPool>,

    save_tasks: Vec<IoRuntimeTask<()>>,
}

#[derive(Debug, Clone)]
struct LayerRect {
    width: NonZeroU16MinusOne,
    height: NonZeroU16MinusOne,
    parallax: fvec2,
    offset: fvec2,
}

impl Editor {
    pub fn new(
        sound: &SoundManager,
        graphics: &Graphics,
        io: &Io,
        tp: &Arc<rayon::ThreadPool>,
        font_data: &FontDefinitions,
    ) -> Self {
        let hotkeys_file = EditorBindsFile::load_file(io);

        let sys = System::new();
        let default_entities =
            EntitiesContainer::load_default(io, ENTITIES_CONTAINER_PATH.as_ref());
        let scene = sound.scene_handle.create(Default::default());
        let entities_container = EntitiesContainer::new(
            io.clone(),
            tp.clone(),
            default_entities,
            None,
            None,
            "entities-container",
            graphics,
            sound,
            &scene,
            ENTITIES_CONTAINER_PATH.as_ref(),
            Default::default(),
        );

        // fake texture array texture for non textured layers
        let mut mem = graphics.get_graphics_mt().mem_alloc(
            GraphicsMemoryAllocationType::TextureRgbaU82dArray {
                width: 1.try_into().unwrap(),
                height: 1.try_into().unwrap(),
                depth: 256.try_into().unwrap(),
                flags: TexFlags::empty(),
            },
        );
        mem.as_mut_slice().iter_mut().for_each(|byte| *byte = 255);
        // clear first tile, must stay empty
        mem.as_mut_slice()[0..4].copy_from_slice(&[0, 0, 0, 0]);

        let fake_texture_array = graphics
            .texture_handle
            .load_texture_2d_array_rgba_u8(mem, "fake-editor-texture")
            .unwrap();

        // fake texture texture for non textured quads
        let mut mem =
            graphics
                .get_graphics_mt()
                .mem_alloc(GraphicsMemoryAllocationType::TextureRgbaU8 {
                    width: 1.try_into().unwrap(),
                    height: 1.try_into().unwrap(),
                    flags: TexFlags::empty(),
                });
        mem.as_mut_slice().iter_mut().for_each(|byte| *byte = 255);

        let fake_texture = graphics
            .texture_handle
            .load_texture_rgba_u8(mem, "fake-editor-texture")
            .unwrap();

        let last_time = sys.time_get();

        let graphics_mt = graphics.get_graphics_mt();

        let mut ui_creator = UiCreator::default();
        ui_creator.load_font(font_data);

        let overlays = PhysicsLayerOverlaysDdnet::new(io, tp, graphics)
            .expect("Data files for editor are wrong");

        let mut hotkeys: EditorBindsFile = hotkeys_file.get_storage().unwrap_or_default();
        hotkeys.apply_defaults();

        let mut res = Self {
            tabs: Default::default(),
            active_tab: "".into(),

            quad_tile_images_container: load_image_store_container(
                io.clone(),
                tp.clone(),
                "quad_or_tilesets",
                graphics,
                sound,
                scene.clone(),
            ),
            sounds_container: load_sound_store_container(
                io.clone(),
                tp.clone(),
                "sounds",
                graphics,
                sound,
                scene.clone(),
            ),

            ui: EditorUiRender::new(graphics, tp.clone(), &ui_creator),
            ui_events: Default::default(),

            tools: Tools {
                tiles: ToolTileLayer {
                    brush: TileBrush::new(
                        &graphics_mt,
                        &graphics.buffer_object_handle,
                        &graphics.backend_handle,
                        &overlays,
                    ),
                    selection: TileSelection::new(),
                },
                quads: ToolQuadLayer {
                    brush: QuadBrush::new(),
                    selection: QuadSelection::new(),
                },
                sounds: ToolSoundLayer {
                    brush: SoundBrush::new(),
                },
                active_tool: ActiveTool::Tiles(ActiveToolTiles::Brush),
            },

            editor_options: Default::default(),

            hotkeys,
            cur_hotkey_events: Default::default(),
            cached_binds_per_event: None,

            auto_mapper: TileLayerAutoMapper::new(graphics, io.clone().into(), tp.clone()),
            middle_down_pointer_pos: None,
            current_scroll_delta: Default::default(),
            current_pointer_pos: Default::default(),
            latest_pointer: Default::default(),
            latest_keys_down: Default::default(),
            latest_modifiers: Default::default(),
            latest_unused_rect: egui::Rect::from_min_size(
                egui::Pos2 { x: 0.0, y: 0.0 },
                egui::Vec2 { x: 100.0, y: 100.0 },
            ),
            latest_canvas_rect: Rect::from_min_size(
                pos2(0.0, 0.0),
                vec2(
                    graphics.canvas_handle.canvas_width() as f32,
                    graphics.canvas_handle.canvas_height() as f32,
                ),
            ),
            last_time,

            notifications: Default::default(),
            notifications_overlay: ClientNotifications::new(graphics, &sys, &ui_creator),

            graphics_mt,
            buffer_object_handle: graphics.buffer_object_handle.clone(),
            backend_handle: graphics.backend_handle.clone(),
            texture_handle: graphics.texture_handle.clone(),
            canvas_handle: graphics.canvas_handle.clone(),
            stream_handle: graphics.stream_handle.clone(),

            scene_handle: sound.scene_handle.clone(),
            container_scene: scene,
            sound_mt: sound.get_sound_mt(),

            entities_container,
            fake_texture_array,
            fake_texture,
            tile_textures: TileLayerOverlaysDdnet::new(io, tp, graphics).unwrap(),

            io: io.clone(),
            thread_pool: tp.clone(),

            save_tasks: Default::default(),

            sys,
        };
        res.load_map("map/maps/ctf1.twmap".as_ref(), Default::default());
        res
    }

    fn new_map(&mut self, name: &str, options: MapLoadOptions) {
        let res = match options {
            MapLoadOptions::WithServer(MapLoadWithServerOptions {
                cert,
                port,
                password,
                mapper_name,
                color,
                admin_password,
            }) => EditorServer::new(
                &self.sys,
                cert,
                port,
                password.unwrap_or_default(),
                admin_password,
                self.io.clone(),
            )
            .map(|server| {
                (
                    format!("127.0.0.1:{}", server.port,),
                    match &server.cert {
                        NetworkServerCertModeResult::Cert { cert } => {
                            NetworkClientCertCheckMode::CheckByCert {
                                cert: cert.to_der().unwrap().into(),
                            }
                        }
                        NetworkServerCertModeResult::PubKeyHash { hash } => {
                            NetworkClientCertCheckMode::CheckByPubKeyHash {
                                hash: Cow::Owned(*hash),
                            }
                        }
                    },
                    server.password.clone(),
                    true,
                    Some(server),
                    mapper_name,
                    color,
                )
            }),
            MapLoadOptions::WithoutServer {
                server_addr,
                cert_hash,
                password,
                mapper_name,
                color,
            } => anyhow::Ok((
                server_addr,
                NetworkClientCertCheckMode::CheckByPubKeyHash {
                    hash: Cow::Owned(cert_hash),
                },
                password,
                false,
                None,
                Some(mapper_name),
                Some(color),
            )),
        };
        let (server_addr, server_cert, password, local_client, server, mapper_name, color) =
            match res {
                Ok(res) => res,
                Err(err) => {
                    self.notifications_overlay.add_err(
                        format!("Failed to start server: {err}"),
                        Duration::from_secs(10),
                    );
                    return;
                }
            };
        let client = EditorClient::new(
            &self.sys,
            &server_addr,
            server_cert,
            self.notifications.clone(),
            password,
            local_client,
            mapper_name,
            color,
        );

        let physics_group_attr = MapGroupPhysicsAttr {
            width: NonZeroU16MinusOne::new(50).unwrap(),
            height: NonZeroU16MinusOne::new(50).unwrap(),
        };
        let game_layer = MapLayerTilePhysicsBase {
            tiles: vec![TileBase::default(); 50 * 50],
        };
        let visuals = {
            let buffer = self.thread_pool.install(|| {
                upload_physics_layer_buffer(
                    &self.graphics_mt,
                    physics_group_attr.width,
                    physics_group_attr.height,
                    MapTileLayerPhysicsTilesRef::Game(&game_layer.tiles),
                )
            });
            finish_physics_layer_buffer(&self.buffer_object_handle, &self.backend_handle, buffer)
        };

        let scene = self.scene_handle.create(Default::default());
        let global_sound_listener = scene.sound_listener_handle.create(Default::default());

        self.tabs.insert(
            name.into(),
            EditorTab {
                map: EditorMap {
                    user: EditorMapProps {
                        options: Default::default(),
                        ui_values: Default::default(),
                        sound_scene: scene,
                        global_sound_listener,
                        time: Duration::ZERO,
                        time_scale: 0,
                    },
                    resources: EditorResources {
                        images: Default::default(),
                        image_arrays: Default::default(),
                        sounds: Default::default(),
                        user: (),
                    },
                    animations: EditorAnimations {
                        pos: Default::default(),
                        color: Default::default(),
                        sound: Default::default(),
                        user: EditorAnimationsProps::default(),
                    },
                    groups: EditorGroups {
                        physics: EditorGroupPhysics {
                            attr: physics_group_attr,
                            layers: vec![EditorPhysicsLayer::Game(
                                MapLayerTilePhysicsBaseSkeleton {
                                    layer: game_layer,
                                    user: EditorPhysicsLayerProps {
                                        visuals,
                                        attr: EditorCommonGroupOrLayerAttr::default(),
                                        selected: Default::default(),
                                        number_extra: Default::default(),
                                        number_extra_text: Default::default(),
                                        enter_extra_text: Default::default(),
                                        leave_extra_text: Default::default(),
                                        context_menu_open: false,
                                        context_menu_extra_open: false,
                                        switch_delay: Default::default(),
                                        speedup_force: Default::default(),
                                        speedup_angle: Default::default(),
                                        speedup_max_speed: Default::default(),
                                    },
                                },
                            )],
                            user: EditorPhysicsGroupProps::default(),
                        },
                        background: Vec::new(),
                        foreground: Vec::new(),
                        user: EditorGroupsProps {
                            pos: Default::default(),
                            zoom: 1.0,
                            parallax_aware_zoom: false,
                        },
                    },
                    config: EditorConfig {
                        def: Config {
                            commands: Default::default(),
                            config_variables: Default::default(),
                        },
                        user: Default::default(),
                    },
                    meta: EditorMetadata {
                        def: Metadata {
                            authors: Default::default(),
                            licenses: Default::default(),
                            version: Default::default(),
                            credits: Default::default(),
                            memo: Default::default(),
                        },
                        user: (),
                    },
                },
                map_render: RenderMap::new(
                    &self.backend_handle,
                    &self.canvas_handle,
                    &self.stream_handle,
                ),
                server,
                client,
                auto_saver: AutoSaver {
                    active: false,
                    interval: Some(Duration::from_secs(60)),
                    path: None,
                    last_time: Some(self.sys.time_get()),
                },
                last_info_update: None,
                admin_panel: Default::default(),
                dbg_panel: Default::default(),
                assets_store: Default::default(),
                assets_store_open: Default::default(),
            },
        );
        self.active_tab = name.into();
    }

    fn map_to_editor_map_impl(
        graphics_mt: GraphicsMultiThreaded,
        sound_mt: SoundMultiThreaded,
        tp: &Arc<rayon::ThreadPool>,
        scene_handle: &SoundSceneHandle,
        backend_handle: &GraphicsBackendHandle,
        buffer_object_handle: &GraphicsBufferObjectHandle,
        texture_handle: &GraphicsTextureHandle,
        map: Map,
        resources: HashMap<Hash, Vec<u8>>,
    ) -> EditorMap {
        // load images into VRAM
        let (image_mems, image_array_mems, sound_mems): (Vec<_>, Vec<_>, Vec<_>) =
            tp.install(|| {
                join_all!(
                    || {
                        map.resources
                            .images
                            .into_par_iter()
                            .map(|i| {
                                let read_file = |meta: &MapResourceMetaData| {
                                    let file = resources.get(&meta.blake3_hash).unwrap();

                                    let mut mem = None;
                                    let _ = load_png_image_as_rgba(file, |width, height, _| {
                                        mem = Some(graphics_mt.mem_alloc(
                                            GraphicsMemoryAllocationType::TextureRgbaU8 {
                                                width: width.try_into().unwrap(),
                                                height: height.try_into().unwrap(),
                                                flags: TexFlags::empty(),
                                            },
                                        ));
                                        mem.as_mut().unwrap().as_mut_slice()
                                    })
                                    .unwrap();

                                    (mem.unwrap(), file.clone())
                                };
                                (read_file(&i.meta), i.hq_meta.as_ref().map(read_file), i)
                            })
                            .collect()
                    },
                    || {
                        map.resources
                            .image_arrays
                            .into_par_iter()
                            .map(|i| {
                                let read_file = |meta: &MapResourceMetaData| {
                                    let file = resources.get(&meta.blake3_hash).unwrap();

                                    let mut png = Vec::new();
                                    let img = load_png_image_as_rgba(
                                        file,
                                        |width, height, color_chanel_count| {
                                            png.resize(
                                                width * height * color_chanel_count,
                                                Default::default(),
                                            );
                                            png.as_mut_slice()
                                        },
                                    )
                                    .unwrap();

                                    let mut mem = graphics_mt.mem_alloc(
                                        GraphicsMemoryAllocationType::TextureRgbaU82dArray {
                                            width: ((img.width / 16) as usize).try_into().unwrap(),
                                            height: ((img.height / 16) as usize)
                                                .try_into()
                                                .unwrap(),
                                            depth: 256.try_into().unwrap(),
                                            flags: TexFlags::empty(),
                                        },
                                    );
                                    let mut image_3d_width = 0;
                                    let mut image_3d_height = 0;
                                    if !texture_2d_to_3d(
                                        tp,
                                        img.data,
                                        img.width as usize,
                                        img.height as usize,
                                        4,
                                        16,
                                        16,
                                        mem.as_mut_slice(),
                                        &mut image_3d_width,
                                        &mut image_3d_height,
                                    ) {
                                        panic!(
                                            "fatal error, could not convert 2d \
                                        texture to 2d array texture"
                                        );
                                    }

                                    // ALWAYS clear pixels of first tile, some mapres still have pixels in them
                                    mem.as_mut_slice()[0..image_3d_width * image_3d_height * 4]
                                        .iter_mut()
                                        .for_each(|byte| *byte = 0);

                                    (mem, file.clone())
                                };
                                (read_file(&i.meta), i.hq_meta.as_ref().map(read_file), i)
                            })
                            .collect()
                    },
                    || {
                        map.resources
                            .sounds
                            .into_par_iter()
                            .map(|i| {
                                let read_file = |meta: &MapResourceMetaData| {
                                    let file = resources.get(&meta.blake3_hash).unwrap();
                                    let mut mem = sound_mt.mem_alloc(file.len());
                                    mem.as_mut_slice().copy_from_slice(file);
                                    (mem, file.clone())
                                };
                                (read_file(&i.meta), i.hq_meta.as_ref().map(read_file), i)
                            })
                            .collect()
                    }
                )
            });

        // sound scene
        let scene = scene_handle.create(Default::default());
        let global_sound_listener = scene.sound_listener_handle.create(Default::default());

        // sound mem to sound objects
        let sound_objects: Vec<_> = sound_mems
            .into_iter()
            .map(|((mem, file), hq_mem_file, i)| {
                (
                    (scene.sound_object_handle.create(mem), file),
                    hq_mem_file.map(|(mem, file)| (scene.sound_object_handle.create(mem), file)),
                    i,
                )
            })
            .collect();

        // load layers into vram
        enum MapLayerBuffer {
            Abritrary(Vec<u8>),
            Tile {
                buffer: Box<MapBufferTileLayer>,
                layer: MapLayerTile,
            },
            Quad {
                buffer: Box<ClientMapBufferQuadLayer>,
                layer: MapLayerQuad,
            },
            Sound(MapLayerSound),
        }
        type GroupBuffers = Vec<(Vec<MapLayerBuffer>, MapGroupAttr, String)>;
        let upload_design_group_buffer = |groups: Vec<MapGroup>| -> GroupBuffers {
            groups
                .into_par_iter()
                .map(|group| {
                    (
                        group
                            .layers
                            .into_par_iter()
                            .map(|layer| match layer {
                                MapLayer::Abritrary(layer) => MapLayerBuffer::Abritrary(layer),
                                MapLayer::Tile(layer) => MapLayerBuffer::Tile {
                                    buffer: Box::new(upload_design_tile_layer_buffer(
                                        &graphics_mt,
                                        &layer.tiles,
                                        layer.attr.width,
                                        layer.attr.height,
                                        layer.attr.image_array.is_some(),
                                    )),
                                    layer,
                                },
                                MapLayer::Quad(layer) => MapLayerBuffer::Quad {
                                    buffer: Box::new(upload_design_quad_layer_buffer(
                                        &graphics_mt,
                                        &layer.attr,
                                        &layer.quads,
                                    )),
                                    layer,
                                },
                                MapLayer::Sound(layer) => MapLayerBuffer::Sound(layer),
                            })
                            .collect(),
                        group.attr,
                        group.name,
                    )
                })
                .collect()
        };
        let (physics_layers, background, foreground): (
            Vec<(MapBufferPhysicsTileLayer, MapLayerPhysics)>,
            _,
            _,
        ) = tp.install(|| {
            join_all!(
                || map
                    .groups
                    .physics
                    .layers
                    .into_par_iter()
                    .map(|layer| {
                        (
                            upload_physics_layer_buffer(
                                &graphics_mt,
                                map.groups.physics.attr.width,
                                map.groups.physics.attr.height,
                                layer.as_ref().tiles_ref(),
                            ),
                            layer,
                        )
                    })
                    .collect(),
                || upload_design_group_buffer(map.groups.background),
                || upload_design_group_buffer(map.groups.foreground)
            )
        });

        let upload_design_group = |groups: GroupBuffers| {
            groups
                .into_iter()
                .map(|(layers, attr, name)| EditorGroup {
                    layers: layers
                        .into_iter()
                        .map(|layer| match layer {
                            MapLayerBuffer::Abritrary(layer) => {
                                EditorLayer::Abritrary(EditorLayerArbitrary {
                                    buf: layer,
                                    user: EditorArbitraryLayerProps {
                                        attr: Default::default(),
                                    },
                                })
                            }
                            MapLayerBuffer::Tile { layer, buffer } => {
                                EditorLayer::Tile(EditorLayerTile {
                                    user: EditorTileLayerProps {
                                        visuals: finish_design_tile_layer_buffer(
                                            buffer_object_handle,
                                            backend_handle,
                                            *buffer,
                                        ),
                                        attr: EditorCommonGroupOrLayerAttr::default(),
                                        selected: Default::default(),
                                        auto_mapper_rule: Default::default(),
                                        auto_mapper_seed: Default::default(),
                                        live_edit: None,
                                    },
                                    layer,
                                })
                            }
                            MapLayerBuffer::Quad { layer, buffer } => {
                                EditorLayer::Quad(EditorLayerQuad {
                                    user: EditorQuadLayerProps {
                                        visuals: finish_design_quad_layer_buffer(
                                            buffer_object_handle,
                                            backend_handle,
                                            *buffer,
                                        ),
                                        attr: EditorCommonGroupOrLayerAttr::default(),
                                        selected: Default::default(),
                                    },
                                    layer,
                                })
                            }
                            MapLayerBuffer::Sound(layer) => EditorLayer::Sound(EditorLayerSound {
                                user: EditorSoundLayerProps {
                                    sounds: SoundLayerSounds::default(),
                                    attr: Default::default(),
                                    selected: Default::default(),
                                },
                                layer,
                            }),
                        })
                        .collect(),
                    attr,
                    name,
                    user: EditorGroupProps::default(),
                })
                .collect()
        };

        EditorMap {
            user: EditorMapProps {
                options: Default::default(),
                ui_values: Default::default(),
                sound_scene: scene,
                global_sound_listener,
                time: Duration::ZERO,
                time_scale: 0,
            },
            resources: EditorResources {
                images: image_mems
                    .into_iter()
                    .map(|((mem, file), hq_mem_file, i)| EditorImage {
                        user: EditorResource {
                            user: texture_handle
                                .load_texture_rgba_u8(mem, i.name.as_str())
                                .unwrap(),
                            file: file.into(),
                            hq: hq_mem_file.map(|(mem, file)| {
                                (
                                    file.into(),
                                    texture_handle
                                        .load_texture_rgba_u8(mem, i.name.as_str())
                                        .unwrap(),
                                )
                            }),
                        },
                        def: i,
                    })
                    .collect(),
                image_arrays: image_array_mems
                    .into_iter()
                    .map(|((mem, file), hq_mem_file, i)| EditorImage2dArray {
                        user: EditorResource {
                            user: texture_handle
                                .load_texture_2d_array_rgba_u8(mem, i.name.as_str())
                                .unwrap(),
                            file: file.into(),
                            hq: hq_mem_file.map(|(mem, file)| {
                                (
                                    file.into(),
                                    texture_handle
                                        .load_texture_2d_array_rgba_u8(mem, i.name.as_str())
                                        .unwrap(),
                                )
                            }),
                        },
                        def: i,
                    })
                    .collect(),
                sounds: sound_objects
                    .into_iter()
                    .map(|((s, file), hq_s_file, i)| EditorSound {
                        def: i,
                        user: EditorResource {
                            user: s,
                            file: file.into(),
                            hq: hq_s_file.map(|(s, file)| (file.into(), s)),
                        },
                    })
                    .collect(),
                user: (),
            },
            animations: EditorAnimations {
                pos: map
                    .animations
                    .pos
                    .into_iter()
                    .map(|pos| EditorPosAnimation {
                        def: pos,
                        user: Default::default(),
                    })
                    .collect(),
                color: map
                    .animations
                    .color
                    .into_iter()
                    .map(|color| EditorColorAnimation {
                        def: color,
                        user: Default::default(),
                    })
                    .collect(),
                sound: map
                    .animations
                    .sound
                    .into_iter()
                    .map(|sound| EditorSoundAnimation {
                        def: sound,
                        user: Default::default(),
                    })
                    .collect(),
                user: EditorAnimationsProps::default(),
            },
            groups: EditorGroups {
                physics: EditorGroupPhysics {
                    layers: physics_layers
                        .into_iter()
                        .map(|(buffer, layer)| {
                            let user = EditorPhysicsLayerProps {
                                visuals: finish_physics_layer_buffer(
                                    buffer_object_handle,
                                    backend_handle,
                                    buffer,
                                ),
                                attr: EditorCommonGroupOrLayerAttr::default(),
                                selected: Default::default(),
                                number_extra: Default::default(),
                                number_extra_text: Default::default(),
                                enter_extra_text: Default::default(),
                                leave_extra_text: Default::default(),
                                context_menu_open: false,
                                context_menu_extra_open: false,
                                switch_delay: Default::default(),
                                speedup_force: Default::default(),
                                speedup_angle: Default::default(),
                                speedup_max_speed: Default::default(),
                            };
                            match layer {
                                MapLayerPhysics::Arbitrary(layer) => EditorPhysicsLayer::Arbitrary(
                                    MapLayerArbitraryPhysicsSkeleton { buf: layer, user },
                                ),
                                MapLayerPhysics::Game(layer) => {
                                    EditorPhysicsLayer::Game(MapLayerTilePhysicsBaseSkeleton {
                                        layer,
                                        user,
                                    })
                                }
                                MapLayerPhysics::Front(layer) => {
                                    EditorPhysicsLayer::Front(MapLayerTilePhysicsBaseSkeleton {
                                        layer,
                                        user,
                                    })
                                }
                                MapLayerPhysics::Tele(layer) => {
                                    EditorPhysicsLayer::Tele(MapLayerTelePhysicsSkeleton {
                                        layer,
                                        user,
                                    })
                                }
                                MapLayerPhysics::Speedup(layer) => {
                                    EditorPhysicsLayer::Speedup(MapLayerTilePhysicsBaseSkeleton {
                                        layer,
                                        user,
                                    })
                                }
                                MapLayerPhysics::Switch(layer) => {
                                    EditorPhysicsLayer::Switch(MapLayerSwitchPhysicsSkeleton {
                                        layer,
                                        user,
                                    })
                                }
                                MapLayerPhysics::Tune(layer) => {
                                    EditorPhysicsLayer::Tune(MapLayerTunePhysicsSkeleton {
                                        layer,
                                        user,
                                    })
                                }
                            }
                        })
                        .collect(),
                    attr: map.groups.physics.attr,
                    user: EditorPhysicsGroupProps::default(),
                },
                background: upload_design_group(background),
                foreground: upload_design_group(foreground),
                user: EditorGroupsProps {
                    pos: Default::default(),
                    zoom: 1.0,
                    parallax_aware_zoom: false,
                },
            },
            config: EditorConfig {
                def: map.config,
                user: Default::default(),
            },
            meta: EditorMetadata {
                def: map.meta,
                user: (),
            },
        }
    }

    fn map_to_editor_map(&self, map: Map, resources: HashMap<Hash, Vec<u8>>) -> EditorMap {
        Self::map_to_editor_map_impl(
            self.graphics_mt.clone(),
            self.sound_mt.clone(),
            &self.thread_pool,
            &self.scene_handle,
            &self.backend_handle,
            &self.buffer_object_handle,
            &self.texture_handle,
            map,
            resources,
        )
    }

    fn path_to_tab_name(path: &Path) -> anyhow::Result<String> {
        Ok(path
            .file_stem()
            .ok_or_else(|| anyhow!("{path:?} is not a valid file"))?
            .to_string_lossy()
            .to_string())
    }

    #[cfg(feature = "legacy")]
    fn load_legacy_map(
        &mut self,
        path: &Path,
        options: MapLoadWithServerOptions,
    ) -> anyhow::Result<()> {
        let name = Self::path_to_tab_name(path)?;

        let tp = self.thread_pool.clone();
        let fs = self.io.fs.clone();
        let path_buf = path.to_path_buf();
        let map_file = self
            .io
            .rt
            .spawn(async move { read_file_editor(&fs, &path_buf).await })
            .get_storage()?;
        let map = map_convert_lib::legacy_to_new::legacy_to_new_from_buf(
            map_file,
            path.file_stem()
                .ok_or(anyhow::anyhow!("wrong file name"))?
                .to_str()
                .ok_or(anyhow::anyhow!("file name not utf8"))?,
            &self.io.clone().into(),
            &tp,
            true,
        )
        .map_err(|err| anyhow::anyhow!("Loading legacy map loading failed: {err}"))?;

        let resources: HashMap<_, _> = map
            .resources
            .images
            .into_iter()
            .map(|(hash, res)| (hash, res.buf))
            .chain(
                map.resources
                    .sounds
                    .into_iter()
                    .map(|(hash, res)| (hash, res.buf)),
            )
            .collect();
        let map = self.map_to_editor_map(map.map, resources);

        let server = EditorServer::new(
            &self.sys,
            options.cert,
            options.port,
            options.password.clone().unwrap_or_default(),
            options.admin_password,
            self.io.clone(),
        )?;
        let client = EditorClient::new(
            &self.sys,
            &format!("127.0.0.1:{}", server.port),
            match &server.cert {
                NetworkServerCertModeResult::Cert { cert } => {
                    NetworkClientCertCheckMode::CheckByCert {
                        cert: cert.to_der()?.into(),
                    }
                }
                NetworkServerCertModeResult::PubKeyHash { hash } => {
                    NetworkClientCertCheckMode::CheckByPubKeyHash {
                        hash: Cow::Borrowed(hash),
                    }
                }
            },
            self.notifications.clone(),
            options.password.unwrap_or_default(),
            true,
            options.mapper_name,
            options.color,
        );

        self.tabs.insert(
            name.clone(),
            EditorTab {
                map,
                map_render: RenderMap::new(
                    &self.backend_handle,
                    &self.canvas_handle,
                    &self.stream_handle,
                ),
                server: Some(server),
                client,
                auto_saver: AutoSaver {
                    active: false,
                    interval: Some(Duration::from_secs(60)),
                    path: Some(path.into()),
                    last_time: Some(self.sys.time_get()),
                },
                last_info_update: None,
                admin_panel: Default::default(),
                dbg_panel: Default::default(),
                assets_store: Default::default(),
                assets_store_open: Default::default(),
            },
        );
        self.active_tab = name;

        Ok(())
    }

    #[cfg(not(feature = "legacy"))]
    fn load_legacy_map(
        &mut self,
        _path: &Path,
        _options: MapLoadWithServerOptions,
    ) -> anyhow::Result<()> {
        Err(anyhow!("loading legacy maps is not supported"))
    }

    fn map_resource_path(ty: ReadFileTy, name: &str, meta: &MapResourceMetaData) -> String {
        format!(
            "map/resources/{}/{}_{}.{}",
            if ty == ReadFileTy::Image {
                "images"
            } else {
                "sounds"
            },
            name,
            fmt_hash(&meta.blake3_hash),
            meta.ty.as_str()
        )
    }

    fn load_map_impl(
        &mut self,
        path: &Path,
        options: MapLoadWithServerOptions,
    ) -> anyhow::Result<()> {
        let name = Self::path_to_tab_name(path)?;

        let fs = self.io.fs.clone();
        let tp = self.thread_pool.clone();
        let load_path = path.to_path_buf();
        let path = path.to_path_buf();
        let (map, resources) = self
            .io
            .rt
            .spawn(async move {
                let file = read_file_editor(&fs, &path).await?;
                let map = Map::read(&file, &tp)?;
                let mut resource_files: HashMap<Hash, Vec<u8>> = Default::default();
                for (ty, i) in map
                    .resources
                    .images
                    .iter()
                    .map(|i| (ReadFileTy::Image, i))
                    .chain(
                        map.resources
                            .image_arrays
                            .iter()
                            .map(|i| (ReadFileTy::Image, i)),
                    )
                    .chain(map.resources.sounds.iter().map(|i| (ReadFileTy::Sound, i)))
                {
                    async fn read_file(
                        fs: &Arc<dyn FileSystemInterface>,
                        ty: ReadFileTy,
                        resource_files: &mut HashMap<Hash, Vec<u8>>,
                        name: &str,
                        meta: &MapResourceMetaData,
                    ) -> anyhow::Result<()> {
                        if let std::collections::hash_map::Entry::Vacant(e) =
                            resource_files.entry(meta.blake3_hash)
                        {
                            let file_path = Editor::map_resource_path(ty, name, meta);
                            let file = read_file_editor(fs, file_path.as_ref()).await;

                            let file = match file {
                                Ok(file) => file,
                                Err(err) => {
                                    // also try to download from downloaded folder
                                    let downloaded: &Path = "downloaded".as_ref();
                                    match read_file_editor(fs, &downloaded.join(file_path)).await {
                                        Ok(file) => file,
                                        Err(_) => anyhow::bail!(err),
                                    }
                                }
                            };

                            e.insert(file);
                        }
                        anyhow::Ok(())
                    }
                    read_file(&fs, ty, &mut resource_files, i.name.as_str(), &i.meta).await?;
                    if let Some(hq_meta) = &i.hq_meta {
                        read_file(&fs, ty, &mut resource_files, i.name.as_str(), hq_meta).await?;
                    }
                }

                Ok((map, resource_files))
            })
            .get_storage()?;

        let map = self.map_to_editor_map(map, resources);

        let server = EditorServer::new(
            &self.sys,
            options.cert,
            options.port,
            options.password.clone().unwrap_or_default(),
            options.admin_password,
            self.io.clone(),
        )?;
        let client = EditorClient::new(
            &self.sys,
            &format!("127.0.0.1:{}", server.port),
            match &server.cert {
                NetworkServerCertModeResult::Cert { cert } => {
                    NetworkClientCertCheckMode::CheckByCert {
                        cert: cert.to_der()?.into(),
                    }
                }
                NetworkServerCertModeResult::PubKeyHash { hash } => {
                    NetworkClientCertCheckMode::CheckByPubKeyHash {
                        hash: Cow::Borrowed(hash),
                    }
                }
            },
            self.notifications.clone(),
            options.password.unwrap_or_default(),
            true,
            options.mapper_name,
            options.color,
        );

        self.tabs.insert(
            name.clone(),
            EditorTab {
                map,
                map_render: RenderMap::new(
                    &self.backend_handle,
                    &self.canvas_handle,
                    &self.stream_handle,
                ),
                server: Some(server),
                client,
                auto_saver: AutoSaver {
                    active: false,
                    interval: Some(Duration::from_secs(60)),
                    path: Some(load_path),
                    last_time: Some(self.sys.time_get()),
                },
                last_info_update: None,
                admin_panel: Default::default(),
                dbg_panel: Default::default(),
                assets_store: Default::default(),
                assets_store_open: Default::default(),
            },
        );
        self.active_tab = name;

        Ok(())
    }

    /// Loads either a legacy or new map based on the file extension.
    fn load_map(&mut self, path: &Path, options: MapLoadWithServerOptions) {
        let res = if path.extension().is_some_and(|ext| ext == "map") {
            self.load_legacy_map(path, options)
        } else {
            self.load_map_impl(path, options)
        };
        if let Err(err) = res {
            log::error!("{err}");
            self.notifications_overlay
                .add_err(err.to_string(), Duration::from_secs(10));
        }
    }

    #[cfg(feature = "legacy")]
    pub fn save_map_legacy(
        tab: &mut EditorTab,
        io: &Io,
        tp: &Arc<rayon::ThreadPool>,
        path: &Path,
    ) -> anyhow::Result<IoRuntimeTask<()>> {
        use map::map::resources::MapResourceRef;

        tab.auto_saver.path = Some(path.to_path_buf());
        let (map, resources, path) = Self::save_map_tab_impl(tab, path);

        let tp = tp.clone();
        let fs = io.fs.clone();
        Ok(io.rt.spawn(async move {
            let mut file: Vec<u8> = Default::default();
            map.write(&mut file, &tp)?;
            let map_legacy = map_convert_lib::new_to_legacy::new_to_legacy_from_buf_async(
                &file,
                |map| {
                    let map_resources = map.resources.clone();
                    Box::pin(async move {
                        let collect_resources = |res: &[MapResourceRef], ty: ReadFileTy| {
                            res.iter()
                                .map(|r| {
                                    resources
                                        .get(&Self::map_resource_path(ty, r.name.as_str(), &r.meta))
                                        .cloned()
                                        .unwrap()
                                })
                                .collect()
                        };
                        Ok((
                            collect_resources(&map_resources.images, ReadFileTy::Image),
                            collect_resources(&map_resources.image_arrays, ReadFileTy::Image),
                            collect_resources(&map_resources.sounds, ReadFileTy::Sound),
                        ))
                    })
                },
                &tp,
            )
            .await?;

            fs.write_file(&path, map_legacy.map).await?;
            Ok(())
        }))
    }

    #[cfg(not(feature = "legacy"))]
    pub fn save_map_legacy(
        tab: &mut EditorTab,
        io: &Io,
        tp: &Arc<rayon::ThreadPool>,
        path: &Path,
    ) -> anyhow::Result<IoRuntimeTask<()>> {
        Err(anyhow!("saving as legacy map is not supported"))
    }

    fn save_map_tab_impl(
        tab: &mut EditorTab,
        path: &Path,
    ) -> (Map, HashMap<String, Vec<u8>>, PathBuf) {
        tab.auto_saver.path = Some(path.to_path_buf());
        let map: Map = tab.map.clone().into();
        let resources = tab
            .map
            .resources
            .images
            .iter()
            .flat_map(|r| {
                [(
                    format!(
                        "map/resources/images/{}_{}.{}",
                        r.def.name.as_str(),
                        fmt_hash(&r.def.meta.blake3_hash),
                        r.def.meta.ty.as_str()
                    ),
                    r.user.file.as_ref().clone(),
                )]
                .into_iter()
                .chain(
                    r.user
                        .hq
                        .as_ref()
                        .zip(r.def.hq_meta.as_ref())
                        .map(|((f, _), meta)| {
                            (
                                format!(
                                    "map/resources/images/{}_{}.{}",
                                    r.def.name.as_str(),
                                    fmt_hash(&meta.blake3_hash),
                                    meta.ty.as_str()
                                ),
                                f.as_ref().clone(),
                            )
                        }),
                )
                .collect::<Vec<_>>()
            })
            .chain(tab.map.resources.image_arrays.iter().flat_map(|r| {
                [(
                    format!(
                        "map/resources/images/{}_{}.{}",
                        r.def.name.as_str(),
                        fmt_hash(&r.def.meta.blake3_hash),
                        r.def.meta.ty.as_str()
                    ),
                    r.user.file.as_ref().clone(),
                )]
                .into_iter()
                .chain(
                    r.user
                        .hq
                        .as_ref()
                        .zip(r.def.hq_meta.as_ref())
                        .map(|((f, _), meta)| {
                            (
                                format!(
                                    "map/resources/images/{}_{}.{}",
                                    r.def.name.as_str(),
                                    fmt_hash(&meta.blake3_hash),
                                    meta.ty.as_str()
                                ),
                                f.as_ref().clone(),
                            )
                        }),
                )
                .collect::<Vec<_>>()
            }))
            .chain(tab.map.resources.sounds.iter().flat_map(|r| {
                [(
                    format!(
                        "map/resources/sounds/{}_{}.{}",
                        r.def.name.as_str(),
                        fmt_hash(&r.def.meta.blake3_hash),
                        r.def.meta.ty.as_str()
                    ),
                    r.user.file.as_ref().clone(),
                )]
                .into_iter()
                .chain(
                    r.user
                        .hq
                        .as_ref()
                        .zip(r.def.hq_meta.as_ref())
                        .map(|((f, _), meta)| {
                            (
                                format!(
                                    "map/resources/sounds/{}_{}.{}",
                                    r.def.name.as_str(),
                                    fmt_hash(&meta.blake3_hash),
                                    meta.ty.as_str()
                                ),
                                f.as_ref().clone(),
                            )
                        }),
                )
                .collect::<Vec<_>>()
            }))
            .collect::<HashMap<_, _>>();
        (map, resources, path.to_path_buf())
    }

    pub fn save_map_tab(
        tab: &mut EditorTab,
        io: &Io,
        tp: &Arc<rayon::ThreadPool>,

        save_tasks: &mut Vec<IoRuntimeTask<()>>,
        notifications_overlay: &mut ClientNotifications,
        path: &Path,
    ) {
        tab.client.should_save = false;
        if path.extension().is_some_and(|ext| ext == "map") {
            match Self::save_map_legacy(tab, io, tp, path) {
                Ok(task) => {
                    save_tasks.push(task);
                }
                Err(err) => {
                    log::error!("{err}");
                    notifications_overlay.add_err(err.to_string(), Duration::from_secs(10));
                }
            }
        } else {
            let (map, resources, path) = Self::save_map_tab_impl(tab, path);
            let tp = tp.clone();
            let fs = io.fs.clone();

            save_tasks.push(io.rt.spawn(async move {
                fs.create_dir("map/maps".as_ref()).await?;
                fs.create_dir("map/resources/images".as_ref()).await?;
                fs.create_dir("map/resources/sounds".as_ref()).await?;

                let mut file: Vec<u8> = Default::default();
                map.write(&mut file, &tp)?;
                fs.write_file(path.as_ref(), file).await?;

                // now write all resources
                for (path, resource) in resources {
                    fs.write_file(path.as_ref(), resource).await?;
                }
                Ok(())
            }));
        }
    }

    pub fn save_map(&mut self, path: &Path) {
        if let Some(tab) = self.tabs.get_mut(&self.active_tab) {
            Self::save_map_tab(
                tab,
                &self.io,
                &self.thread_pool,
                &mut self.save_tasks,
                &mut self.notifications_overlay,
                path,
            );
        } else {
            let msg = "No map was loaded to be saved.";
            log::info!("{msg}");
            self.notifications_overlay
                .add_err(msg, Duration::from_secs(10));
        }
    }

    fn save_tab(&mut self, tab: &str) -> bool {
        if let Some((path, tab)) = self
            .tabs
            .get_mut(tab)
            .and_then(|tab| tab.auto_saver.path.clone().map(|path| (path.clone(), tab)))
        {
            Self::save_map_tab(
                tab,
                &self.io,
                &self.thread_pool,
                &mut self.save_tasks,
                &mut self.notifications_overlay,
                &path,
            );
            true
        } else {
            let msg = "The current map has never been saved.\n\
            It has to be saved using the GUI at least once.";
            log::info!("{msg}");
            self.notifications_overlay
                .add_err(msg, Duration::from_secs(10));

            self.ui.menu_dialog_mode = EditorMenuDialogMode::save(&self.io);
            false
        }
    }

    fn save_all_tabs(&mut self) -> bool {
        let mut all_saved = true;
        for (path, tab) in self
            .tabs
            .values_mut()
            .map(|tab| (tab.auto_saver.path.clone(), tab))
        {
            if let Some(path) = path {
                Self::save_map_tab(
                    tab,
                    &self.io,
                    &self.thread_pool,
                    &mut self.save_tasks,
                    &mut self.notifications_overlay,
                    &path,
                );
            } else {
                let msg = "Some maps have never been saved.\n\
                    It has to be saved using the GUI at least once.";
                log::info!("{msg}");
                self.notifications_overlay
                    .add_err(msg, Duration::from_secs(10));
                all_saved = false;
            }
        }

        all_saved
    }

    fn update(&mut self) {
        let time_now = self.sys.time_get();
        let time_diff = time_now - self.last_time;
        self.last_time = time_now;
        let mut removed_tabs: Vec<String> = Default::default();
        for (tab_name, tab) in &mut self.tabs {
            tab.map.user.time += time_diff * tab.map.user.time_scale;

            let update_res = tab.client.update(
                &self.thread_pool,
                &self.sound_mt,
                &self.graphics_mt,
                &self.buffer_object_handle,
                &self.backend_handle,
                &self.texture_handle,
                &mut tab.map,
                &mut tab.admin_panel,
                &self.auto_mapper,
            );
            if update_res.is_err() {
                removed_tabs.push(tab_name.clone());
            }
            if let Ok(Some(EditorEventOverwriteMap {
                map,
                resources,
                live_edited_layers,
            })) = update_res
            {
                let map = Map::read(&map, &self.thread_pool).unwrap();
                tab.map = Self::map_to_editor_map_impl(
                    self.graphics_mt.clone(),
                    self.sound_mt.clone(),
                    &self.thread_pool,
                    &self.scene_handle,
                    &self.backend_handle,
                    &self.buffer_object_handle,
                    &self.texture_handle,
                    map,
                    resources,
                );
                for layer_index in live_edited_layers {
                    tab.client
                        .set_live_edit_layer(&mut tab.map, layer_index, true);
                }
            }
            if let Some(server) = &mut tab.server {
                server.update(
                    &self.thread_pool,
                    &self.sound_mt,
                    &self.graphics_mt,
                    &self.buffer_object_handle,
                    &self.backend_handle,
                    &self.texture_handle,
                    &mut tab.map,
                    &mut tab.auto_saver,
                    &mut self.notifications_overlay,
                    &mut tab.client.should_save,
                );
            }

            if tab.auto_saver.active {
                let last_time = tab
                    .auto_saver
                    .last_time
                    .get_or_insert_with(|| self.sys.time_get());

                if let (Some(interval), Some(path)) =
                    (tab.auto_saver.interval, tab.auto_saver.path.as_ref())
                {
                    let cur_time = self.sys.time_get();
                    if cur_time.saturating_sub(*last_time) > interval {
                        *last_time = cur_time;
                        let path = path.clone();
                        Self::save_map_tab(
                            tab,
                            &self.io,
                            &self.thread_pool,
                            &mut self.save_tasks,
                            &mut self.notifications_overlay,
                            &path,
                        );
                    }
                }
            }
        }
        for tab in removed_tabs {
            self.tabs.remove(&tab);
        }
    }

    fn render_tile_layer_rect(
        &self,
        map: &EditorMap,
        parallax: fvec2,
        offset: fvec2,
        layer_width: NonZeroU16MinusOne,
        layer_height: NonZeroU16MinusOne,
    ) {
        let x = 0.0_f32;
        let y = 0.0_f32;
        let w = layer_width.get() as f32;
        let h = layer_height.get() as f32;
        let rect = Rect {
            min: Pos2::new(x.min(x + w), y.min(y + h)),
            max: Pos2::new(x.max(x + w), y.max(y + h)),
        };
        let color = ubvec4::new(255, 255, 255, 255);
        let state = render_rect_state(
            &self.canvas_handle,
            map,
            &vec2::new(parallax.x.to_num(), parallax.y.to_num()),
            &vec2::new(offset.x.to_num(), offset.y.to_num()),
        );
        render_rect_from_state(&self.stream_handle, state, rect, color);
    }

    fn render_design_layer<AS: HiarcTrait, A: HiarcTrait>(
        &self,
        map_render: &RenderMap,
        map: &EditorMap,
        animations: &AnimationsSkeleton<AS, A>,
        group: &EditorGroup,
        layer: &EditorLayer,
        as_tile_index: Option<&TextureContainer2dArray>,
        as_tile_flag: Option<&TextureContainer2dArray>,
        layer_rect: &mut Vec<LayerRect>,
    ) {
        let time = map.user.render_time();

        map_render.render_layer(
            animations,
            &map.resources,
            &ConfigMap::default(),
            &map.game_camera(),
            &RenderMap::calc_anim_time(
                map.game_time_info().ticks_per_second,
                map.animation_tick(),
                &map.game_time_info().intra_tick_time,
            ),
            &time,
            &group.attr,
            layer,
            match layer {
                MapLayerSkeleton::Abritrary(_) | MapLayerSkeleton::Sound(_) => None,
                MapLayerSkeleton::Tile(layer) => {
                    if let Some(tex) = as_tile_index {
                        Some(ForcedTexture::TileLayerTileIndex(tex))
                    } else if let Some(tex) = as_tile_flag {
                        Some(ForcedTexture::TileLayerTileFlag(tex))
                    } else if let Some(EditorTileLayerPropsSelection {
                        image_2d_array_selection_open:
                            Some(ResourceSelection {
                                hovered_resource: Some(index),
                            }),
                        ..
                    }) = layer.user.selected
                    {
                        index
                            .map(|index| {
                                map.resources
                                    .image_arrays
                                    .get(index)
                                    .map(|res| ForcedTexture::TileLayer(&res.user.user))
                            })
                            .unwrap_or_else(|| {
                                Some(ForcedTexture::TileLayer(&self.fake_texture_array))
                            })
                    } else if layer.layer.attr.image_array.is_none() {
                        Some(ForcedTexture::TileLayer(&self.fake_texture_array))
                    } else {
                        None
                    }
                }
                MapLayerSkeleton::Quad(layer) => {
                    if let Some(EditorQuadLayerPropsPropsSelection {
                        image_selection_open:
                            Some(ResourceSelection {
                                hovered_resource: Some(index),
                            }),
                        ..
                    }) = layer.user.selected
                    {
                        index
                            .map(|index| {
                                map.resources
                                    .images
                                    .get(index)
                                    .map(|res| ForcedTexture::QuadLayer(&res.user.user))
                            })
                            .unwrap_or_else(|| Some(ForcedTexture::QuadLayer(&self.fake_texture)))
                    } else if layer.layer.attr.image.is_none() {
                        Some(ForcedTexture::QuadLayer(&self.fake_texture))
                    } else {
                        None
                    }
                }
            },
        );

        if let Some(MapLayerSkeleton::Tile(layer)) = layer.editor_attr().active.then_some(layer) {
            layer_rect.push(LayerRect {
                parallax: group.attr.parallax,
                offset: group.attr.offset,
                width: layer.layer.attr.width,
                height: layer.layer.attr.height,
            })
        }
    }

    fn render_group_clip(&self, map: &EditorMap, attr: MapGroupAttr) {
        let MapGroupAttr {
            clipping: Some(clip),
            ..
        } = attr
        else {
            return;
        };

        let x = clip.pos.x.to_num::<f32>();
        let y = clip.pos.y.to_num::<f32>();
        let w = clip.size.x.to_num::<f32>();
        let h = clip.size.y.to_num::<f32>();
        let rect = Rect {
            min: Pos2::new(x.min(x + w), y.min(y + h)),
            max: Pos2::new(x.max(x + w), y.max(y + h)),
        };
        let color = ubvec4::new(255, 0, 0, 255);
        render_rect(
            &self.canvas_handle,
            &self.stream_handle,
            map,
            rect,
            color,
            &vec2::new(100.0, 100.0),
            &vec2::new(0.0, 0.0),
        );
    }

    fn render_design_groups(
        &self,
        map_render: &RenderMap,
        map: &EditorMap,
        groups: &[EditorGroup],
        tile_index_texture: TextureContainer2dArray,
        tile_flag_texture: TextureContainer2dArray,
        group_clips: &mut Vec<MapGroupAttr>,
        layer_rect: &mut Vec<LayerRect>,
    ) {
        for group in groups.iter().filter(|group| !group.editor_attr().hidden) {
            for layer in group.layers.iter() {
                if !layer.editor_attr().hidden {
                    if map.user.ui_values.animations_panel_open {
                        self.render_design_layer(
                            map_render,
                            map,
                            &map.animations.user.animations,
                            group,
                            layer,
                            None,
                            None,
                            layer_rect,
                        );
                    } else {
                        self.render_design_layer(
                            map_render,
                            map,
                            &map.animations,
                            group,
                            layer,
                            None,
                            None,
                            layer_rect,
                        );
                    }
                    if layer.editor_attr().active && map.user.options.show_tile_numbers {
                        self.render_design_layer(
                            map_render,
                            map,
                            &map.animations,
                            group,
                            layer,
                            Some(&tile_index_texture),
                            None,
                            layer_rect,
                        );
                        self.render_design_layer(
                            map_render,
                            map,
                            &map.animations,
                            group,
                            layer,
                            None,
                            Some(&tile_flag_texture),
                            layer_rect,
                        );
                    }
                    if let MapLayerSkeleton::Sound(layer) = layer {
                        let time = map.user.render_time();
                        map_render.sound.handle_sound_layer(
                            &map.animations,
                            &time,
                            &RenderMap::calc_anim_time(
                                50.try_into().unwrap(),
                                (time.as_millis() / (1000 / 50)).max(1) as GameTickType,
                                &time,
                            ),
                            &map.resources.sounds,
                            &group.attr,
                            layer,
                            &Camera {
                                pos: map.groups.user.pos,
                                zoom: map.groups.user.zoom,
                                parallax_aware_zoom: map.groups.user.parallax_aware_zoom,
                                forced_aspect_ratio: None,
                            },
                            0.3,
                        );
                    }
                } else if let MapLayerSkeleton::Sound(layer) = layer {
                    layer.user.sounds.stop_all();
                }
            }
            if group.attr.clipping.is_some()
                && (group.editor_attr().active
                    || group.layers.iter().any(|layer| layer.editor_attr().active))
            {
                group_clips.push(group.attr);
            }
        }
    }

    fn render_physics_layer(
        entities_container: &mut EntitiesContainer,
        map_render: &RenderMap,
        map: &EditorMap,
        layer: &EditorPhysicsLayer,
        as_tile_index: Option<&TextureContainer2dArray>,
        as_tile_flag: Option<&TextureContainer2dArray>,
    ) {
        let time = map.user.render_time();
        map_render.render_physics_layer(
            &map.animations,
            entities_container,
            None,
            // TODO:
            "ddnet",
            layer,
            &Camera {
                pos: map.groups.user.pos,
                zoom: map.groups.user.zoom,
                parallax_aware_zoom: map.groups.user.parallax_aware_zoom,
                forced_aspect_ratio: None,
            },
            &time,
            &RenderMap::calc_anim_time(
                map.game_time_info().ticks_per_second,
                (time.as_millis() / (1000 / 50)).max(1) as GameTickType,
                &map.game_time_info().intra_tick_time,
            ),
            100,
            as_tile_index
                .map(ForcedTexture::TileLayerTileIndex)
                .or(as_tile_flag.map(ForcedTexture::TileLayerTileFlag)),
        );
    }

    fn render_physics_group(
        entities_container: &mut EntitiesContainer,
        map_render: &RenderMap,
        map: &EditorMap,
        group: &EditorGroupPhysics,
        tile_index_texture: TextureContainer2dArray,
        tile_flag_texture: TextureContainer2dArray,
        layer_rect: &mut Vec<LayerRect>,
    ) {
        if group.editor_attr().hidden {
            return;
        }
        for layer in group
            .layers
            .iter()
            .filter(|&layer| !layer.user().attr.hidden)
        {
            Self::render_physics_layer(entities_container, map_render, map, layer, None, None);

            if layer.editor_attr().active && map.user.options.show_tile_numbers {
                Self::render_physics_layer(
                    entities_container,
                    map_render,
                    map,
                    layer,
                    Some(&tile_index_texture),
                    None,
                );
                Self::render_physics_layer(
                    entities_container,
                    map_render,
                    map,
                    layer,
                    None,
                    Some(&tile_flag_texture),
                );
            }

            if layer.editor_attr().active {
                layer_rect.push(LayerRect {
                    parallax: fvec2::new(ffixed::from_num(100.0), ffixed::from_num(100.0)),
                    offset: fvec2::default(),
                    width: group.attr.width,
                    height: group.attr.height,
                });
            }
        }
    }

    /// brushes, moving camera etc.
    fn handle_world(&mut self, ui_canvas: &UiCanvasSize, unused_rect: egui::Rect) {
        // handle middle mouse click
        if self.latest_pointer.middle_down() {
            let active_tab = self.tabs.get_mut(&self.active_tab);
            if let Some(tab) = active_tab {
                if let Some(middle_down_pointer) = &self.middle_down_pointer_pos {
                    let pos = self.current_pointer_pos;
                    let old_pos = middle_down_pointer;

                    let zoom = tab.map.groups.user.zoom;
                    let pos = ui_pos_to_world_pos(
                        &self.canvas_handle,
                        ui_canvas,
                        zoom,
                        vec2::new(pos.x, pos.y),
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        100.0,
                        100.0,
                        false,
                    );
                    let old_pos = ui_pos_to_world_pos(
                        &self.canvas_handle,
                        ui_canvas,
                        zoom,
                        vec2::new(old_pos.x, old_pos.y),
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        100.0,
                        100.0,
                        false,
                    );

                    tab.map.groups.user.pos.x -= pos.x - old_pos.x;
                    tab.map.groups.user.pos.y -= pos.y - old_pos.y;
                }
                self.middle_down_pointer_pos = Some(self.current_pointer_pos);
            }
        } else {
            self.middle_down_pointer_pos = None;
        }

        let active_tab = self.tabs.get_mut(&self.active_tab);
        if let Some(tab) = active_tab {
            // handle zoom
            if self.current_scroll_delta.y.abs() > 0.01 {
                let zoom_ranges = [
                    (0.0..0.6, 0.1),
                    (0.6..1.0, 0.2),
                    (1.0..5.0, 0.5),
                    (5.0..10.0, 1.0),
                    (10.0..f32::MAX, 10.0),
                ];
                // zoom in => non-inclusive range, zoom out => inclusive range
                let (_, step) = zoom_ranges
                    .iter()
                    .find(|&(zoom_range, _)| {
                        if self.current_scroll_delta.y.is_sign_negative() {
                            (zoom_range.start..zoom_range.end)
                                .contains(&tab.map.groups.user.zoom.abs())
                        } else {
                            (zoom_range.start..=zoom_range.end)
                                .contains(&tab.map.groups.user.zoom.abs())
                        }
                    })
                    .unwrap();
                tab.map.groups.user.zoom = (tab.map.groups.user.zoom
                    + step * -self.current_scroll_delta.y.signum())
                .clamp(0.2, 200.0);
            }

            // change active tool set
            match tab.map.active_layer() {
                Some(layer) => match layer {
                    EditorLayerUnionRef::Physics { .. } => {
                        if !matches!(self.tools.active_tool, ActiveTool::Tiles(_)) {
                            self.tools
                                .set_tool(ActiveTool::Tiles(ActiveToolTiles::Brush));
                        }
                    }
                    EditorLayerUnionRef::Design { layer, .. } => match layer {
                        MapLayerSkeleton::Abritrary(_) => {}
                        MapLayerSkeleton::Tile(_) => {
                            if !matches!(self.tools.active_tool, ActiveTool::Tiles(_)) {
                                self.tools
                                    .set_tool(ActiveTool::Tiles(ActiveToolTiles::Brush));
                            }
                        }
                        MapLayerSkeleton::Quad(_) => {
                            if !matches!(self.tools.active_tool, ActiveTool::Quads(_)) {
                                self.tools
                                    .set_tool(ActiveTool::Quads(ActiveToolQuads::Brush));
                            }
                        }
                        MapLayerSkeleton::Sound(_) => {
                            if !matches!(self.tools.active_tool, ActiveTool::Sounds(_)) {
                                self.tools
                                    .set_tool(ActiveTool::Sounds(ActiveToolSounds::Brush));
                            }
                        }
                    },
                },
                None => { // simply do nothing
                }
            }

            match &self.tools.active_tool {
                ActiveTool::Tiles(tool) => self.tools.tiles.update(
                    ui_canvas,
                    tool,
                    &self.thread_pool,
                    &self.graphics_mt,
                    &self.buffer_object_handle,
                    &self.backend_handle,
                    &self.canvas_handle,
                    &mut self.entities_container,
                    &self.fake_texture_array,
                    &tab.map,
                    &self.latest_pointer,
                    &self.latest_keys_down,
                    &self.latest_modifiers,
                    &self.current_pointer_pos,
                    &unused_rect,
                    &mut tab.client,
                ),
                ActiveTool::Quads(tool) => self.tools.quads.update(
                    ui_canvas,
                    tool,
                    &self.graphics_mt,
                    &self.buffer_object_handle,
                    &self.backend_handle,
                    &self.canvas_handle,
                    &mut tab.map,
                    &self.fake_texture,
                    &self.latest_pointer,
                    &self.current_pointer_pos,
                    &self.latest_modifiers,
                    &mut tab.client,
                ),
                ActiveTool::Sounds(tool) => self.tools.sounds.update(
                    ui_canvas,
                    tool,
                    &self.canvas_handle,
                    &mut tab.map,
                    &self.latest_pointer,
                    &self.current_pointer_pos,
                    &mut tab.client,
                ),
            }
        }
    }

    fn render_tools(&mut self, ui_canvas: &UiCanvasSize) {
        let active_tab = self.tabs.get_mut(&self.active_tab);
        if let Some(tab) = active_tab {
            // change active tool set
            match tab.map.active_layer() {
                Some(layer) => match layer {
                    EditorLayerUnionRef::Physics { .. } => {
                        if !matches!(self.tools.active_tool, ActiveTool::Tiles(_)) {
                            self.tools
                                .set_tool(ActiveTool::Tiles(ActiveToolTiles::Brush));
                        }
                    }
                    EditorLayerUnionRef::Design { layer, .. } => match layer {
                        MapLayerSkeleton::Abritrary(_) => {}
                        MapLayerSkeleton::Tile(_) => {
                            if !matches!(self.tools.active_tool, ActiveTool::Tiles(_)) {
                                self.tools
                                    .set_tool(ActiveTool::Tiles(ActiveToolTiles::Brush));
                            }
                        }
                        MapLayerSkeleton::Quad(_) => {
                            if !matches!(self.tools.active_tool, ActiveTool::Quads(_)) {
                                self.tools
                                    .set_tool(ActiveTool::Quads(ActiveToolQuads::Brush));
                            }
                        }
                        MapLayerSkeleton::Sound(_) => {
                            if !matches!(self.tools.active_tool, ActiveTool::Sounds(_)) {
                                self.tools
                                    .set_tool(ActiveTool::Sounds(ActiveToolSounds::Brush));
                            }
                        }
                    },
                },
                None => {
                    // simply do nothing
                }
            }

            match &self.tools.active_tool {
                ActiveTool::Tiles(tool) => self.tools.tiles.render(
                    ui_canvas,
                    tool,
                    &self.backend_handle,
                    &self.stream_handle,
                    &self.canvas_handle,
                    &mut self.entities_container,
                    &self.fake_texture_array,
                    &tab.map,
                    &self.latest_pointer,
                    &self.latest_keys_down,
                    &self.current_pointer_pos,
                    &self.latest_unused_rect,
                ),
                ActiveTool::Quads(tool) => self.tools.quads.render(
                    ui_canvas,
                    tool,
                    &self.stream_handle,
                    &self.canvas_handle,
                    &tab.map,
                    &self.latest_pointer,
                    &self.current_pointer_pos,
                ),
                ActiveTool::Sounds(tool) => self.tools.sounds.render(
                    ui_canvas,
                    tool,
                    &self.stream_handle,
                    &self.canvas_handle,
                    &tab.map,
                    &self.latest_pointer,
                    &self.current_pointer_pos,
                ),
            }
        }
    }

    fn clone_anim_from_map<A, AP: DeserializeOwned + PartialOrd + Clone>(
        animations: &mut Vec<AnimBaseSkeleton<(), AP>>,
        from: &[AnimBaseSkeleton<A, AP>],
    ) where
        AnimBaseSkeleton<A, AP>: Into<AnimBase<AP>>,
    {
        animations.clear();
        animations.extend(from.iter().map(|anim| AnimBaseSkeleton {
            def: anim.def.clone(),
            user: (),
        }));
    }

    fn add_fake_anim_point(map: &mut EditorMap) {
        Self::clone_anim_from_map(
            &mut map.animations.user.animations.color,
            &map.animations.color,
        );
        Self::clone_anim_from_map(&mut map.animations.user.animations.pos, &map.animations.pos);
        Self::clone_anim_from_map(
            &mut map.animations.user.animations.sound,
            &map.animations.sound,
        );

        let anims = &map.animations.user.active_anims;
        let anim_points = &map.animations.user.active_anim_points;

        fn repl_or_insert<A: DeserializeOwned + Clone + Copy, const CHANNELS: usize>(
            animations: &mut [AnimBaseSkeleton<(), AnimPoint<A, CHANNELS>>],
            anim: &Option<(
                usize,
                AnimBase<AnimPoint<A, CHANNELS>>,
                EditorActiveAnimationProps,
            )>,
            anim_point: &Option<AnimPoint<A, CHANNELS>>,
            t: Duration,
        ) {
            if let Some((anim_index, _, _)) = anim.as_ref() {
                enum ReplOrInsert {
                    Repl(usize),
                    Insert(usize),
                }
                if let Some((mode, point)) = animations
                    .get(*anim_index)
                    .and_then(|anim| {
                        anim.def.points.iter().enumerate().find_map(|(p, point)| {
                            match point.time.cmp(&t) {
                                std::cmp::Ordering::Less => None,
                                std::cmp::Ordering::Equal => Some(ReplOrInsert::Repl(p)),
                                std::cmp::Ordering::Greater => Some(ReplOrInsert::Insert(p)),
                            }
                        })
                    })
                    .zip(anim_point.as_ref())
                {
                    match mode {
                        ReplOrInsert::Repl(index) => {
                            animations[*anim_index].def.points[index] = AnimPoint {
                                time: t,
                                curve_type: AnimPointCurveType::Linear,
                                value: point.value,
                            };
                        }
                        ReplOrInsert::Insert(index) => {
                            animations[*anim_index].def.points.insert(
                                index,
                                AnimPoint {
                                    time: t,
                                    curve_type: AnimPointCurveType::Linear,
                                    value: point.value,
                                },
                            );
                        }
                    }
                }
            }
        }
        repl_or_insert(
            &mut map.animations.user.animations.pos,
            &anims.pos,
            &anim_points.pos,
            map.user.ui_values.timeline.time(),
        );
        repl_or_insert(
            &mut map.animations.user.animations.color,
            &anims.color,
            &anim_points.color,
            map.user.ui_values.timeline.time(),
        );
        repl_or_insert(
            &mut map.animations.user.animations.sound,
            &anims.sound,
            &anim_points.sound,
            map.user.ui_values.timeline.time(),
        );
    }

    pub fn render_world(&mut self) {
        if let Some(tab) = self.tabs.get_mut(&self.active_tab) {
            // update anim if anim panel is open and e.g. quad selection is active
            if tab.map.user.ui_values.animations_panel_open {
                Self::add_fake_anim_point(&mut tab.map);
            }
        }
        let active_tab = self.tabs.get(&self.active_tab);
        if let Some(tab) = active_tab {
            let tile_index_texture = self.tile_textures.index.clone();
            let tile_flag_texture = self.tile_textures.flag.clone();
            // we use sound
            tab.map.user.sound_scene.stay_active();
            let mut group_clips: Vec<MapGroupAttr> = Default::default();
            let mut layer_rects: Vec<LayerRect> = Default::default();
            // bg
            self.render_design_groups(
                &tab.map_render,
                &tab.map,
                &tab.map.groups.background,
                tile_index_texture.clone(),
                tile_flag_texture.clone(),
                &mut group_clips,
                &mut layer_rects,
            );
            // physics
            Self::render_physics_group(
                &mut self.entities_container,
                &tab.map_render,
                &tab.map,
                &tab.map.groups.physics,
                tile_index_texture.clone(),
                tile_flag_texture.clone(),
                &mut layer_rects,
            );
            // fg
            self.render_design_groups(
                &tab.map_render,
                &tab.map,
                &tab.map.groups.foreground,
                tile_index_texture,
                tile_flag_texture,
                &mut group_clips,
                &mut layer_rects,
            );
            // group clips
            for group_clip in group_clips {
                self.render_group_clip(&tab.map, group_clip);
            }
            // layer rects
            for LayerRect {
                parallax,
                offset,
                width,
                height,
            } in layer_rects
            {
                self.render_tile_layer_rect(&tab.map, parallax, offset, width, height);
            }
            // sound update
            tab.map
                .user
                .global_sound_listener
                .update(tab.map.groups.user.pos);
        }
    }

    fn render_ui(
        &mut self,
        input: egui::RawInput,
        config: &ConfigEngine,
    ) -> (
        Option<egui::Rect>,
        Option<InputState>,
        Option<UiCanvasSize>,
        egui::PlatformOutput,
        Option<EditorResult>,
    ) {
        let mut unused_rect: Option<egui::Rect> = None;
        let mut input_state: Option<InputState> = None;
        let mut ui_canvas: Option<UiCanvasSize> = None;
        let egui_output = self.ui.render(EditorUiRenderPipe {
            cur_time: self.sys.time_get(),
            config,
            inp: input,
            editor_tabs: EditorTabsRefMut {
                tabs: &mut self.tabs,
                active_tab: &mut self.active_tab,
            },
            notifications: &self.notifications,
            ui_events: &mut self.ui_events,
            unused_rect: &mut unused_rect,
            input_state: &mut input_state,
            canvas_size: &mut ui_canvas,
            tools: &mut self.tools,
            editor_options: &mut self.editor_options,
            auto_mapper: &mut self.auto_mapper,
            io: &self.io,

            quad_tile_images_container: &mut self.quad_tile_images_container,
            sound_images_container: &mut self.sounds_container,
            container_scene: &self.container_scene,

            hotkeys: &mut self.hotkeys,
            cur_hotkey_events: &mut self.cur_hotkey_events,
            cached_binds_per_event: &mut self.cached_binds_per_event,
        });

        let mut forced_result = None;

        // handle ui events
        for ev in std::mem::take(&mut self.ui_events) {
            match ev {
                EditorUiEvent::NewMap => {
                    self.new_map("new-map", Default::default());
                }
                EditorUiEvent::OpenFile { name } => self.load_map(&name, Default::default()),
                EditorUiEvent::SaveFile { name } => {
                    self.save_map(&name);
                }
                EditorUiEvent::SaveCurMap => {
                    self.save_tab(&self.active_tab.clone());
                }
                EditorUiEvent::SaveMapAndClose { tab } => {
                    if self.save_tab(&tab) {
                        self.tabs.remove(&self.active_tab);
                    }
                }
                EditorUiEvent::SaveAll => {
                    self.save_all_tabs();
                }
                EditorUiEvent::SaveAllAndClose => {
                    if self.save_all_tabs() {
                        forced_result = Some(EditorResult::Close);
                    }
                }
                EditorUiEvent::HostMap(host_map) => {
                    let EditorUiEventHostMap {
                        map_path,
                        port,
                        password,
                        cert,
                        private_key,
                        mapper_name,
                        color,
                    } = *host_map;
                    self.load_map(
                        map_path.as_ref(),
                        MapLoadWithServerOptions {
                            cert: Some(NetworkServerCertMode::FromCertAndPrivateKey(Box::new(
                                NetworkServerCertAndKey { cert, private_key },
                            ))),
                            port: Some(port),
                            password: Some(password),
                            mapper_name: Some(mapper_name),
                            color: Some(color),
                            admin_password: None,
                        },
                    );
                }
                EditorUiEvent::Join {
                    ip_port,
                    cert_hash,
                    password,
                    mapper_name,
                    color,
                } => self.new_map(
                    &ip_port.clone(),
                    MapLoadOptions::WithoutServer {
                        server_addr: ip_port,
                        cert_hash: (0..cert_hash.len())
                            .step_by(2)
                            .map(|i| u8::from_str_radix(&cert_hash[i..i + 2], 16).unwrap())
                            .collect::<Vec<_>>()
                            .try_into()
                            .unwrap(),
                        password,
                        mapper_name,
                        color,
                    },
                ),
                EditorUiEvent::Close => {
                    if self.tabs.values().any(|t| t.client.should_save) {
                        self.ui.modal_dialog_mode = EditorModalDialogMode::CloseEditor;
                    } else {
                        forced_result = Some(EditorResult::Close);
                    }
                }
                EditorUiEvent::ForceClose => {
                    forced_result = Some(EditorResult::Close);
                }
                EditorUiEvent::Minimize => {
                    forced_result = Some(EditorResult::Minimize);
                }
                EditorUiEvent::Undo => {
                    if let Some(tab) = self.tabs.get(&self.active_tab) {
                        tab.client.undo();
                    }
                }
                EditorUiEvent::Redo => {
                    if let Some(tab) = self.tabs.get(&self.active_tab) {
                        tab.client.redo();
                    }
                }
                EditorUiEvent::CursorWorldPos { pos } => {
                    if let Some(tab) = self.tabs.get_mut(&self.active_tab) {
                        let now = self.sys.time_get();
                        // 50 times per sec
                        if tab.last_info_update.is_none_or(|last_info_update| {
                            now.saturating_sub(last_info_update) > Duration::from_millis(20)
                        }) {
                            tab.client.update_info(pos);
                            tab.last_info_update = Some(now);
                        }
                    }
                }
                EditorUiEvent::Chat { msg } => {
                    if let Some(tab) = self.tabs.get(&self.active_tab) {
                        tab.client.send_chat(msg);
                    }
                }
                EditorUiEvent::AdminAuth { password } => {
                    if let Some(tab) = self.tabs.get(&self.active_tab) {
                        tab.client.admin_auth(password);
                    }
                }
                EditorUiEvent::AdminChangeConfig { state } => {
                    if let Some(tab) = self.tabs.get(&self.active_tab) {
                        tab.client.admin_change_cfg(state);
                    }
                }
                EditorUiEvent::DbgAction(props) => {
                    if let Some(tab) = self.tabs.get(&self.active_tab) {
                        tab.client.dbg_action(props);
                    }
                }
            }
        }
        (
            unused_rect,
            input_state,
            ui_canvas,
            egui_output,
            forced_result,
        )
    }

    pub fn host_map(
        &mut self,
        path: &Path,
        port: u16,
        password: String,
        admin_password: String,
        cert_mode: NetworkServerCertMode,
    ) {
        self.load_map(
            path,
            MapLoadWithServerOptions {
                cert: Some(cert_mode),
                port: Some(port),
                password: Some(password),
                mapper_name: Some("server".to_string()),
                color: None,
                admin_password: Some(admin_password),
            },
        );

        if let Some(tab) = Self::path_to_tab_name(path)
            .ok()
            .and_then(|tab| self.tabs.get_mut(&tab))
        {
            tab.auto_saver.active = true;
            tab.auto_saver.interval = Some(Duration::from_secs(60 * 5));
        }
    }
}

#[derive(Serialize, Deserialize)]
pub enum EditorResult {
    Close,
    Minimize,
    PlatformOutput(egui::PlatformOutput),
}

pub trait EditorInterface {
    fn render(&mut self, input: egui::RawInput, config: &ConfigEngine) -> EditorResult;
}

impl EditorInterface for Editor {
    fn render(&mut self, input: egui::RawInput, config: &ConfigEngine) -> EditorResult {
        // do an update
        self.update();

        // then render the map
        self.render_world();

        // if msaa is enabled, consume them now
        self.backend_handle.consumble_multi_samples();

        // render the tools directly after the world
        // the handling/update of the tools & world happens after the UI tho
        self.render_tools(&self.latest_canvas_rect.clone());

        // then render the UI above it
        let (unused_rect, input_state, canvas_size, ui_output, forced_result) =
            self.render_ui(input, config);
        self.latest_canvas_rect = canvas_size.unwrap_or_else(|| {
            Rect::from_min_size(
                pos2(0.0, 0.0),
                vec2(
                    self.canvas_handle.canvas_width() as f32,
                    self.canvas_handle.canvas_height() as f32,
                ),
            )
        });

        // outside of the UI / inside of the world, handle brushes etc.
        // working with egui directly doesn't feel great... copy some interesting input values
        if let Some((latest_pointer, scroll_delta, keys, modifiers)) = input_state.map(|inp| {
            (
                inp.pointer.clone(),
                inp.raw_scroll_delta,
                inp.keys_down.clone(),
                inp.modifiers,
            )
        }) {
            if unused_rect.is_some_and(|unused_rect| {
                unused_rect.contains(
                    latest_pointer
                        .interact_pos()
                        .unwrap_or(self.current_pointer_pos),
                )
            }) {
                self.latest_keys_down = keys;
                self.latest_modifiers = modifiers;
                self.latest_pointer = latest_pointer;
                self.latest_unused_rect = unused_rect.unwrap();
                self.current_scroll_delta = scroll_delta;
                self.current_pointer_pos = self
                    .latest_pointer
                    .latest_pos()
                    .unwrap_or(self.current_pointer_pos);
                self.handle_world(
                    &canvas_size.unwrap_or_else(|| {
                        Rect::from_min_size(
                            pos2(0.0, 0.0),
                            vec2(
                                self.canvas_handle.canvas_width() as f32,
                                self.canvas_handle.canvas_height() as f32,
                            ),
                        )
                    }),
                    self.latest_unused_rect,
                );
            } else {
                self.current_scroll_delta = Default::default();
            }
        }

        if let Some(text) = ui_output.commands.iter().find_map(|c| {
            if let OutputCommand::CopyText(t) = c {
                Some(t)
            } else {
                None
            }
        }) {
            log::info!("[Editor] Copied the following text: {}", text);
        }

        // handle save tasks
        let mut unfinished_tasks = Vec::default();
        for task in self.save_tasks.drain(..) {
            if task.is_finished() {
                match task.get_storage() {
                    Ok(_) => {
                        log::info!("Map saved.");
                        self.notifications_overlay
                            .add_info("Map saved.", Duration::from_secs(2));
                        // ignore
                    }
                    Err(err) => {
                        log::error!("{err}");
                        self.notifications_overlay
                            .add_err(err.to_string(), Duration::from_secs(10));
                    }
                }
            } else {
                unfinished_tasks.push(task);
            }
        }
        std::mem::swap(&mut self.save_tasks, &mut unfinished_tasks);

        // render the overlay for notifications
        for ev in self.notifications.take() {
            match ev {
                EditorNotification::Error(msg) => {
                    log::error!("{msg}");
                    self.notifications_overlay
                        .add_err(msg, Duration::from_secs(10));
                }
                EditorNotification::Warning(msg) => {
                    log::warn!("{msg}");
                    self.notifications_overlay
                        .add_warn(msg, Duration::from_secs(10));
                }
                EditorNotification::Info(msg) => {
                    log::info!("{msg}");
                    self.notifications_overlay
                        .add_info(msg, Duration::from_secs(5));
                }
            }
        }
        self.notifications_overlay.render();

        if let Some(res) = forced_result {
            res
        } else {
            EditorResult::PlatformOutput(ui_output)
        }
    }
}
