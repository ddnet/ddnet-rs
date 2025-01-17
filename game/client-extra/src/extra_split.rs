use anyhow::anyhow;

#[derive(Debug, Clone)]
pub struct Extras06Part {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Extras06Part {
    fn new(data: Vec<u8>, width: usize, height: usize) -> Self {
        Self {
            data,
            width: width as u32,
            height: height as u32,
        }
    }
}

#[derive(Debug)]
pub struct Extras06ConvertResult {
    pub snowflake: Extras06Part,
    pub sparkle: Extras06Part,
}

fn single_img(
    file: &[u8],
    x: usize,
    y: usize,
    sub_width: usize,
    sub_height: usize,
    pitch: usize,
) -> Extras06Part {
    let mut res: Vec<u8> = Default::default();

    let in_line = file.split_at(y * pitch).1.split_at(sub_height * pitch).0;
    in_line.chunks(pitch).for_each(|chunk| {
        res.extend(chunk.split_at(x * 4).1.split_at(sub_width * 4).0);
    });

    Extras06Part::new(res, sub_width, sub_height)
}

/// Splits the extras.png into its individual components.
///
/// The width has to be divisible by 16
/// and the height by 16.
pub fn split_06_extras(
    file: &[u8],
    width: u32,
    height: u32,
) -> anyhow::Result<Extras06ConvertResult> {
    if width % 16 != 0 {
        Err(anyhow!("width is not divisible by 16"))
    } else if height % 16 != 0 {
        Err(anyhow!("height is not divisible by 16"))
    } else {
        let full_width = width as usize * 4; // * 4 for RGBA
        let segment_width = width as usize / 16;
        let segment_height = height as usize / 16;

        let snowflake = single_img(
            file,
            0 * segment_width,
            0 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );
        let sparkle = single_img(
            file,
            2 * segment_width,
            0 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );

        Ok(Extras06ConvertResult { snowflake, sparkle })
    }
}
