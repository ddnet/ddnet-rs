use ffmpeg_next::{
    ChannelLayout,
    format::{self, Pixel},
    util,
};
use sound::frame_fetcher_plugin::BackendAudioFrame;

pub struct FrameConverter {
    width: u32,
    height: u32,
    converter: ffmpeg_next::software::scaling::Context,
    render_frame: util::frame::Video,
}

impl FrameConverter {
    pub fn new(resolution: (u32, u32)) -> Result<Self, ffmpeg_next::Error> {
        let (width, height) = resolution;
        let converter =
            ffmpeg_next::software::converter((width, height), format::Pixel::RGBA, Pixel::NV12)?;
        let render_frame = util::frame::Video::new(format::Pixel::RGBA, width, height);
        Ok(Self {
            width,
            height,
            converter,
            render_frame,
        })
    }

    pub fn process(
        &mut self,
        frame: &[u8],
        frame_index: i64,
    ) -> Result<util::frame::Video, ffmpeg_next::Error> {
        let mut encode_frame = util::frame::Video::new(Pixel::NV12, self.width, self.height);
        self.render_frame.data_mut(0).copy_from_slice(frame);
        encode_frame.set_pts(Some(frame_index));
        self.converter.run(&self.render_frame, &mut encode_frame)?;
        Ok(encode_frame)
    }
}

pub struct AudioConverter;

impl AudioConverter {
    pub fn process(
        &mut self,
        frame: &[BackendAudioFrame],
        frame_index: i64,
    ) -> Result<util::frame::Audio, ffmpeg_next::Error> {
        let mut encode_frame = util::frame::Audio::new(
            format::Sample::F32(format::sample::Type::Planar),
            frame.len(),
            ChannelLayout::default(2),
        );
        for (i, f) in frame.iter().enumerate() {
            let left = f.left.to_le_bytes();
            let right = f.right.to_le_bytes();
            let left_off = i * std::mem::size_of::<f32>();
            let right_off = i * std::mem::size_of::<f32>();
            encode_frame.data_mut(0)[left_off..left_off + std::mem::size_of::<f32>()]
                .copy_from_slice(&left);
            // dunno but the data_mut function of ffmpeg_next is just wrong
            // it uses the linesize of index 1, which for whatever reason
            // is not intitialized
            unsafe {
                std::slice::from_raw_parts_mut(
                    (*encode_frame.as_mut_ptr()).data[1],
                    (*encode_frame.as_ptr()).linesize[0] as usize,
                )[right_off..right_off + std::mem::size_of::<f32>()]
                    .copy_from_slice(&right)
            }
        }
        encode_frame.set_pts(Some(frame_index));
        Ok(encode_frame)
    }
}
