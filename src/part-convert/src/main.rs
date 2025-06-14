use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use assets_base::tar::{new_tar, tar_add_file, TarBuilder};
use assets_splitting::particles_split::Particles06Part;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// file name of the particles
    file: String,
    /// output path (directory)
    output: PathBuf,
    /// Put the resulting assets into a tar archieve.
    #[arg(short, long, default_value_t = false, action = clap::ArgAction::Set)]
    tar: bool,
}

struct TarFile {
    file: TarBuilder,
}

enum WriteMode<'a> {
    Tar(&'a mut HashMap<String, TarFile>),
    Disk,
}

fn write_part(write_mode: &mut WriteMode<'_>, part: Particles06Part, output: &Path, name: &str) {
    let png = image_utils::png::save_png_image(&part.data, part.width, part.height).unwrap();
    match write_mode {
        WriteMode::Tar(files) => {
            let tar = files
                .entry(output.to_string_lossy().to_string())
                .or_insert_with(|| TarFile { file: new_tar() });

            tar_add_file(&mut tar.file, format!("{name}.png"), &png);
        }
        WriteMode::Disk => {
            std::fs::write(output.join(format!("{name}.png")), png).unwrap();
        }
    }
}

fn main() {
    let args = Args::parse();

    let file = std::fs::read(args.file).unwrap();
    let mut mem: Vec<u8> = Default::default();
    let img: image_utils::png::PngResult<'_> =
        image_utils::png::load_png_image_as_rgba(&file, |width, height, bytes_per_pixel| {
            mem.resize(width * height * bytes_per_pixel, Default::default());
            &mut mem
        })
        .unwrap();
    let converted =
        assets_splitting::particles_split::split_06_particles(img.data, img.width, img.height)
            .unwrap();

    let mut tar_files: HashMap<String, TarFile> = Default::default();
    let mut write_mode = if args.tar {
        WriteMode::Tar(&mut tar_files)
    } else {
        WriteMode::Disk
    };

    std::fs::create_dir_all(args.output.clone()).unwrap();

    write_part(&mut write_mode, converted.slice, &args.output, "slice_001");
    write_part(&mut write_mode, converted.ball, &args.output, "ball_001");

    converted
        .splat
        .into_iter()
        .enumerate()
        .for_each(|(index, splat)| {
            write_part(
                &mut write_mode,
                splat,
                &args.output,
                &format!("splat_{:03}", index + 1),
            )
        });

    write_part(&mut write_mode, converted.smoke, &args.output, "smoke_001");
    write_part(&mut write_mode, converted.shell, &args.output, "shell_001");

    converted
        .explosion
        .into_iter()
        .enumerate()
        .for_each(|(index, explosion)| {
            write_part(
                &mut write_mode,
                explosion,
                &args.output,
                &format!("explosion_{:03}", index + 1),
            )
        });

    write_part(
        &mut write_mode,
        converted.airjump,
        &args.output,
        "airjump_001",
    );

    converted
        .hit
        .into_iter()
        .enumerate()
        .for_each(|(index, hit)| {
            write_part(
                &mut write_mode,
                hit,
                &args.output,
                &format!("hit_{:03}", index + 1),
            )
        });

    for (name, file) in tar_files {
        let tar_file = file.file.into_inner().unwrap();
        std::fs::write(format!("{name}.tar"), tar_file).unwrap_or_else(|err| {
            panic!(
                "failed to write tar file {name} in {:?}: {err}",
                args.output
            )
        });
    }
}
