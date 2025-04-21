use palette::FromColor;

use crate::math::vector::ubvec4;

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
        let darkest = 0.5;
        hsl.lightness = darkest + hsl.lightness * (1.0 - darkest);
    }

    let mut rgb = palette::rgb::LinSrgb::from_color(hsl);

    // clamp
    rgb.red = rgb.red.clamp(0.0, 1.0);
    rgb.blue = rgb.blue.clamp(0.0, 1.0);
    rgb.green = rgb.green.clamp(0.0, 1.0);

    ubvec4::new(
        (rgb.red * 255.0) as u8,
        (rgb.green * 255.0) as u8,
        (rgb.blue * 255.0) as u8,
        if ignore_alpha { 255 } else { a as u8 },
    )
}

pub fn rgba_to_legacy_color(rgba: ubvec4) -> i32 {
    let rgb = palette::rgb::LinSrgb::from_components((
        rgba.r() as f64 / 255.0,
        rgba.g() as f64 / 255.0,
        rgba.b() as f64 / 255.0,
    ));

    let hsl = palette::Hsl::from_color(rgb);

    let h: f64 = hsl.hue.into_inner();
    let h = ((h / 360.0) * 255.0) as u8;
    let s = (hsl.saturation * 255.0) as u8;
    let l = (hsl.lightness * 255.0) as u8;

    (rgba.a() as i32) << 24 | (h as i32) << 16 | (s as i32) << 8 | l as i32
}
