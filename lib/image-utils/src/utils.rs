use rayon::{
    prelude::{IndexedParallelIterator, ParallelIterator},
    slice::{ParallelSlice, ParallelSliceMut},
};

const TW_DILATE_ALPHA_THRESHOLD: u8 = 10;

pub fn dilate(
    thread_pool: &rayon::ThreadPool,
    w: usize,
    h: usize,
    bpp: usize,
    src_buff: &[u8],
    dest_buff: &mut [u8],
    alpha_threshold: u8,
) {
    let dirs_x = [0, -1, 1, 0];
    let dirs_y = [-1, 0, 0, 1];

    let alpha_comp_index = bpp - 1;

    thread_pool.install(|| {
        dest_buff
            .par_chunks_exact_mut(bpp)
            .enumerate()
            .take(w * h)
            .for_each(|(i, dst)| {
                let x = i % w;
                let y = i / w;

                let m = y * w * bpp + x * bpp;
                dst.copy_from_slice(&src_buff[m..(bpp + m)]);
                if src_buff[m + alpha_comp_index] > alpha_threshold {
                    return;
                }

                // clear pixels that are considered transparent
                // this allows the image to always be black where no dilate is needed
                dst[0..(bpp - 1)].fill(0);

                let mut sums_of_opaque = [0, 0, 0];
                let mut counter = 0;
                for c in 0..4 {
                    let ix = (x as i64 + dirs_x[c]).clamp(0, w as i64 - 1) as usize;
                    let iy = (y as i64 + dirs_y[c]).clamp(0, h as i64 - 1) as usize;
                    let k = iy * w * bpp + ix * bpp;
                    if src_buff[k + alpha_comp_index] > alpha_threshold {
                        for p in 0..bpp - 1 {
                            // Seems safe for BPP = 3, 4 which we use.
                            sums_of_opaque[p] += src_buff[k + p] as u32;
                        }
                        counter += 1;
                        break;
                    }
                }

                if counter > 0 {
                    for i in 0..bpp - 1 {
                        sums_of_opaque[i] /= counter;
                        dst[i] = sums_of_opaque[i] as u8;
                    }

                    dst[alpha_comp_index] = 255;
                }
            });
    });
}

fn copy_color_values(
    thread_pool: &rayon::ThreadPool,
    w: usize,
    h: usize,
    bpp: usize,
    src_buffer: &[u8],
    dest_buffer: &mut [u8],
) {
    thread_pool.install(|| {
        dest_buffer
            .par_chunks_exact_mut(bpp)
            .take(w * h)
            .zip(src_buffer.par_chunks_exact(bpp).take(w * h))
            .for_each(|(dst, src)| {
                if dst[bpp - 1] == 0 {
                    dst[0..bpp - 1].copy_from_slice(&src[0..bpp - 1]);
                }
            });
    });
}

#[allow(clippy::too_many_arguments)]
pub fn dilate_image_sub(
    thread_pool: &rayon::ThreadPool,
    img_buff: &mut [u8],
    w: usize,
    _h: usize,
    bpp: usize,
    x: usize,
    y: usize,
    sw: usize,
    sh: usize,
) {
    let [mut buffer_data1, mut buffer_data2] = [
        vec![0; sw * sh * std::mem::size_of::<u8>() * bpp],
        vec![0; sw * sh * std::mem::size_of::<u8>() * bpp],
    ];

    let mut buffer_data_original = vec![0; sw * sh * std::mem::size_of::<u8>() * bpp];

    let pixel_buffer_data = img_buff;

    thread_pool.install(|| {
        // fill buffer_data_original completely
        buffer_data_original
            .chunks_exact_mut(sw * bpp)
            .enumerate()
            .for_each(|(yh, chunk)| {
                let src_img_offset = ((y + yh) * w * bpp) + (x * bpp);

                chunk.copy_from_slice(
                    &pixel_buffer_data[src_img_offset..src_img_offset + chunk.len()],
                );
            });
    });

    dilate(
        thread_pool,
        sw,
        sh,
        bpp,
        buffer_data_original.as_slice(),
        buffer_data1.as_mut_slice(),
        TW_DILATE_ALPHA_THRESHOLD,
    );

    for _i in 0..5 {
        dilate(
            thread_pool,
            sw,
            sh,
            bpp,
            buffer_data1.as_slice(),
            buffer_data2.as_mut_slice(),
            TW_DILATE_ALPHA_THRESHOLD,
        );
        dilate(
            thread_pool,
            sw,
            sh,
            bpp,
            buffer_data2.as_slice(),
            buffer_data1.as_mut_slice(),
            TW_DILATE_ALPHA_THRESHOLD,
        );
    }

    copy_color_values(
        thread_pool,
        sw,
        sh,
        bpp,
        buffer_data1.as_slice(),
        buffer_data_original.as_mut_slice(),
    );

    thread_pool.install(|| {
        pixel_buffer_data
            .chunks_exact_mut(w * bpp)
            .skip(y)
            .take(sh)
            .enumerate()
            .for_each(|(yh, chunk)| {
                let src_img_offset = x * bpp;
                let dst_img_offset = yh * sw * bpp;
                let copy_size = sw * bpp;
                chunk[src_img_offset..src_img_offset + copy_size].copy_from_slice(
                    &buffer_data_original[dst_img_offset..dst_img_offset + copy_size],
                );
            });
    });
}

pub fn dilate_image(
    thread_pool: &rayon::ThreadPool,
    img_buff: &mut [u8],
    w: usize,
    h: usize,
    bpp: usize,
) {
    dilate_image_sub(thread_pool, img_buff, w, h, bpp, 0, 0, w, h);
}

#[allow(clippy::too_many_arguments)]
pub fn texture_2d_to_3d(
    thread_pool: &rayon::ThreadPool,
    img_buff: &[u8],
    image_width: usize,
    image_height: usize,
    image_color_channel_count: usize,
    split_count_width: usize,
    split_count_height: usize,
    target_3d_img_buff_data: &mut [u8],
    target_3d_img_width: &mut usize,
    target_3d_img_height: &mut usize,
) -> bool {
    *target_3d_img_width = image_width / split_count_width;
    *target_3d_img_height = image_height / split_count_height;

    let full_image_width = image_width * image_color_channel_count;

    let target_image_full_width = { *target_3d_img_width } * image_color_channel_count;
    thread_pool.install(|| {
        target_3d_img_buff_data
            .par_chunks_exact_mut(target_image_full_width)
            .enumerate()
            .for_each(|(index, write_chunk)| {
                let x_src = (index / *target_3d_img_height) % split_count_width;
                let y_src = index % *target_3d_img_height
                    + ((index / (split_count_width * *target_3d_img_height))
                        * *target_3d_img_height);
                let src_off = y_src * full_image_width + (x_src * target_image_full_width);

                write_chunk.copy_from_slice(&img_buff[src_off..src_off + target_image_full_width]);
            });
    });

    true
}

pub fn highest_bit(of_var_param: u32) -> u32 {
    let mut of_var = of_var_param;
    if of_var == 0 {
        return 0;
    }

    let mut ret_v = 1;

    loop {
        of_var >>= 1;
        if of_var == 0 {
            break;
        }
        ret_v <<= 1;
    }

    ret_v
}
