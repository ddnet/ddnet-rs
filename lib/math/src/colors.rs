use palette::FromColor;

use crate::math::vector::ubvec4;

const DARKEST: f64 = 0.5;

pub fn legacy_color_to_rgba(
    legacy_color_code: i32,
    ignore_alpha: bool,
    clamp_lightness: bool,
) -> ubvec4 {
    let a = (legacy_color_code >> 24) & 0xFF;
    let h = ((legacy_color_code >> 16) & 0xFF) as f64 / 255.0;
    let s = ((legacy_color_code >> 8) & 0xFF) as f64 / 255.0;
    let l = ((legacy_color_code) & 0xFF) as f64 / 255.0;

    let mut hsl = palette::Hsl::new(h * 360.0, s, l);

    if clamp_lightness {
        hsl.lightness = DARKEST + hsl.lightness * (1.0 - DARKEST);
    }

    let mut rgb = palette::rgb::LinSrgb::from_color(hsl);

    // clamp
    rgb.red = rgb.red.clamp(0.0, 1.0);
    rgb.blue = rgb.blue.clamp(0.0, 1.0);
    rgb.green = rgb.green.clamp(0.0, 1.0);

    ubvec4::new(
        (rgb.red * 255.0).round() as u8,
        (rgb.green * 255.0).round() as u8,
        (rgb.blue * 255.0).round() as u8,
        if ignore_alpha { 255 } else { a as u8 },
    )
}

pub fn rgba_to_legacy_color(rgba: ubvec4, ignore_alpha: bool, unclamp_lightness: bool) -> i32 {
    let rgb = palette::rgb::LinSrgb::from_components((
        rgba.r() as f64 / 255.0,
        rgba.g() as f64 / 255.0,
        rgba.b() as f64 / 255.0,
    ));

    let mut hsl = palette::Hsl::from_color(rgb);

    if unclamp_lightness {
        hsl.lightness = (hsl.lightness - DARKEST) / (1.0 - DARKEST);
    }

    let h: f64 = hsl.hue.into_inner();
    let h = ((h.rem_euclid(360.0) / 360.0) * 255.0).round() as u8;
    let s = (hsl.saturation * 255.0).round() as u8;
    let l = (hsl.lightness * 255.0).round() as u8;

    (if ignore_alpha { 0 } else { rgba.a() as i32 }) << 24
        | (h as i32) << 16
        | (s as i32) << 8
        | l as i32
}

#[cfg(test)]
mod test {
    use crate::{
        colors::{legacy_color_to_rgba, rgba_to_legacy_color},
        math::vector::ubvec4,
    };

    #[test]
    fn back_and_forth() {
        let test_rgb = ubvec4::new(255, 0, 255, 255);
        let legacy = rgba_to_legacy_color(test_rgb, true, true);
        let rgb = legacy_color_to_rgba(legacy, true, true);

        // the conversion is slightly lossy, because the hue is pressed into a u8
        assert!(ubvec4::new(255, 0, 252, 255) == rgb);
    }

    #[test]
    fn purple() {
        let test_rgb = ubvec4::new(90, 0, 255, 255);
        let legacy = rgba_to_legacy_color(test_rgb, true, true);
        let rgb = legacy_color_to_rgba(legacy, true, true);

        assert!(legacy == 12189440);
        assert!(test_rgb == rgb);
        assert!(legacy_color_to_rgba(12189440, true, true) == test_rgb);
    }
}
