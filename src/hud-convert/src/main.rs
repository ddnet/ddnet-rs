use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use assets_base::tar::{TarBuilder, new_tar, tar_add_file};
use assets_splitting::ddrace_hud_split::DdraceHudPart;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// file name of the game
    file: PathBuf,
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

fn write_part(write_mode: &mut WriteMode<'_>, part: DdraceHudPart, output: &Path, name: &str) {
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
        assets_splitting::ddrace_hud_split::split_ddrace_hud(img.data, img.width, img.height)
            .unwrap();

    let mut tar_files: HashMap<String, TarFile> = Default::default();
    let mut write_mode = if args.tar {
        WriteMode::Tar(&mut tar_files)
    } else {
        WriteMode::Disk
    };

    std::fs::create_dir_all(args.output.join("huds/default/ddrace")).unwrap();

    write_part(
        &mut write_mode,
        converted.jump,
        &args.output,
        "huds/default/ddrace/jump",
    );
    write_part(
        &mut write_mode,
        converted.jump_used,
        &args.output,
        "huds/default/ddrace/jump_used",
    );
    write_part(
        &mut write_mode,
        converted.solo,
        &args.output,
        "huds/default/ddrace/solo",
    );
    write_part(
        &mut write_mode,
        converted.collision_off,
        &args.output,
        "huds/default/ddrace/collision_off",
    );
    write_part(
        &mut write_mode,
        converted.endless_jump,
        &args.output,
        "huds/default/ddrace/endless_jump",
    );
    write_part(
        &mut write_mode,
        converted.endless_hook,
        &args.output,
        "huds/default/ddrace/endless_hook",
    );
    write_part(
        &mut write_mode,
        converted.jetpack,
        &args.output,
        "huds/default/ddrace/jetpack",
    );

    write_part(
        &mut write_mode,
        converted.freeze_full_left,
        &args.output,
        "huds/default/ddrace/freeze_full_left",
    );
    write_part(
        &mut write_mode,
        converted.freeze_full,
        &args.output,
        "huds/default/ddrace/freeze_full",
    );
    write_part(
        &mut write_mode,
        converted.freeze_empty,
        &args.output,
        "huds/default/ddrace/freeze_empty",
    );
    write_part(
        &mut write_mode,
        converted.freeze_empty_right,
        &args.output,
        "huds/default/ddrace/freeze_empty_right",
    );
    write_part(
        &mut write_mode,
        converted.disabled_hook_others,
        &args.output,
        "huds/default/ddrace/disabled_hook_others",
    );
    write_part(
        &mut write_mode,
        converted.disabled_hammer,
        &args.output,
        "huds/default/ddrace/disabled_hammer",
    );
    write_part(
        &mut write_mode,
        converted.disabled_shotgun,
        &args.output,
        "huds/default/ddrace/disabled_shotgun",
    );
    write_part(
        &mut write_mode,
        converted.disabled_grenade,
        &args.output,
        "huds/default/ddrace/disabled_grenade",
    );
    write_part(
        &mut write_mode,
        converted.disabled_laser,
        &args.output,
        "huds/default/ddrace/disabled_laser",
    );
    write_part(
        &mut write_mode,
        converted.disabled_gun,
        &args.output,
        "huds/default/ddrace/disabled_gun",
    );

    write_part(
        &mut write_mode,
        converted.ninja_full_left,
        &args.output,
        "huds/default/ddrace/ninja_full_left",
    );
    write_part(
        &mut write_mode,
        converted.ninja_full,
        &args.output,
        "huds/default/ddrace/ninja_full",
    );
    write_part(
        &mut write_mode,
        converted.ninja_empty,
        &args.output,
        "huds/default/ddrace/ninja_empty",
    );
    write_part(
        &mut write_mode,
        converted.ninja_empty_right,
        &args.output,
        "huds/default/ddrace/ninja_empty_right",
    );
    write_part(
        &mut write_mode,
        converted.tele_grenade,
        &args.output,
        "huds/default/ddrace/tele_grenade",
    );
    write_part(
        &mut write_mode,
        converted.tele_pistol,
        &args.output,
        "huds/default/ddrace/tele_pistol",
    );
    write_part(
        &mut write_mode,
        converted.tele_laser,
        &args.output,
        "huds/default/ddrace/tele_laser",
    );
    write_part(
        &mut write_mode,
        converted.deep_frozen,
        &args.output,
        "huds/default/ddrace/deep_frozen",
    );
    write_part(
        &mut write_mode,
        converted.live_frozen,
        &args.output,
        "huds/default/ddrace/live_frozen",
    );

    write_part(
        &mut write_mode,
        converted.disabled_finish,
        &args.output,
        "huds/default/ddrace/disabled_finish",
    );
    write_part(
        &mut write_mode,
        converted.dummy_hammer,
        &args.output,
        "huds/default/ddrace/dummy_hammer",
    );
    write_part(
        &mut write_mode,
        converted.dummy_copy,
        &args.output,
        "huds/default/ddrace/dummy_copy",
    );
    write_part(
        &mut write_mode,
        converted.stage_locked,
        &args.output,
        "huds/default/ddrace/stage_locked",
    );
    write_part(
        &mut write_mode,
        converted.team0_mode,
        &args.output,
        "huds/default/ddrace/team0_mode",
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
