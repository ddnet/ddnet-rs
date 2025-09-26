use std::{rc::Rc, time::Duration};

use client_containers::{
    emoticons::{Emoticons, EmoticonsContainer},
    entities::EntitiesContainer,
    flags::FlagsContainer,
    hooks::{Hook, HookContainer},
    skins::{Skin, SkinContainer},
    weapons::{WeaponContainer, Weapons},
};
use client_render_base::{
    map::{
        map_pipeline::{MapGraphics, TileLayerDrawInfo},
        render_tools::RenderTools,
    },
    render::{
        animation::AnimState,
        default_anim::{base_anim, idle_anim},
        tee::{RenderTee, TeeRenderHands, TeeRenderInfo, TeeRenderSkinColor, offset_to_mid},
        toolkit::ToolkitRender,
    },
};
use egui::Rect;
use game_interface::types::{
    character_info::NetworkSkinInfo, emoticons::EmoticonType, render::character::TeeEye,
    resource_key::ResourceKey, weapons::WeaponType,
};
use graphics::{
    handles::{
        canvas::canvas::GraphicsCanvasHandle,
        shader_storage::shader_storage::ShaderStorage,
        stream::stream::GraphicsStreamHandle,
        stream_types::StreamedQuad,
        texture::texture::{TextureContainer, TextureContainer2dArray},
    },
    streaming::quad_scope_begin,
};
use graphics_types::rendering::{ColorRgba, State};
use math::math::vector::{dvec2, ubvec4, vec2};
use pool::mt_datatypes::PoolVec;
use tracing::instrument;
use ui_base::{custom_callback::CustomCallbackTrait, types::UiState};

/// TODO: this function exists in the editor already. graphics also have a similar one.
pub fn rotate(center: &vec2, rotation: f32, points: &mut [vec2]) {
    let c = rotation.cos();
    let s = rotation.sin();

    for point in points.iter_mut() {
        let x = point.x - center.x;
        let y = point.y - center.y;
        *point = vec2 {
            x: x * c - y * s + center.x,
            y: x * s + y * c + center.y,
        };
    }
}

#[instrument(level = "trace", skip_all)]
pub fn render_tee_for_ui_with_skin(
    canvas_handle: &GraphicsCanvasHandle,
    skin: Rc<Skin>,
    render_tee: &RenderTee,
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    render_rect: Rect,
    clip_rect: Option<Rect>,
    skin_info: Option<&NetworkSkinInfo>,
    pos: vec2,
    size: f32,
    eyes: TeeEye,
) {
    #[derive(Debug)]
    struct RenderTeeCb {
        render_rect: Rect,
        clip_rect: Option<Rect>,
        skin: Rc<Skin>,
        skin_info: Option<NetworkSkinInfo>,
        pos: vec2,
        size: f32,
        canvas_handle: GraphicsCanvasHandle,
        render_tee: RenderTee,
        eyes: TeeEye,
        opacity: f32,
    }
    impl CustomCallbackTrait for RenderTeeCb {
        fn render(&self) {
            let mut anim_state = AnimState::default();
            anim_state.set(&base_anim(), &Duration::from_millis(0));
            anim_state.add(&idle_anim(), &Duration::from_millis(0), 1.0);

            let (color_body, color_feet) = if let Some(NetworkSkinInfo::Custom {
                body_color,
                feet_color,
            }) = self.skin_info
            {
                (body_color.into(), feet_color.into())
            } else {
                (TeeRenderSkinColor::Original, TeeRenderSkinColor::Original)
            };

            let tee_render_info = TeeRenderInfo {
                color_body,
                color_feet,
                got_air_jump: false,
                feet_flipped: false,
                size: self.size,
                eye_left: self.eyes,
                eye_right: self.eyes,
            };

            let dir = vec2::new(1.0, 0.0);

            let mut state = State::new();
            state.map_canvas(
                self.render_rect.min.x,
                self.render_rect.min.y,
                self.render_rect.max.x,
                self.render_rect.max.y,
            );
            let scale_x = self.canvas_handle.canvas_width() as f32 / self.render_rect.width();
            let scale_y = self.canvas_handle.canvas_height() as f32 / self.render_rect.height();
            if let Some(clip_rect) = &self.clip_rect {
                state.clip_auto_rounding(
                    clip_rect.min.x * scale_x,
                    clip_rect.min.y * scale_y,
                    clip_rect.width() * scale_x,
                    clip_rect.height() * scale_y,
                );
            }

            self.render_tee.render_tee(
                &anim_state,
                &self.skin,
                &tee_render_info,
                &TeeRenderHands {
                    left: None,
                    right: None,
                },
                &dir,
                &(self.pos + offset_to_mid(&self.skin.metrics, &anim_state, &tee_render_info)),
                self.opacity,
                &state,
            );
        }
    }

    let cb = RenderTeeCb {
        render_rect,
        clip_rect,
        skin,
        skin_info: skin_info.copied(),
        pos,
        size,
        canvas_handle: canvas_handle.clone(),
        render_tee: render_tee.clone(),
        eyes,
        opacity: ui.opacity(),
    };

    ui_state.add_custom_paint(ui, render_rect, Rc::new(cb));
}

#[instrument(level = "trace", skip_all)]
pub fn render_tee_for_ui(
    canvas_handle: &GraphicsCanvasHandle,
    skin_container: &mut SkinContainer,
    render_tee: &RenderTee,
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    render_rect: Rect,
    clip_rect: Option<Rect>,
    skin: &ResourceKey,
    skin_info: Option<&NetworkSkinInfo>,
    pos: vec2,
    size: f32,
    eyes: TeeEye,
) {
    let skin = skin_container.get_or_default(skin);
    render_tee_for_ui_with_skin(
        canvas_handle,
        skin.clone(),
        render_tee,
        ui,
        ui_state,
        render_rect,
        clip_rect,
        skin_info,
        pos,
        size,
        eyes,
    )
}

#[instrument(level = "trace", skip_all)]
pub fn render_weapon_for_ui(
    canvas_handle: &GraphicsCanvasHandle,
    weapon_container: &mut WeaponContainer,
    render_toolkit: &ToolkitRender,
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    render_rect: Rect,
    clip_rect: Option<Rect>,
    weapon: &ResourceKey,
    weapon_ty: WeaponType,
    pos: vec2,
    size: f32,
) {
    #[derive(Debug)]
    struct RenderWeaponCb {
        render_rect: Rect,
        clip_rect: Option<Rect>,
        weapons: Weapons,
        weapon_ty: WeaponType,
        pos: vec2,
        size: f32,
        canvas_handle: GraphicsCanvasHandle,
        render_toolkit: ToolkitRender,
        opacity: f32,
    }
    impl CustomCallbackTrait for RenderWeaponCb {
        fn render(&self) {
            let mut state = quad_scope_begin();
            state.map_canvas(
                self.render_rect.min.x,
                self.render_rect.min.y,
                self.render_rect.max.x,
                self.render_rect.max.y,
            );
            let scale_x = self.canvas_handle.canvas_width() as f32 / self.render_rect.width();
            let scale_y = self.canvas_handle.canvas_height() as f32 / self.render_rect.height();
            if let Some(clip_rect) = &self.clip_rect {
                state.clip_auto_rounding(
                    clip_rect.min.x * scale_x,
                    clip_rect.min.y * scale_y,
                    clip_rect.width() * scale_x,
                    clip_rect.height() * scale_y,
                );
            }

            state.set_colors_from_single(1.0, 1.0, 1.0, self.opacity);

            self.render_toolkit.render_weapon(
                &self.weapons,
                &self.weapon_ty,
                &self.pos,
                self.size,
                &dvec2::new(1.0, 0.0),
                state,
            );
        }
    }

    let weapon = weapon_container.get_or_default(weapon);
    let cb = RenderWeaponCb {
        render_rect,
        clip_rect,
        weapons: weapon.clone(),
        weapon_ty,
        pos,
        size,
        canvas_handle: canvas_handle.clone(),
        render_toolkit: render_toolkit.clone(),
        opacity: ui.opacity(),
    };

    ui_state.add_custom_paint(ui, render_rect, Rc::new(cb));
}

#[instrument(level = "trace", skip_all)]
pub fn render_hook_for_ui(
    canvas_handle: &GraphicsCanvasHandle,
    hook_container: &mut HookContainer,
    render_toolkit: &ToolkitRender,
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    render_rect: Rect,
    clip_rect: Option<Rect>,
    hook: &ResourceKey,
    pos: vec2,
    size: f32,
) {
    #[derive(Debug)]
    struct RenderHookCb {
        render_rect: Rect,
        clip_rect: Option<Rect>,
        hook: Hook,
        pos: vec2,
        size: f32,
        canvas_handle: GraphicsCanvasHandle,
        render_toolkit: ToolkitRender,
        opacity: f32,
    }
    impl CustomCallbackTrait for RenderHookCb {
        fn render(&self) {
            let mut state = quad_scope_begin();
            state.map_canvas(
                self.render_rect.min.x,
                self.render_rect.min.y,
                self.render_rect.max.x,
                self.render_rect.max.y,
            );
            let scale_x = self.canvas_handle.canvas_width() as f32 / self.render_rect.width();
            let scale_y = self.canvas_handle.canvas_height() as f32 / self.render_rect.height();
            if let Some(clip_rect) = &self.clip_rect {
                state.clip_auto_rounding(
                    clip_rect.min.x * scale_x,
                    clip_rect.min.y * scale_y,
                    clip_rect.width() * scale_x,
                    clip_rect.height() * scale_y,
                );
            }

            state.set_colors_from_single(1.0, 1.0, 1.0, self.opacity);

            self.render_toolkit.render_hook(
                &self.hook,
                self.pos,
                self.pos + vec2::new(self.size * 2.0, 0.0),
                -vec2::new(1.0, 0.0),
                self.size,
                state,
            );
        }
    }

    let hook = hook_container.get_or_default(hook);
    let cb = RenderHookCb {
        render_rect,
        clip_rect,
        hook: hook.clone(),
        pos,
        size,
        canvas_handle: canvas_handle.clone(),
        render_toolkit: render_toolkit.clone(),
        opacity: ui.opacity(),
    };

    ui_state.add_custom_paint(ui, render_rect, Rc::new(cb));
}

#[instrument(level = "trace", skip_all)]
pub fn render_emoticon_for_ui(
    stream_handle: &GraphicsStreamHandle,
    canvas_handle: &GraphicsCanvasHandle,
    emoticon_container: &mut EmoticonsContainer,
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    render_rect: Rect,
    clip_rect: Option<Rect>,
    emoticon: &ResourceKey,
    pos: vec2,
    size: f32,
    ty: EmoticonType,
) {
    #[derive(Debug)]
    struct RenderEmoticonCb {
        render_rect: Rect,
        clip_rect: Option<Rect>,
        emoticons: Emoticons,
        pos: vec2,
        size: f32,
        canvas_handle: GraphicsCanvasHandle,
        stream_handle: GraphicsStreamHandle,
        ty: EmoticonType,
    }
    impl CustomCallbackTrait for RenderEmoticonCb {
        fn render(&self) {
            let mut state = State::new();
            state.map_canvas(
                self.render_rect.min.x,
                self.render_rect.min.y,
                self.render_rect.max.x,
                self.render_rect.max.y,
            );
            let scale_x = self.canvas_handle.canvas_width() as f32 / self.render_rect.width();
            let scale_y = self.canvas_handle.canvas_height() as f32 / self.render_rect.height();
            if let Some(clip_rect) = &self.clip_rect {
                state.clip_auto_rounding(
                    clip_rect.min.x * scale_x,
                    clip_rect.min.y * scale_y,
                    clip_rect.width() * scale_x,
                    clip_rect.height() * scale_y,
                );
            }

            pub fn render_rect(
                stream_handle: &GraphicsStreamHandle,
                center: &vec2,
                size: &vec2,
                state: State,
                texture: &TextureContainer,
            ) {
                stream_handle.render_quads(
                    &[StreamedQuad::default()
                        .from_pos_and_size(
                            vec2::new(center.x - size.x / 2.0, center.y - size.y / 2.0),
                            *size,
                        )
                        .color(ubvec4::new(255, 255, 255, 255))
                        .tex_free_form(
                            vec2::new(0.0, 0.0),
                            vec2::new(1.0, 0.0),
                            vec2::new(1.0, 1.0),
                            vec2::new(0.0, 1.0),
                        )],
                    state,
                    texture.into(),
                );
            }

            render_rect(
                &self.stream_handle,
                &self.pos,
                &vec2::new(self.size, self.size),
                state,
                &self.emoticons.emoticons[self.ty as usize],
            );
        }
    }

    let emoticons = emoticon_container.get_or_default(emoticon);
    let cb = RenderEmoticonCb {
        render_rect,
        clip_rect,
        emoticons: emoticons.clone(),
        pos,
        size,
        canvas_handle: canvas_handle.clone(),
        stream_handle: stream_handle.clone(),
        ty,
    };

    ui_state.add_custom_paint(ui, render_rect, Rc::new(cb));
}

#[instrument(level = "trace", skip_all)]
pub fn render_flag_for_ui(
    stream_handle: &GraphicsStreamHandle,
    canvas_handle: &GraphicsCanvasHandle,
    flags_container: &mut FlagsContainer,
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    render_rect: Rect,
    clip_rect: Option<Rect>,
    flags_key: &ResourceKey,
    flag_name: &str,
    pos: vec2,
    size: f32,
) {
    #[derive(Debug)]
    struct RenderFlagsCb {
        render_rect: Rect,
        clip_rect: Option<Rect>,
        flag: TextureContainer,
        pos: vec2,
        size: f32,
        stream_handle: GraphicsStreamHandle,
        canvas_handle: GraphicsCanvasHandle,
        opacity: f32,
    }
    impl CustomCallbackTrait for RenderFlagsCb {
        fn render(&self) {
            let mut state = State::new();
            state.map_canvas(
                self.render_rect.min.x,
                self.render_rect.min.y,
                self.render_rect.max.x,
                self.render_rect.max.y,
            );
            let scale_x = self.canvas_handle.canvas_width() as f32 / self.render_rect.width();
            let scale_y = self.canvas_handle.canvas_height() as f32 / self.render_rect.height();
            if let Some(clip_rect) = &self.clip_rect {
                state.clip_auto_rounding(
                    clip_rect.min.x * scale_x,
                    clip_rect.min.y * scale_y,
                    clip_rect.width() * scale_x,
                    clip_rect.height() * scale_y,
                );
            }

            RenderTools::render_rect(
                &self.stream_handle,
                &self.pos,
                &vec2::new(self.size, self.size / 2.0),
                &ubvec4::new(255, 255, 255, (self.opacity * 255.0) as u8),
                state,
                Some(&self.flag),
            );
        }
    }

    let flags = flags_container.get_or_default(flags_key);
    let cb = RenderFlagsCb {
        render_rect,
        clip_rect,
        flag: flags.get_or_default(flag_name).clone(),
        pos,
        size,
        canvas_handle: canvas_handle.clone(),
        stream_handle: stream_handle.clone(),
        opacity: ui.opacity(),
    };

    ui_state.add_custom_paint(ui, render_rect, Rc::new(cb));
}

#[instrument(level = "trace", skip_all)]
pub fn render_entities_for_ui(
    canvas_handle: &GraphicsCanvasHandle,
    entities_container: &mut EntitiesContainer,
    map_render: &MapGraphics,
    shader_storage: ShaderStorage,
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    render_rect: Rect,
    clip_rect: Option<Rect>,
    entities: &ResourceKey,
    pos: vec2,
    size: f32,
) {
    #[derive(Debug)]
    struct RenderEntitiesCb {
        render_rect: Rect,
        clip_rect: Option<Rect>,
        texture: TextureContainer2dArray,
        pos: vec2,
        size: f32,
        canvas_handle: GraphicsCanvasHandle,
        map_render: MapGraphics,
        shader_storage: ShaderStorage,
        opacity: f32,
    }
    impl CustomCallbackTrait for RenderEntitiesCb {
        fn render(&self) {
            let mut state = State::new();

            // render tiles
            // w or h doesn't matter bcs square
            let render_rect = Rect::from_center_size(
                egui::pos2(self.pos.x, self.pos.y),
                egui::vec2(self.size, self.size),
            );
            let size = render_rect.width();
            let size_ratio_x = 16.0 / size;
            let size_ratio_y = 16.0 / size;
            let tl_x = -render_rect.min.x * size_ratio_x;
            let tl_y = -render_rect.min.y * size_ratio_y;

            state.map_canvas(
                tl_x,
                tl_y,
                tl_x + self.render_rect.width() * size_ratio_x,
                tl_y + self.render_rect.height() * size_ratio_y,
            );

            let scale_x = self.canvas_handle.canvas_width() as f32 / self.render_rect.width();
            let scale_y = self.canvas_handle.canvas_height() as f32 / self.render_rect.height();
            if let Some(clip_rect) = &self.clip_rect {
                state.clip_auto_rounding(
                    clip_rect.min.x * scale_x,
                    clip_rect.min.y * scale_y,
                    clip_rect.width() * scale_x,
                    clip_rect.height() * scale_y,
                );
            }

            let color = ColorRgba::new(1.0, 1.0, 1.0, self.opacity);
            let shader_storage = &self.shader_storage;

            self.map_render.render_tile_layer(
                &state,
                (&self.texture).into(),
                shader_storage,
                &color,
                PoolVec::from_without_pool(
                    (0..16)
                        .map(|y| TileLayerDrawInfo {
                            quad_offset: y * 16,
                            quad_count: 16,
                            pos_y: y as f32,
                        })
                        .collect(),
                ),
            );
        }
    }

    let entities = entities_container.get_or_default(entities);
    let texture = entities.get_or_default("default");
    let cb = RenderEntitiesCb {
        render_rect,
        clip_rect,
        texture: texture.clone(),
        shader_storage,
        pos,
        size,
        canvas_handle: canvas_handle.clone(),
        map_render: map_render.clone(),
        opacity: ui.opacity(),
    };

    ui_state.add_custom_paint(ui, render_rect, Rc::new(cb));
}

#[instrument(level = "trace", skip_all)]
pub fn render_texture_for_ui(
    stream_handle: &GraphicsStreamHandle,
    canvas_handle: &GraphicsCanvasHandle,
    texture: &TextureContainer,
    ui: &mut egui::Ui,
    ui_state: &mut UiState,
    render_rect: Rect,
    clip_rect: Option<Rect>,
    pos: vec2,
    size: vec2,
    // tl, tr, br, bl
    uv: Option<(vec2, vec2, vec2, vec2)>,
) {
    #[derive(Debug)]
    struct RenderTextureCb {
        render_rect: Rect,
        clip_rect: Option<Rect>,
        texture: TextureContainer,
        pos: vec2,
        size: vec2,
        canvas_handle: GraphicsCanvasHandle,
        stream_handle: GraphicsStreamHandle,
        uv: Option<(vec2, vec2, vec2, vec2)>,
    }
    impl CustomCallbackTrait for RenderTextureCb {
        fn render(&self) {
            let mut state = State::new();
            state.map_canvas(
                self.render_rect.min.x,
                self.render_rect.min.y,
                self.render_rect.max.x,
                self.render_rect.max.y,
            );
            let scale_x = self.canvas_handle.canvas_width() as f32 / self.render_rect.width();
            let scale_y = self.canvas_handle.canvas_height() as f32 / self.render_rect.height();
            if let Some(clip_rect) = &self.clip_rect {
                state.clip_auto_rounding(
                    clip_rect.min.x * scale_x,
                    clip_rect.min.y * scale_y,
                    clip_rect.width() * scale_x,
                    clip_rect.height() * scale_y,
                );
            }

            pub fn render_rect(
                stream_handle: &GraphicsStreamHandle,
                center: &vec2,
                size: &vec2,
                uv: &Option<(vec2, vec2, vec2, vec2)>,
                state: State,
                texture: &TextureContainer,
            ) {
                let quad = StreamedQuad::default()
                    .from_pos_and_size(
                        vec2::new(center.x - size.x / 2.0, center.y - size.y / 2.0),
                        *size,
                    )
                    .color(ubvec4::new(255, 255, 255, 255));
                let quad = if let Some((tl, tr, br, bl)) = *uv {
                    quad.tex_free_form(tl, tr, br, bl)
                } else {
                    quad.tex_free_form(
                        vec2::new(0.0, 0.0),
                        vec2::new(1.0, 0.0),
                        vec2::new(1.0, 1.0),
                        vec2::new(0.0, 1.0),
                    )
                };
                stream_handle.render_quads(&[quad], state, texture.into());
            }

            render_rect(
                &self.stream_handle,
                &self.pos,
                &self.size,
                &self.uv,
                state,
                &self.texture,
            );
        }
    }

    let cb = RenderTextureCb {
        render_rect,
        clip_rect,
        texture: texture.clone(),
        pos,
        size,
        uv,
        canvas_handle: canvas_handle.clone(),
        stream_handle: stream_handle.clone(),
    };

    ui_state.add_custom_paint(ui, render_rect, Rc::new(cb));
}
