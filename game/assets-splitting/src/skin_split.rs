use anyhow::anyhow;

#[derive(Debug, Clone)]
pub struct Skin06Part {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Skin06Part {
    fn new(data: Vec<u8>, width: usize, height: usize) -> Self {
        Self {
            data,
            width: width as u32,
            height: height as u32,
        }
    }
}

#[derive(Debug)]
pub struct Skin06ConvertResult {
    pub body: Skin06Part,
    pub body_outline: Skin06Part,

    pub hand: Skin06Part,
    pub hand_outline: Skin06Part,

    pub foot: Skin06Part,
    pub foot_outline: Skin06Part,

    pub eye_normal: Skin06Part,
    pub eye_angry: Skin06Part,
    pub eye_pain: Skin06Part,
    pub eye_happy: Skin06Part,
    pub eye_dead: Skin06Part,
    pub eye_surprised: Skin06Part,

    pub watermark: Skin06Part,
}

/// skin texture is like this (assuming 256x128)
///
/// |-------------------------------------------------------------------------------------|
/// |                        |                                  | 32x32    | 32x32        |
/// |                        |                                  | hand     | hand_outline |
/// |                        |                                  |-------------------------|
/// |  96x96                 |   96x96                          | 64x32 foot              |
/// |  body                  |   body_outline                   |-------------------------|
/// |                        |                                  | 64x32 foot_outline      |
/// |-------------------------------------------------------------------------------------|
/// | 64x32     | 32x32      | 32x32     | 32x32    | 32x32     | 32x32    | 32x32        |
/// | watermark | eye normal | eye angry | eye pain | eye happy | eye dead | eye surprise |
/// |-------------------------------------------------------------------------------------|
///
/// Additionally the width has to be divisible by 8 and the height by 4
pub fn split_06_skin(
    skin_file: &[u8],
    width: u32,
    height: u32,
) -> anyhow::Result<Skin06ConvertResult> {
    if width % 8 != 0 {
        Err(anyhow!("width is not divisible by 8"))
    } else if height % 4 != 0 {
        Err(anyhow!("height is not divisible by 4"))
    } else {
        let mut body: Vec<u8> = Default::default();
        let mut body_outline: Vec<u8> = Default::default();
        let mut hand: Vec<u8> = Default::default();
        let mut hand_outline: Vec<u8> = Default::default();
        let mut foot: Vec<u8> = Default::default();
        let mut foot_outline: Vec<u8> = Default::default();
        let mut watermark: Vec<u8> = Default::default();
        let mut eye_normal: Vec<u8> = Default::default();
        let mut eye_angry: Vec<u8> = Default::default();
        let mut eye_pain: Vec<u8> = Default::default();
        let mut eye_happy: Vec<u8> = Default::default();
        let mut eye_dead: Vec<u8> = Default::default();
        let mut eye_surprise: Vec<u8> = Default::default();
        let full_width = width as usize * 4; // * 4 for RGBA
        let segment_width = width as usize / 8;
        let segment_full_width = segment_width * 4; // * 4 for RGBA
        let segment_height = height as usize / 4;
        skin_file
            .chunks_exact(full_width)
            .enumerate()
            .for_each(|(y, y_chunk)| {
                if y < segment_height * 3 {
                    let (body_seg, rest) = y_chunk.split_at(segment_full_width * 3);
                    body.extend(body_seg);
                    let (body_outline_seg, rest) = rest.split_at(segment_full_width * 3);
                    body_outline.extend(body_outline_seg);

                    if y < segment_height {
                        let (hand_seg, rest) = rest.split_at(segment_full_width);
                        hand.extend(hand_seg);
                        let (hand_outline_seg, _) = rest.split_at(segment_full_width);
                        hand_outline.extend(hand_outline_seg);
                    } else if y < segment_height * 2 {
                        let (foot_seg, _) = rest.split_at(segment_full_width * 2);
                        foot.extend(foot_seg);
                    } else {
                        let (foot_outline_seg, _) = rest.split_at(segment_full_width * 2);
                        foot_outline.extend(foot_outline_seg);
                    }
                } else {
                    let (water_mark_seg, rest) = y_chunk.split_at(segment_full_width * 2);
                    watermark.extend(water_mark_seg);

                    let (eye_normal_seg, rest) = rest.split_at(segment_full_width);
                    eye_normal.extend(eye_normal_seg);
                    let (eye_angry_seg, rest) = rest.split_at(segment_full_width);
                    eye_angry.extend(eye_angry_seg);
                    let (eye_pain_seg, rest) = rest.split_at(segment_full_width);
                    eye_pain.extend(eye_pain_seg);
                    let (eye_happy_seg, rest) = rest.split_at(segment_full_width);
                    eye_happy.extend(eye_happy_seg);
                    let (eye_dead_seg, rest) = rest.split_at(segment_full_width);
                    eye_dead.extend(eye_dead_seg);
                    let (eye_surprise_seg, _) = rest.split_at(segment_full_width);
                    eye_surprise.extend(eye_surprise_seg);
                }
            });
        Ok(Skin06ConvertResult {
            body: Skin06Part::new(body, segment_width * 3, segment_height * 3),
            body_outline: Skin06Part::new(body_outline, segment_width * 3, segment_height * 3),

            hand: Skin06Part::new(hand, segment_width, segment_height),
            hand_outline: Skin06Part::new(hand_outline, segment_width, segment_height),

            foot: Skin06Part::new(foot, segment_width * 2, segment_height),
            foot_outline: Skin06Part::new(foot_outline, segment_width * 2, segment_height),

            eye_normal: Skin06Part::new(eye_normal, segment_width, segment_height),
            eye_angry: Skin06Part::new(eye_angry, segment_width, segment_height),
            eye_pain: Skin06Part::new(eye_pain, segment_width, segment_height),
            eye_happy: Skin06Part::new(eye_happy, segment_width, segment_height),
            eye_dead: Skin06Part::new(eye_dead, segment_width, segment_height),
            eye_surprised: Skin06Part::new(eye_surprise, segment_width, segment_height),

            watermark: Skin06Part::new(watermark, segment_width * 2, segment_height),
        })
    }
}
