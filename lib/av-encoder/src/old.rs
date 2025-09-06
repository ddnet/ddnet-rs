use std::{fmt::Debug, marker::PhantomData, rc::Rc, sync::Arc};

use av_data::{params::CodecParams, pixel::Formaton, rational::Rational64, timeinfo::TimeInfo};
use av_format::{common::GlobalInfo, stream::Stream};
use graphics_backend::backend::GraphicsBackend;
use graphics_backend_traits::{
    frame_fetcher_plugin::{
        BackendFrameFetcher, BackendPresentedImageData, FetchCanvasError, FetchCanvasIndex,
        OffscreenCanvasId,
    },
    traits::GraphicsBackendInterface,
};
use hiarc::{Hiarc, hiarc_safer_arc_mutex};
use matroska::muxer::MkvMuxer;
use rav1e::{
    Config, Context, EncoderConfig, EncoderStatus,
    prelude::{FrameType, SpeedSettings},
};
pub use tokio::sync::oneshot::{Receiver, Sender, channel};

#[hiarc_safer_arc_mutex]
#[derive(Hiarc)]
pub struct AudioVideoEncoderImpl {
    #[hiarc_skip_unsafe]
    ctx: Context<u8>,
    #[hiarc_skip_unsafe]
    enc_cfg: EncoderConfig,
    cur_frame: usize,
    #[hiarc_skip_unsafe]
    muxer: av_format::muxer::Context<MkvMuxer, Vec<u8>>,
    frame_buffer_id: OffscreenCanvasId,

    file_sender: Option<Sender<Vec<u8>>>,

    _backend_data: PhantomData<BackendPresentedImageData>,
}

impl Debug for AudioVideoEncoderImplImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioVideoEncoderImplImpl")
            .field("ctx", &self.ctx)
            .field("enc_cfg", &self.enc_cfg)
            .field("cur_frame", &self.cur_frame)
            .finish()
    }
}

impl Debug for AudioVideoEncoderImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioVideoEncoderImpl").finish()
    }
}

#[hiarc_safer_arc_mutex]
impl AudioVideoEncoderImpl {
    pub fn new(frame_buffer_id: OffscreenCanvasId, file_sender: Sender<Vec<u8>>) -> Self {
        let enc_cfg = EncoderConfig {
            width: 800,
            height: 600,
            bit_depth: 8,
            ..Default::default()
        };

        let cfg = Config::new().with_encoder_config(enc_cfg.clone());

        let ctx: Context<u8> = cfg.new_context().unwrap();

        let mut muxer = av_format::muxer::Context::new(
            MkvMuxer::webm(),
            av_format::muxer::Writer::new(Vec::new()),
        );
        muxer.configure().unwrap();
        let mut vid_stream = Stream::from_params(
            &CodecParams {
                kind: Some(av_data::params::MediaKind::Video(
                    av_data::params::VideoInfo {
                        width: 800,
                        height: 600,
                        format: Some(Arc::new(Formaton::new(
                            av_data::pixel::ColorModel::Trichromatic(
                                av_data::pixel::TrichromaticEncodingSystem::RGB,
                            ),
                            &[],
                            1,
                            false,
                            true,
                            false,
                        ))),
                    },
                )),
                codec_id: Some("av1".into()),
                extradata: None,
                bit_rate: 12000,
                convergence_window: 0,
                delay: 0,
            },
            Rational64::new(1, 30),
        );
        vid_stream.id = 0;
        muxer
            .set_global_info(GlobalInfo {
                duration: None,
                timebase: None,
                streams: vec![vid_stream],
            })
            .unwrap();
        muxer.write_header().unwrap();

        Self {
            ctx,
            enc_cfg,
            cur_frame: 0,
            muxer,
            frame_buffer_id,
            file_sender: Some(file_sender),
            _backend_data: Default::default(),
        }
    }

    fn handle_ctx_packets(&mut self) {
        loop {
            match self.ctx.receive_packet() {
                Ok(pkt) => {
                    log::info!("Packet {}", pkt.input_frameno);
                    self.cur_frame += 1;
                    let mut write_pkt = av_data::packet::Packet::new();
                    write_pkt.data = pkt.data;
                    write_pkt.pos = Some(pkt.input_frameno as usize);
                    write_pkt.stream_index = 0;
                    write_pkt.t = TimeInfo {
                        pts: None,
                        dts: None,
                        duration: None,
                        timebase: None,
                        user_private: None,
                    };
                    write_pkt.is_key = true;
                    matches!(pkt.frame_type, FrameType::KEY);
                    write_pkt.is_corrupted = false;

                    self.muxer.write_packet(Arc::new(write_pkt)).unwrap();
                }
                Err(e) => match e {
                    EncoderStatus::LimitReached => {
                        log::info!("Limit reached");
                        break;
                    }
                    EncoderStatus::Encoded => {
                        log::info!("  Encoded");
                    }
                    EncoderStatus::NeedMoreData => {
                        log::info!("  Need more data");
                        break;
                    }
                    _ => {
                        panic!("Unable to receive packet {}", self.cur_frame);
                    }
                },
            }
        }
    }
}

#[hiarc_safer_arc_mutex]
impl BackendFrameFetcher for AudioVideoEncoderImpl {
    #[hiarc_trait_is_immutable_self]
    fn next_frame(&mut self, frame_data: BackendPresentedImageData) {
        let ctx = &mut self.ctx;

        let mut f = ctx.new_frame();

        let pixel_buffer = frame_data.dest_data_buffer;

        for p in &mut f.planes {
            let stride = (self.enc_cfg.width + p.cfg.xdec) >> p.cfg.xdec;
            p.copy_from_raw_u8(&pixel_buffer, stride, 4);
        }

        match ctx.send_frame(f.clone()) {
            Ok(_) => {}
            Err(e) => match e {
                EncoderStatus::EnoughData => {
                    log::info!(
                        "Unable to append frame {} to the internal queue",
                        self.cur_frame
                    );
                }
                _ => {
                    panic!("Unable to send frame {}", self.cur_frame);
                }
            },
        }

        self.handle_ctx_packets();
    }

    fn current_fetch_index(&self) -> FetchCanvasIndex {
        FetchCanvasIndex::Offscreen(self.frame_buffer_id)
    }

    fn fetch_err(&self, err: FetchCanvasError) {
        match err {
            FetchCanvasError::CanvasNotFound => {
                // ignore for now
            }
            FetchCanvasError::DriverErr(err) => {
                panic!("err in audio encoding: {err}");
            }
        }
    }
}

#[hiarc_safer_arc_mutex]
impl Drop for AudioVideoEncoderImpl {
    fn drop(&mut self) {
        self.ctx.flush();
        self.handle_ctx_packets();
        self.file_sender
            .take()
            .unwrap()
            .send(self.muxer.writer().as_ref().0.clone())
            .unwrap();
    }
}

pub struct AudioVideoEncoder {
    backend: Rc<GraphicsBackend>,
    encoder: Option<Arc<AudioVideoEncoderImpl>>,
}

impl AudioVideoEncoder {
    pub fn new(
        frame_buffer_id: OffscreenCanvasId,
        file_name: &str,
        backend: &Rc<GraphicsBackend>,
        file_sender: Sender<Vec<u8>>,
    ) -> anyhow::Result<Self> {
        let encoder = Arc::new(AudioVideoEncoderImpl::new(frame_buffer_id, file_sender));

        backend.attach_frame_fetcher("av-encoder".into(), encoder.clone())?;

        Ok(Self {
            backend: backend.clone(),
            encoder: Some(encoder),
        })
    }
}

impl Drop for AudioVideoEncoder {
    fn drop(&mut self) {
        let _ = self.backend.detach_frame_fetcher("av-encoder".into());
    }
}
