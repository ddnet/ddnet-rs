use client_render_base::map::render_tools::RenderTools;
use graphics_types::rendering::ColorRgba;
use map::map::groups::layers::tiles::MapTileLayerAttr;

use crate::map::{EditorMap, EditorMapInterface};

pub const TILE_VISUAL_SIZE: f32 = 1.0;

pub fn get_animated_color(map: &EditorMap, attr: Option<&MapTileLayerAttr>) -> ColorRgba {
    match attr {
        Some(attr) => {
            let attr_color = ColorRgba::new(
                attr.color.r().to_num(),
                attr.color.g().to_num(),
                attr.color.b().to_num(),
                attr.color.a().to_num(),
            );
            if let Some(color_anim) = attr.color_anim {
                let time = map.user.render_time();
                let anim = &map.active_animations().color[color_anim];
                let anim_pos = RenderTools::render_eval_anim(
                    anim.def.points.as_slice(),
                    time::Duration::try_from(time).unwrap(),
                    map.user.include_last_anim_point(),
                );
                ColorRgba::new(
                    attr_color.r * anim_pos.r().to_num::<f32>(),
                    attr_color.g * anim_pos.g().to_num::<f32>(),
                    attr_color.b * anim_pos.b().to_num::<f32>(),
                    attr_color.a * anim_pos.a().to_num::<f32>(),
                )
            } else {
                attr_color
            }
        }
        None => ColorRgba::new(1.0, 1.0, 1.0, 1.0),
    }
}
