use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use assets_base::tar::{TarBuilder, new_tar, tar_add_file};
use assets_splitting::emoticon_split::Emoticon06Part;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// file name of the emoticon
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

fn write_part(write_mode: &mut WriteMode<'_>, part: Emoticon06Part, output: &Path, name: &str) {
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
        assets_splitting::emoticon_split::split_06_emoticon(img.data, img.width, img.height)
            .unwrap();

    let mut tar_files: HashMap<String, TarFile> = Default::default();
    let mut write_mode = if args.tar {
        WriteMode::Tar(&mut tar_files)
    } else {
        WriteMode::Disk
    };

    std::fs::create_dir_all(&args.output).unwrap();

    write_part(&mut write_mode, converted.oop, &args.output, "oop");
    write_part(
        &mut write_mode,
        converted.exclamation,
        &args.output,
        "exclamation",
    );
    write_part(&mut write_mode, converted.hearts, &args.output, "hearts");
    write_part(&mut write_mode, converted.drop, &args.output, "drop");
    write_part(&mut write_mode, converted.dotdot, &args.output, "dotdot");
    write_part(&mut write_mode, converted.music, &args.output, "music");
    write_part(&mut write_mode, converted.sorry, &args.output, "sorry");
    write_part(&mut write_mode, converted.ghost, &args.output, "ghost");
    write_part(&mut write_mode, converted.sushi, &args.output, "sushi");
    write_part(
        &mut write_mode,
        converted.splattee,
        &args.output,
        "splattee",
    );
    write_part(
        &mut write_mode,
        converted.deviltee,
        &args.output,
        "deviltee",
    );
    write_part(&mut write_mode, converted.zomg, &args.output, "zomg");
    write_part(&mut write_mode, converted.zzz, &args.output, "zzz");
    write_part(&mut write_mode, converted.wtf, &args.output, "wtf");
    write_part(&mut write_mode, converted.eyes, &args.output, "eyes");
    write_part(
        &mut write_mode,
        converted.question,
        &args.output,
        "question",
    );

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
