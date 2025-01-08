use std::{num::NonZeroUsize, path::PathBuf, sync::Arc};

use clap::Parser;
use image::utils::dilate_image;
use oxipng::optimize_from_memory;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// file name of the png to dilate.
    file: PathBuf,
    /// optional output file.
    output: Option<PathBuf>,
    /// automatic png optimization
    #[arg(short, long, default_value_t = true, action = clap::ArgAction::Set)]
    optimize: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let thread_pool = Arc::new(
        rayon::ThreadPoolBuilder::new()
            .num_threads(
                std::thread::available_parallelism()
                    .unwrap_or(NonZeroUsize::new(2).unwrap())
                    .get(),
            )
            .build()
            .unwrap(),
    );

    let file = tokio::fs::read(&args.file).await.unwrap();

    let mut mem = Vec::new();
    let img = image::png::load_png_image_as_rgba(&file, |w, h, bpp| {
        assert!(bpp == 4, "png must be RGBA.");
        mem = vec![0; w * h * bpp];
        &mut mem
    })
    .unwrap();

    let width = img.width as usize;
    let height = img.height as usize;

    dilate_image(&thread_pool, &mut mem, width, height, 4);

    let path = if let Some(fp) = args.output.clone() {
        fp
    } else {
        args.file.clone()
    };

    let mut file = image::png::save_png_image_ex(&mem, width as u32, height as u32, true).unwrap();

    if args.optimize {
        file = optimize_from_memory(&file, &oxipng::Options::default()).unwrap();
    }

    tokio::fs::write(path, file).await.unwrap();
}
