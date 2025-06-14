use anyhow::anyhow;

#[derive(Debug, Clone)]
pub struct Particles06Part {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl Particles06Part {
    fn new(data: Vec<u8>, width: usize, height: usize) -> Self {
        Self {
            data,
            width: width as u32,
            height: height as u32,
        }
    }
}

#[derive(Debug)]
pub struct Particles06ConvertResult {
    pub slice: Particles06Part,
    pub ball: Particles06Part,
    pub splat: [Particles06Part; 3],

    pub smoke: Particles06Part,
    pub shell: Particles06Part,
    pub explosion: [Particles06Part; 1],
    pub airjump: Particles06Part,
    pub hit: [Particles06Part; 1],
}

fn single_img(
    particles_file: &[u8],
    x: usize,
    y: usize,
    sub_width: usize,
    sub_height: usize,
    pitch: usize,
) -> Particles06Part {
    let mut res: Vec<u8> = Default::default();

    let in_line = particles_file
        .split_at(y * pitch)
        .1
        .split_at(sub_height * pitch)
        .0;
    in_line.chunks(pitch).for_each(|chunk| {
        res.extend(chunk.split_at(x * 4).1.split_at(sub_width * 4).0);
    });

    Particles06Part::new(res, sub_width, sub_height)
}

/// splits the particles.png into its individual components
/// Additionally the width has to be divisible by 8
/// and the height by 8
pub fn split_06_particles(
    particles_file: &[u8],
    width: u32,
    height: u32,
) -> anyhow::Result<Particles06ConvertResult> {
    if width % 8 != 0 {
        Err(anyhow!("width is not divisible by 8"))
    } else if height % 8 != 0 {
        Err(anyhow!("height is not divisible by 8"))
    } else {
        let full_width = width as usize * 4; // * 4 for RGBA
        let segment_width = width as usize / 8;
        let segment_height = height as usize / 8;

        let part_slice = single_img(
            particles_file,
            0 * segment_width,
            0 * segment_height,
            1 * segment_width,
            1 * segment_height,
            full_width,
        );
        let part_ball = single_img(
            particles_file,
            1 * segment_width,
            0 * segment_height,
            1 * segment_width,
            1 * segment_height,
            full_width,
        );
        let part_splat01 = single_img(
            particles_file,
            2 * segment_width,
            0 * segment_height,
            1 * segment_width,
            1 * segment_height,
            full_width,
        );
        let part_splat02 = single_img(
            particles_file,
            3 * segment_width,
            0 * segment_height,
            1 * segment_width,
            1 * segment_height,
            full_width,
        );
        let part_splat03 = single_img(
            particles_file,
            4 * segment_width,
            0 * segment_height,
            1 * segment_width,
            1 * segment_height,
            full_width,
        );

        let part_smoke = single_img(
            particles_file,
            0 * segment_width,
            1 * segment_height,
            1 * segment_width,
            1 * segment_height,
            full_width,
        );
        let part_shell = single_img(
            particles_file,
            0 * segment_width,
            2 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );
        let part_expl01 = single_img(
            particles_file,
            0 * segment_width,
            4 * segment_height,
            4 * segment_width,
            4 * segment_height,
            full_width,
        );
        let part_airjump = single_img(
            particles_file,
            2 * segment_width,
            2 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );
        let part_hit01 = single_img(
            particles_file,
            4 * segment_width,
            1 * segment_height,
            2 * segment_width,
            2 * segment_height,
            full_width,
        );

        Ok(Particles06ConvertResult {
            slice: part_slice,
            ball: part_ball,
            splat: [part_splat01, part_splat02, part_splat03],

            smoke: part_smoke,
            shell: part_shell,
            explosion: [part_expl01],
            airjump: part_airjump,
            hit: [part_hit01],
        })
    }
}
