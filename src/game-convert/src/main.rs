use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use clap::Parser;
use client_extra::game_split::Game06Part;
use tar::Header;

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
    file: tar::Builder<Vec<u8>>,
}

enum WriteMode<'a> {
    Tar(&'a mut HashMap<String, TarFile>),
    Disk,
}

fn new_tar() -> TarFile {
    let mut builder = tar::Builder::new(Vec::new());
    builder.mode(tar::HeaderMode::Deterministic);
    TarFile { file: builder }
}

fn write_part(
    write_mode: &mut WriteMode<'_>,
    part: Game06Part,
    output: &Path,
    base_path: &str,
    name: &str,
) {
    let png = image_utils::png::save_png_image(&part.data, part.width, part.height).unwrap();
    match write_mode {
        WriteMode::Tar(files) => {
            let tar = files.entry(base_path.to_string()).or_insert_with(new_tar);

            let mut header = Header::new_gnu();
            header.set_cksum();
            header.set_size(png.len() as u64);
            header.set_mode(0o644);
            header.set_uid(1000);
            header.set_gid(1000);
            tar.file
                .append_data(
                    &mut header,
                    format!("{name}.png"),
                    std::io::Cursor::new(&png),
                )
                .unwrap();
        }
        WriteMode::Disk => {
            std::fs::write(output.join(base_path).join(format!("{name}.png")), png)
                .unwrap_or_else(|err| panic!("failed to write {name} in {output:?}: {err}"));
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
        client_extra::game_split::split_06_game(img.data, img.width, img.height).unwrap();

    let mut tar_files: HashMap<String, TarFile> = Default::default();
    let mut write_mode = if args.tar {
        WriteMode::Tar(&mut tar_files)
    } else {
        WriteMode::Disk
    };

    std::fs::create_dir_all(args.output.join("huds/")).unwrap();
    std::fs::create_dir_all(args.output.join("weapons/")).unwrap();
    std::fs::create_dir_all(args.output.join("hooks/")).unwrap();
    std::fs::create_dir_all(args.output.join("ctfs/")).unwrap();
    std::fs::create_dir_all(args.output.join("ninjas/")).unwrap();
    std::fs::create_dir_all(args.output.join("games/")).unwrap();
    if !args.tar {
        std::fs::create_dir_all(args.output.join("huds/default")).unwrap();
        std::fs::create_dir_all(args.output.join("weapons/default")).unwrap();
        std::fs::create_dir_all(args.output.join("hooks/default")).unwrap();
        std::fs::create_dir_all(args.output.join("ctfs/default")).unwrap();
        std::fs::create_dir_all(args.output.join("ninjas/default")).unwrap();
        std::fs::create_dir_all(args.output.join("games/default")).unwrap();
        std::fs::create_dir_all(args.output.join("weapons/default/hammer")).unwrap();
        std::fs::create_dir_all(args.output.join("weapons/default/gun")).unwrap();
        std::fs::create_dir_all(args.output.join("weapons/default/shotgun")).unwrap();
        std::fs::create_dir_all(args.output.join("weapons/default/grenade")).unwrap();
        std::fs::create_dir_all(args.output.join("weapons/default/laser")).unwrap();
        std::fs::create_dir_all(args.output.join("huds/default/vanilla")).unwrap();
        std::fs::create_dir_all(args.output.join("huds/default/ddrace")).unwrap();
    }

    write_part(
        &mut write_mode,
        converted.cursor_hammer,
        &args.output,
        "weapons/default",
        "hammer/cursor",
    );
    write_part(
        &mut write_mode,
        converted.cursor_gun,
        &args.output,
        "weapons/default",
        "gun/cursor",
    );
    write_part(
        &mut write_mode,
        converted.cursor_shotgun,
        &args.output,
        "weapons/default",
        "shotgun/cursor",
    );
    write_part(
        &mut write_mode,
        converted.cursor_grenade,
        &args.output,
        "weapons/default",
        "grenade/cursor",
    );
    write_part(
        &mut write_mode,
        converted.cursor_ninja,
        &args.output,
        "ninjas/default",
        "cursor",
    );
    write_part(
        &mut write_mode,
        converted.cursor_laser,
        &args.output,
        "weapons/default",
        "laser/cursor",
    );

    write_part(
        &mut write_mode,
        converted.weapon_hammer,
        &args.output,
        "weapons/default",
        "hammer/weapon",
    );
    write_part(
        &mut write_mode,
        converted.weapon_gun,
        &args.output,
        "weapons/default",
        "gun/weapon",
    );
    write_part(
        &mut write_mode,
        converted.weapon_shotgun,
        &args.output,
        "weapons/default",
        "shotgun/weapon",
    );
    write_part(
        &mut write_mode,
        converted.weapon_grenade,
        &args.output,
        "weapons/default",
        "grenade/weapon",
    );
    write_part(
        &mut write_mode,
        converted.weapon_ninja,
        &args.output,
        "ninjas/default",
        "weapon",
    );
    write_part(
        &mut write_mode,
        converted.weapon_laser,
        &args.output,
        "weapons/default",
        "laser/weapon",
    );

    write_part(
        &mut write_mode,
        converted.projectile_gun,
        &args.output,
        "weapons/default",
        "gun/projectile0",
    );
    write_part(
        &mut write_mode,
        converted.projectile_shotgun,
        &args.output,
        "weapons/default",
        "shotgun/projectile0",
    );
    write_part(
        &mut write_mode,
        converted.projectile_grenade,
        &args.output,
        "weapons/default",
        "grenade/projectile0",
    );
    write_part(
        &mut write_mode,
        converted.projectile_laser,
        &args.output,
        "weapons/default",
        "laser/projectile0",
    );

    converted
        .muzzle_gun
        .into_iter()
        .enumerate()
        .for_each(|(index, muzzle)| {
            write_part(
                &mut write_mode,
                muzzle,
                &args.output,
                "weapons/default",
                &format!("gun/muzzle{index}"),
            )
        });
    converted
        .muzzle_shotgun
        .into_iter()
        .enumerate()
        .for_each(|(index, muzzle)| {
            write_part(
                &mut write_mode,
                muzzle,
                &args.output,
                "weapons/default",
                &format!("shotgun/muzzle{index}"),
            )
        });
    converted
        .muzzle_ninja
        .into_iter()
        .enumerate()
        .for_each(|(index, muzzle)| {
            write_part(
                &mut write_mode,
                muzzle,
                &args.output,
                "ninjas/default",
                &format!("muzzle{index}"),
            )
        });
    if let Some(ninja_bar_full_left) = converted.ninja_bar_full_left {
        write_part(
            &mut write_mode,
            ninja_bar_full_left,
            &args.output,
            "ninjas/default",
            "ninja_bar_full_left",
        );
    }
    if let Some(ninja_bar_full) = converted.ninja_bar_full {
        write_part(
            &mut write_mode,
            ninja_bar_full,
            &args.output,
            "ninjas/default",
            "ninja_bar_full",
        );
    }
    if let Some(ninja_bar_empty) = converted.ninja_bar_empty {
        write_part(
            &mut write_mode,
            ninja_bar_empty,
            &args.output,
            "ninjas/default",
            "ninja_bar_empty",
        );
    }
    if let Some(ninja_bar_empty_right) = converted.ninja_bar_empty_right {
        write_part(
            &mut write_mode,
            ninja_bar_empty_right,
            &args.output,
            "ninjas/default",
            "ninja_bar_empty_right",
        );
    }

    write_part(
        &mut write_mode,
        converted.flag_blue,
        &args.output,
        "ctfs/default",
        "flag_blue",
    );
    write_part(
        &mut write_mode,
        converted.flag_red,
        &args.output,
        "ctfs/default",
        "flag_red",
    );

    write_part(
        &mut write_mode,
        converted.hook_chain,
        &args.output,
        "hooks/default",
        "hook_chain",
    );
    write_part(
        &mut write_mode,
        converted.hook_head,
        &args.output,
        "hooks/default",
        "hook_head",
    );

    write_part(
        &mut write_mode,
        converted.health_full,
        &args.output,
        "huds/default",
        "vanilla/heart",
    );
    write_part(
        &mut write_mode,
        converted.health_empty,
        &args.output,
        "huds/default",
        "vanilla/heart_empty",
    );
    write_part(
        &mut write_mode,
        converted.armor_full,
        &args.output,
        "huds/default",
        "vanilla/shield",
    );
    write_part(
        &mut write_mode,
        converted.armor_empty,
        &args.output,
        "huds/default",
        "vanilla/shield_empty",
    );

    write_part(
        &mut write_mode,
        converted.pickup_health,
        &args.output,
        "games/default",
        "heart",
    );
    write_part(
        &mut write_mode,
        converted.pickup_armor,
        &args.output,
        "games/default",
        "shield",
    );
    write_part(
        &mut write_mode,
        converted.star1,
        &args.output,
        "games/default",
        "star1",
    );
    write_part(
        &mut write_mode,
        converted.star2,
        &args.output,
        "games/default",
        "star2",
    );
    write_part(
        &mut write_mode,
        converted.star3,
        &args.output,
        "games/default",
        "star3",
    );
    if let Some(lose_shotgun) = converted.lose_shotgun {
        write_part(
            &mut write_mode,
            lose_shotgun,
            &args.output,
            "games/default",
            "lose_shotgun",
        );
    }
    if let Some(lose_grenade) = converted.lose_grenade {
        write_part(
            &mut write_mode,
            lose_grenade,
            &args.output,
            "games/default",
            "lose_grenade",
        );
    }
    if let Some(lose_laser) = converted.lose_laser {
        write_part(
            &mut write_mode,
            lose_laser,
            &args.output,
            "games/default",
            "lose_laser",
        );
    }
    if let Some(lose_ninja) = converted.lose_ninja {
        write_part(
            &mut write_mode,
            lose_ninja,
            &args.output,
            "games/default",
            "lose_ninja",
        );
    }

    for (name, file) in tar_files {
        let tar_file = file.file.into_inner().unwrap();
        std::fs::write(args.output.join(format!("{name}.tar")), tar_file).unwrap_or_else(|err| {
            panic!(
                "failed to write tar file {name} in {:?}: {err}",
                args.output
            )
        });
    }
}
