use std::{path::Path, ptr};

use ffmpeg_next::{
    codec, encoder,
    ffi::{
        AVBufferRef, AVHWDeviceType, AVHWFramesContext, av_buffer_ref, av_buffer_unref,
        av_hwdevice_ctx_create, av_hwframe_ctx_alloc, av_hwframe_ctx_init, av_hwframe_get_buffer,
        av_hwframe_transfer_data,
    },
    format::{self, Pixel},
    util,
};

use crate::types::EncoderSettings;

use super::utils::{as_res, non_null};

pub trait AvEncoder<A> {
    fn encode_frame(
        &mut self,
        octx: &mut format::context::Output,
        frame: &A,
    ) -> Result<(), ffmpeg_next::Error>;
}

pub struct VideoEncoder {
    encoder: codec::encoder::video::Encoder,
    hw_ctx: Option<*mut AVBufferRef>,
    hw_frame: Option<ffmpeg_next::Frame>,
    packet: ffmpeg_next::Packet,
}

unsafe impl Send for VideoEncoder {}

impl VideoEncoder {
    fn process_packets(
        &mut self,
        octx: &mut format::context::Output,
    ) -> Result<(), ffmpeg_next::Error> {
        while let Ok(()) = self.encoder.receive_packet(&mut self.packet) {
            self.packet.set_stream(0);
            self.packet.rescale_ts(
                self.encoder.time_base(),
                octx.stream(0)
                    .ok_or(ffmpeg_next::Error::External)?
                    .time_base(),
            );
            self.packet.write(octx)?;
        }
        Ok(())
    }
}

impl AvEncoder<util::frame::Video> for VideoEncoder {
    fn encode_frame(
        &mut self,
        octx: &mut format::context::Output,
        frame: &util::frame::Video,
    ) -> Result<(), ffmpeg_next::Error> {
        if let Some(hw_frame) = &mut self.hw_frame {
            unsafe {
                as_res(av_hwframe_transfer_data(
                    hw_frame.as_mut_ptr(),
                    frame.as_ptr(),
                    0,
                ))?;
            }
            hw_frame.set_pts(frame.pts());
            self.encoder.send_frame(hw_frame)?;
        } else {
            self.encoder.send_frame(frame)?;
        }
        self.process_packets(octx)?;
        Ok(())
    }
}

pub struct AudioEncoder {
    pub(crate) encoder: codec::encoder::audio::Encoder,
    packet: ffmpeg_next::Packet,
}

impl AudioEncoder {
    fn process_packets(
        &mut self,
        octx: &mut format::context::Output,
    ) -> Result<(), ffmpeg_next::Error> {
        while let Ok(()) = self.encoder.receive_packet(&mut self.packet) {
            self.packet.set_stream(1);
            self.packet.rescale_ts(
                self.encoder.time_base(),
                octx.stream(1)
                    .ok_or(ffmpeg_next::Error::External)?
                    .time_base(),
            );
            self.packet.write_interleaved(octx)?;
        }
        Ok(())
    }
}

impl AvEncoder<util::frame::Audio> for AudioEncoder {
    fn encode_frame(
        &mut self,
        octx: &mut format::context::Output,
        frame: &util::frame::Audio,
    ) -> Result<(), ffmpeg_next::Error> {
        self.encoder.send_frame(frame)?;
        self.process_packets(octx)?;
        Ok(())
    }
}

pub struct Encoder {
    pub(crate) octx: format::context::Output,

    pub(crate) video: VideoEncoder,
    pub(crate) audio: AudioEncoder,
}

impl Encoder {
    pub fn new(file_path: &Path, settings: &EncoderSettings) -> Result<Self, ffmpeg_next::Error> {
        ffmpeg_next::init()?;
        ffmpeg_next::log::set_level(ffmpeg_next::log::Level::Info);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).map_err(|_| ffmpeg_next::Error::External)?;
        }
        let mut octx = format::output(file_path)?;

        let global_header = octx.format().flags().contains(format::Flags::GLOBAL_HEADER);

        let video = Self::video_encoder(&mut octx, settings, global_header)?;
        let audio = Self::audio_encoder(&mut octx, settings, global_header)?;

        format::context::output::dump(
            &octx,
            0,
            Some(
                file_path
                    .as_os_str()
                    .to_str()
                    .ok_or(ffmpeg_next::Error::External)?,
            ),
        );
        octx.write_header()?;

        Ok(Self { octx, video, audio })
    }

    fn video_encoder(
        octx: &mut format::context::Output,
        settings: &EncoderSettings,
        global_header: bool,
    ) -> anyhow::Result<VideoEncoder, ffmpeg_next::Error> {
        let codec = match settings.hw_accel.as_str() {
            "vaapi" => "h264_vaapi",
            "cuda" => "h264_nvenc",
            "amf" => "h264_amf",
            _ => "libx264",
        };
        let codec = encoder::find_by_name(codec).ok_or(ffmpeg_next::Error::EncoderNotFound)?;
        let mut encoder = codec::context::Context::new_with_codec(codec)
            .encoder()
            .video()?;

        let format = match settings.hw_accel.as_str() {
            "vaapi" => Pixel::VAAPI,
            "cuda" => Pixel::CUDA,
            "amf" => Pixel::YUV420P,
            _ => Pixel::NV12,
        };
        encoder.set_format(format);
        encoder.set_width(settings.width);
        encoder.set_height(settings.height);
        let frame_rate = ffmpeg_next::Rational::new(settings.fps as i32, 1);
        encoder.set_frame_rate(Some(frame_rate));
        encoder.set_time_base(frame_rate.invert());
        if global_header {
            encoder.set_flags(codec::Flags::GLOBAL_HEADER);
        }

        let mut hw_frame = None;
        let mut hw_ctx = None;

        let hw_accel_ty = match settings.hw_accel.as_str() {
            "vaapi" => Some(AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI),
            "cuda" => Some(AVHWDeviceType::AV_HWDEVICE_TYPE_CUDA),
            "amf" => Some(AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA),
            _ => None,
        };
        if let Some(ty) = hw_accel_ty {
            unsafe {
                encoder.set_max_b_frames(0);
                let mut device_ctx: *mut AVBufferRef = ptr::null_mut();
                as_res(av_hwdevice_ctx_create(
                    &mut device_ctx,
                    ty,
                    ptr::null(),
                    ptr::null_mut(),
                    0,
                ))?;
                let hw_buffer_ref = non_null(av_hwframe_ctx_alloc(device_ctx))?;
                let hw_frames_ctx = (*hw_buffer_ref).data as *mut AVHWFramesContext;
                (*hw_frames_ctx).format = format.into();
                (*hw_frames_ctx).sw_format = Pixel::NV12.into();
                (*hw_frames_ctx).width = settings.width as i32;
                (*hw_frames_ctx).height = settings.height as i32;
                as_res(av_hwframe_ctx_init(hw_buffer_ref))?;
                (*encoder.as_mut_ptr()).hw_frames_ctx = non_null(av_buffer_ref(hw_buffer_ref))?;
                let mut frame = ffmpeg_next::Frame::empty();
                as_res(av_hwframe_get_buffer(
                    (*encoder.as_mut_ptr()).hw_frames_ctx,
                    frame.as_mut_ptr(),
                    0,
                ))?;
                hw_ctx = Some(device_ctx);
                hw_frame = Some(frame);
            }
        }
        let mut options = ffmpeg_next::Dictionary::new();
        options.set("preset", "ultrafast");
        options.set("crf", &settings.crf.to_string());
        options.set("x264-params", "bframes=8");
        let encoder = encoder.open_with(options)?;

        let mut ost = octx.add_stream_with(&encoder)?;
        ost.set_time_base(frame_rate.invert());

        let packet = ffmpeg_next::Packet::empty();
        Ok(VideoEncoder {
            encoder,
            hw_ctx,
            hw_frame,
            packet,
        })
    }

    fn audio_encoder(
        octx: &mut format::context::Output,
        settings: &EncoderSettings,
        global_header: bool,
    ) -> anyhow::Result<AudioEncoder, ffmpeg_next::Error> {
        let codec = encoder::find_by_name("aac").ok_or(ffmpeg_next::Error::EncoderNotFound)?;
        let mut encoder = codec::context::Context::new_with_codec(codec)
            .encoder()
            .audio()?;

        encoder.set_format(format::Sample::F32(format::sample::Type::Planar));
        encoder.set_rate(settings.sample_rate as i32);
        encoder.set_channel_layout(ffmpeg_next::ChannelLayout::default(2));
        let frame_rate = ffmpeg_next::Rational::new(settings.sample_rate as i32, 1);
        //encoder.set_frame_rate(Some(frame_rate));
        encoder.set_time_base(frame_rate.invert());
        if global_header {
            encoder.set_flags(codec::Flags::GLOBAL_HEADER);
        }

        let encoder = encoder.open()?;

        let mut ost = octx.add_stream_with(&encoder)?;
        ost.set_time_base(frame_rate.invert());

        let packet = ffmpeg_next::Packet::empty();
        Ok(AudioEncoder { encoder, packet })
    }

    pub fn encode_eof(&mut self) -> Result<(), ffmpeg_next::Error> {
        self.video.encoder.send_eof()?;
        self.audio.encoder.send_eof()?;
        self.video.process_packets(&mut self.octx)?;
        self.audio.process_packets(&mut self.octx)?;
        self.octx.write_trailer()?;
        if let Some(ctx) = &mut self.video.hw_ctx {
            unsafe {
                av_buffer_unref(ctx);
            }
        }
        Ok(())
    }
}
