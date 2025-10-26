use assets_splitting::skin_split::Skin06Part;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// file name of the skin
    file: String,
    /// output path (directory)
    output: String,
}

fn write_part(part: Skin06Part, output: &str, name: &str) {
    let png = image_utils::png::save_png_image(&part.data, part.width, part.height).unwrap();
    std::fs::write(output.to_string() + "/" + name + ".png", png).unwrap();
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
    let converted = assets_splitting::skin_split::split_06_skin(
        &rayon::ThreadPoolBuilder::new().build().unwrap(),
        img.data,
        img.width,
        img.height,
    )
    .unwrap();

    std::fs::create_dir_all(&args.output).unwrap();
    std::fs::create_dir_all(&(args.output.clone() + "/eyes_left")).unwrap();
    std::fs::create_dir_all(&(args.output.clone() + "/eyes_right")).unwrap();

    write_part(converted.body, &args.output, "body");
    write_part(converted.body_outline, &args.output, "body_outline");

    write_part(converted.hand.clone(), &args.output, "hand_left");
    write_part(
        converted.hand_outline.clone(),
        &args.output,
        "hand_left_outline",
    );
    write_part(converted.hand, &args.output, "hand_right");
    write_part(converted.hand_outline, &args.output, "hand_right_outline");

    write_part(converted.foot.clone(), &args.output, "foot_left");
    write_part(
        converted.foot_outline.clone(),
        &args.output,
        "foot_left_outline",
    );
    write_part(converted.foot, &args.output, "foot_right");
    write_part(converted.foot_outline, &args.output, "foot_right_outline");

    write_part(
        converted.eye_normal.clone(),
        &args.output,
        "eyes_left/normal",
    );
    write_part(converted.eye_blink.clone(), &args.output, "eyes_left/blink");
    write_part(converted.eye_angry.clone(), &args.output, "eyes_left/angry");
    write_part(converted.eye_pain.clone(), &args.output, "eyes_left/pain");
    write_part(converted.eye_happy.clone(), &args.output, "eyes_left/happy");
    write_part(converted.eye_dead.clone(), &args.output, "eyes_left/dead");
    write_part(
        converted.eye_surprised.clone(),
        &args.output,
        "eyes_left/surprised",
    );

    write_part(converted.eye_normal, &args.output, "eyes_right/normal");
    write_part(converted.eye_blink, &args.output, "eyes_right/blink");
    write_part(converted.eye_angry, &args.output, "eyes_right/angry");
    write_part(converted.eye_pain, &args.output, "eyes_right/pain");
    write_part(converted.eye_happy, &args.output, "eyes_right/happy");
    write_part(converted.eye_dead, &args.output, "eyes_right/dead");
    write_part(
        converted.eye_surprised,
        &args.output,
        "eyes_right/surprised",
    );

    write_part(converted.watermark, &args.output, "watermark");
}
