use anyhow::anyhow;

#[derive(Debug, Clone)]
pub struct Emoticon06Part {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Emoticon06Part {
    fn new(data: Vec<u8>, width: usize, height: usize) -> Self {
        Self {
            data,
            width: width as u32,
            height: height as u32,
        }
    }
}

#[derive(Debug)]
pub struct Emoticon06ConvertResult {
    pub oop: Emoticon06Part,
    pub exclamation: Emoticon06Part,
    pub hearts: Emoticon06Part,
    pub drop: Emoticon06Part,
    pub dotdot: Emoticon06Part,
    pub music: Emoticon06Part,
    pub sorry: Emoticon06Part,
    pub ghost: Emoticon06Part,
    pub sushi: Emoticon06Part,
    pub splattee: Emoticon06Part,
    pub deviltee: Emoticon06Part,
    pub zomg: Emoticon06Part,
    pub zzz: Emoticon06Part,
    pub wtf: Emoticon06Part,
    pub eyes: Emoticon06Part,
    pub question: Emoticon06Part,
}

/// splits the emoticon file into its 16 individual parts
/// Additionally the width & height have to be divisible by 4
pub fn split_06_emoticon(
    emoticon_file: &[u8],
    width: u32,
    height: u32,
) -> anyhow::Result<Emoticon06ConvertResult> {
    if width % 4 != 0 {
        Err(anyhow!("width is not divisible by 4"))
    } else if height % 4 != 0 {
        Err(anyhow!("height is not divisible by 4"))
    } else {
        let mut oop: Vec<u8> = Default::default();
        let mut exclamation: Vec<u8> = Default::default();
        let mut hearts: Vec<u8> = Default::default();
        let mut drop: Vec<u8> = Default::default();
        let mut dotdot: Vec<u8> = Default::default();
        let mut music: Vec<u8> = Default::default();
        let mut sorry: Vec<u8> = Default::default();
        let mut ghost: Vec<u8> = Default::default();
        let mut sushi: Vec<u8> = Default::default();
        let mut splattee: Vec<u8> = Default::default();
        let mut deviltee: Vec<u8> = Default::default();
        let mut zomg: Vec<u8> = Default::default();
        let mut zzz: Vec<u8> = Default::default();
        let mut wtf: Vec<u8> = Default::default();
        let mut eyes: Vec<u8> = Default::default();
        let mut question: Vec<u8> = Default::default();

        let full_width = width as usize * 4; // * 4 for RGBA
        let segment_width = width as usize / 4;
        let segment_full_width = segment_width * 4; // * 4 for RGBA
        let segment_height = height as usize / 4;
        emoticon_file
            .chunks_exact(full_width)
            .enumerate()
            .for_each(|(y, y_chunk)| {
                if y < segment_height {
                    let rest = y_chunk;

                    let (emoticon_part, rest) = rest.split_at(segment_full_width);
                    oop.extend(emoticon_part);
                    let (emoticon_part, rest) = rest.split_at(segment_full_width);
                    exclamation.extend(emoticon_part);
                    let (emoticon_part, rest) = rest.split_at(segment_full_width);
                    hearts.extend(emoticon_part);
                    let (emoticon_part, _) = rest.split_at(segment_full_width);
                    drop.extend(emoticon_part);
                } else if y < segment_height * 2 {
                    let rest = y_chunk;

                    let (emoticon_part, rest) = rest.split_at(segment_full_width);
                    dotdot.extend(emoticon_part);
                    let (emoticon_part, rest) = rest.split_at(segment_full_width);
                    music.extend(emoticon_part);
                    let (emoticon_part, rest) = rest.split_at(segment_full_width);
                    sorry.extend(emoticon_part);
                    let (emoticon_part, _) = rest.split_at(segment_full_width);
                    ghost.extend(emoticon_part);
                } else if y < segment_height * 3 {
                    let rest = y_chunk;

                    let (emoticon_part, rest) = rest.split_at(segment_full_width);
                    sushi.extend(emoticon_part);
                    let (emoticon_part, rest) = rest.split_at(segment_full_width);
                    splattee.extend(emoticon_part);
                    let (emoticon_part, rest) = rest.split_at(segment_full_width);
                    deviltee.extend(emoticon_part);
                    let (emoticon_part, _) = rest.split_at(segment_full_width);
                    zomg.extend(emoticon_part);
                } else if y < segment_height * 4 {
                    let rest = y_chunk;

                    let (emoticon_part, rest) = rest.split_at(segment_full_width);
                    zzz.extend(emoticon_part);
                    let (emoticon_part, rest) = rest.split_at(segment_full_width);
                    wtf.extend(emoticon_part);
                    let (emoticon_part, rest) = rest.split_at(segment_full_width);
                    eyes.extend(emoticon_part);
                    let (emoticon_part, _) = rest.split_at(segment_full_width);
                    question.extend(emoticon_part);
                }
            });
        Ok(Emoticon06ConvertResult {
            oop: Emoticon06Part::new(oop, segment_width, segment_height),
            exclamation: Emoticon06Part::new(exclamation, segment_width, segment_height),
            hearts: Emoticon06Part::new(hearts, segment_width, segment_height),
            drop: Emoticon06Part::new(drop, segment_width, segment_height),
            dotdot: Emoticon06Part::new(dotdot, segment_width, segment_height),
            music: Emoticon06Part::new(music, segment_width, segment_height),
            sorry: Emoticon06Part::new(sorry, segment_width, segment_height),
            ghost: Emoticon06Part::new(ghost, segment_width, segment_height),
            sushi: Emoticon06Part::new(sushi, segment_width, segment_height),
            splattee: Emoticon06Part::new(splattee, segment_width, segment_height),
            deviltee: Emoticon06Part::new(deviltee, segment_width, segment_height),
            zomg: Emoticon06Part::new(zomg, segment_width, segment_height),
            zzz: Emoticon06Part::new(zzz, segment_width, segment_height),
            wtf: Emoticon06Part::new(wtf, segment_width, segment_height),
            eyes: Emoticon06Part::new(eyes, segment_width, segment_height),
            question: Emoticon06Part::new(question, segment_width, segment_height),
        })
    }
}
