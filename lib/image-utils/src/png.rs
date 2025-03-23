use std::{borrow::Cow, io, num::NonZeroU32};

use image::RgbaImage;

#[derive(Debug)]
pub struct PngResultPersistentFast {
    width: u32,
    height: u32,
}

impl PngResultPersistentFast {
    pub fn to_persistent(self, data: Vec<u8>) -> PngResultPersistent {
        PngResultPersistent {
            data,
            width: self.width,
            height: self.height,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct PngResultPersistent {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct PngResult<'a> {
    pub data: &'a [u8],
    pub width: u32,
    pub height: u32,
}

impl PngResult<'_> {
    pub fn to_persistent(self) -> PngResultPersistent {
        PngResultPersistent {
            data: self.data.to_vec(),
            width: self.width,
            height: self.height,
        }
    }

    pub fn prepare_moved_persistent(self) -> PngResultPersistentFast {
        PngResultPersistentFast {
            width: self.width,
            height: self.height,
        }
    }
}

/// takes a closure of (width, height, color_channel_count)
pub fn load_png_image_as_rgba<'a, T>(file: &[u8], alloc_mem: T) -> io::Result<PngResult<'a>>
where
    T: FnOnce(usize, usize, usize) -> &'a mut [u8],
{
    use png::ColorType::*;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(file));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;

    let real_img_size = reader.output_buffer_size();
    let color_type = reader.output_color_type().0;

    let info = reader.info();
    let img_data = alloc_mem(info.width as usize, info.height as usize, 4);
    let info = reader.next_frame(img_data)?;

    let data = match color_type {
        Rgb => {
            let tmp = img_data[0..real_img_size].to_vec();
            for (index, ga) in tmp.chunks(3).enumerate() {
                img_data[index * 4] = ga[0];
                img_data[index * 4 + 1] = ga[1];
                img_data[index * 4 + 2] = ga[2];
                img_data[index * 4 + 3] = 255;
            }
            img_data
        }
        Rgba => img_data,
        Grayscale => {
            let tmp = img_data[0..real_img_size].to_vec();
            for (index, g) in tmp.iter().enumerate() {
                img_data[index * 4] = *g;
                img_data[index * 4 + 1] = *g;
                img_data[index * 4 + 2] = *g;
                img_data[index * 4 + 3] = 255;
            }
            img_data
        }
        GrayscaleAlpha => {
            let tmp = img_data[0..real_img_size].to_vec();
            for (index, ga) in tmp.chunks(2).enumerate() {
                let g = ga[0];
                let a = ga[1];
                img_data[index * 4] = g;
                img_data[index * 4 + 1] = g;
                img_data[index * 4 + 2] = g;
                img_data[index * 4 + 3] = a;
            }
            img_data
        }
        _ => return Err(io::Error::new(io::ErrorKind::Other, "uncovered color type")),
    };

    Ok(PngResult {
        data,
        width: info.width,
        height: info.height,
    })
}

#[derive(Debug, Clone, Copy)]
pub struct PngValidatorOptions {
    pub max_width: NonZeroU32,
    pub max_height: NonZeroU32,
    pub min_width: Option<NonZeroU32>,
    pub min_height: Option<NonZeroU32>,
    /// Whether the width must be divisible (without rest) by the given value
    pub divisible_width: Option<NonZeroU32>,
    /// Whether the height must be divisible (without rest) by the given value
    pub divisible_height: Option<NonZeroU32>,
}

impl Default for PngValidatorOptions {
    fn default() -> Self {
        // 2048x2048 should be a safe limit for games
        Self {
            max_width: 2048.try_into().unwrap(),
            max_height: 2048.try_into().unwrap(),
            min_width: None,
            min_height: None,
            divisible_width: None,
            divisible_height: None,
        }
    }
}

pub fn is_png_image_valid(file: &[u8], options: PngValidatorOptions) -> anyhow::Result<()> {
    let mut mem = Vec::new();
    let img = load_png_image_as_rgba(file, |w, h, ppp| {
        mem.resize(w * h * ppp, 0);
        &mut mem
    })?;
    anyhow::ensure!(
        img.width <= options.max_width.get() && img.height <= options.max_height.get(),
        "the maximum allowed width and height \
        for an image are currently: {} x {}",
        options.max_width,
        options.max_height
    );
    anyhow::ensure!(
        img.width > 0 && img.height > 0,
        "width and height must be >= 1"
    );
    anyhow::ensure!(
        options.min_width.is_none_or(|w| img.width >= w.get())
            && options.min_height.is_none_or(|h| img.height >= h.get()),
        "width and height must be at least {}x{}",
        options.min_width.map(|w| w.get()).unwrap_or(1),
        options.min_height.map(|h| h.get()).unwrap_or(1),
    );
    anyhow::ensure!(
        options
            .divisible_width
            .is_none_or(|d| img.width % d.get() == 0)
            && options
                .divisible_height
                .is_none_or(|d| img.height % d.get() == 0),
        "width and height must be divisible by {} - {}",
        options.divisible_width.map(|w| w.get()).unwrap_or(1),
        options.divisible_height.map(|h| h.get()).unwrap_or(1),
    );
    Ok(())
}

pub fn save_png_image_ex(
    raw_bytes: &[u8],
    width: u32,
    height: u32,
    compresion_best: bool,
) -> anyhow::Result<Vec<u8>> {
    use png::ColorType::*;
    let mut res: Vec<u8> = Default::default();
    let mut encoder = png::Encoder::new(&mut res, width, height);
    encoder.set_color(Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    if compresion_best {
        encoder.set_compression(png::Compression::Best);
    }
    let mut writer = encoder.write_header()?;

    writer.write_image_data(raw_bytes)?;

    writer.finish()?;

    Ok(res)
}

pub fn save_png_image(raw_bytes: &[u8], width: u32, height: u32) -> anyhow::Result<Vec<u8>> {
    save_png_image_ex(raw_bytes, width, height, false)
}

pub fn resize_rgba(
    img: Cow<[u8]>,
    width: u32,
    height: u32,
    new_width: u32,
    new_height: u32,
) -> Vec<u8> {
    image::imageops::resize(
        &RgbaImage::from_raw(width, height, img.into_owned()).unwrap(),
        new_width,
        new_height,
        image::imageops::FilterType::Lanczos3,
    )
    .to_vec()
}
