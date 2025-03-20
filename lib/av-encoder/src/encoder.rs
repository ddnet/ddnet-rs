use std::{
    collections::HashMap,
    fmt::Debug,
    marker::PhantomData,
    ops::Deref,
    path::Path,
    rc::Rc,
    sync::{atomic::AtomicU64, mpsc, Arc, Mutex},
    thread::ThreadId,
};

use base::join_thread::JoinThread;
use ffmpeg_next::util;
use graphics_backend::backend::GraphicsBackend;
use graphics_backend_traits::{
    frame_fetcher_plugin::{
        BackendFrameFetcher, BackendPresentedImageDataRgba, FetchCanvasError, FetchCanvasIndex,
        OffscreenCanvasId,
    },
    traits::GraphicsBackendInterface,
};
use hiarc::{hiarc_safer_arc_mutex, Hiarc};
use pool::mt_datatypes::PoolUnclearedVec;
use rayon::iter::{ParallelBridge, ParallelIterator};
use sound::backend_types::SoundBackendInterface;
use sound::frame_fetcher_plugin::{
    self, BackendAudioFrame, FetchSoundManagerError, FetchSoundManagerIndex, OffairSoundManagerId,
};
use sound_backend::sound_backend::SoundBackend;
pub use tokio::sync::oneshot::{channel, Receiver, Sender};

use crate::{
    ffmpeg::{
        converter::{AudioConverter, FrameConverter},
        encoder::{AvEncoder, Encoder},
    },
    traits::AudioVideoEncoder,
    types::EncoderSettings,
};

pub enum AvFrame {
    Video(ffmpeg_next::frame::Video),
    Audio(ffmpeg_next::frame::Audio),
}

#[hiarc_safer_arc_mutex]
#[derive(Debug, Hiarc)]
pub struct AudioVideoEncoderImpl {
    video_sender: std::sync::mpsc::Sender<(PoolUnclearedVec<u8>, i64)>,
    cur_video_frame: u64,
    video_frame_buffer_id: OffscreenCanvasId,

    audio_sender: std::sync::mpsc::Sender<(BackendAudioFrame, i64)>,
    cur_audio_frame: u64,
    audio_frame_buffer_id: OffairSoundManagerId,

    video_frames_in_queue: Arc<AtomicU64>,
    max_video_frames_in_queue: u64,
    audio_frames_in_queue: Arc<AtomicU64>,
    max_audio_frames_in_queue: u64,

    _backend_data: PhantomData<BackendPresentedImageDataRgba>,
    _encoder_thread: JoinThread<()>,
    _video_conversion_thread: JoinThread<()>,
    _audio_conversion_thread: JoinThread<()>,
}

#[hiarc_safer_arc_mutex]
impl AudioVideoEncoderImpl {
    pub fn new(
        video_frame_buffer_id: OffscreenCanvasId,
        audio_frame_buffer_id: OffairSoundManagerId,
        file_path: &Path,
        encoder_settings: EncoderSettings,
    ) -> anyhow::Result<Self> {
        let (video_sender, video_receiver) = mpsc::channel::<(PoolUnclearedVec<u8>, i64)>();
        let (audio_sender, audio_receiver) = mpsc::channel::<(BackendAudioFrame, i64)>();
        let (encode_sender, encode_receiver) =
            mpsc::sync_channel::<AvFrame>(encoder_settings.max_threads as usize * 4);

        let video_frames_in_queue: Arc<AtomicU64> = Default::default();
        let max_video_frames_in_queue: u64 = encoder_settings.max_threads;
        let audio_frames_in_queue: Arc<AtomicU64> = Default::default();
        let max_audio_frames_in_queue: u64 = encoder_settings.max_threads;

        let thread_pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(encoder_settings.max_threads as usize)
                .build()?,
        );

        let mut enc = Encoder::new(file_path, &encoder_settings)?;
        let frame_size = enc.audio.encoder.frame_size();

        let video_frames_in_queue_thread = video_frames_in_queue.clone();
        let audio_frames_in_queue_thread = audio_frames_in_queue.clone();
        let encode_sender_thread = encode_sender.clone();
        let encoder_thread = std::thread::Builder::new()
            .name("av-encoder".to_string())
            .spawn(move || {
                let mut video_expected_index = 0;
                let mut video_out_of_order = Vec::<util::frame::Video>::new();

                let mut audio_expected_index = 0;
                let mut audio_out_of_order = Vec::<util::frame::Audio>::new();

                while let Ok(encoded) = encode_receiver.recv() {
                    fn encode<A: Deref<Target = ffmpeg_next::frame::Frame>, E: AvEncoder<A>>(
                        octx: &mut ffmpeg_next::format::context::Output,
                        enc: &mut E,
                        encoded: A,
                        frames_in_queue_thread: &AtomicU64,
                        expected_index: &mut i64,
                        index_scale: i64,
                        out_of_order: &mut Vec<A>,
                    ) {
                        if encoded.pts() == Some(*expected_index) {
                            frames_in_queue_thread
                                .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                            enc.encode_frame(octx, &encoded).unwrap();
                            *expected_index += index_scale;
                            while let Some(ooo) = out_of_order
                                .iter()
                                .position(|e| e.pts() == Some(*expected_index))
                            {
                                frames_in_queue_thread
                                    .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                                enc.encode_frame(octx, &out_of_order.remove(ooo)).unwrap();
                                *expected_index += index_scale;
                            }
                        } else {
                            out_of_order.push(encoded);
                        }
                    }

                    match encoded {
                        AvFrame::Video(encoded) => {
                            encode(
                                &mut enc.octx,
                                &mut enc.video,
                                encoded,
                                &video_frames_in_queue_thread,
                                &mut video_expected_index,
                                1,
                                &mut video_out_of_order,
                            );
                        }
                        AvFrame::Audio(encoded) => {
                            encode(
                                &mut enc.octx,
                                &mut enc.audio,
                                encoded,
                                &audio_frames_in_queue_thread,
                                &mut audio_expected_index,
                                frame_size as i64,
                                &mut audio_out_of_order,
                            );
                        }
                    }
                }
                enc.encode_eof().unwrap();
            })
            .unwrap();
        let video_conversion_thread = std::thread::Builder::new()
            .name("av-convert".to_string())
            .spawn(move || {
                let enc = Mutex::new(HashMap::<
                    ThreadId,
                    Arc<Mutex<sendable::SendOption<FrameConverter>>>,
                >::with_capacity(
                    encoder_settings.max_threads as usize
                ));
                thread_pool.install(|| {
                    video_receiver
                        .into_iter()
                        .par_bridge()
                        .for_each(|(rendered, index)| {
                            let thread_id = std::thread::current().id();

                            let converter = enc
                                .lock()
                                .unwrap()
                                .entry(thread_id)
                                .or_insert_with(|| {
                                    Arc::new(Mutex::new(sendable::SendOption::new(Some(
                                        FrameConverter::new((
                                            encoder_settings.width,
                                            encoder_settings.height,
                                        ))
                                        .unwrap(),
                                    ))))
                                })
                                .clone();
                            let mut converter = converter.lock().unwrap();
                            let converter = converter.as_mut().unwrap();

                            let encode_frame = converter.process(&rendered, index).unwrap();
                            encode_sender_thread
                                .send(AvFrame::Video(encode_frame))
                                .unwrap();
                        });
                });

                thread_pool.broadcast(|_| {
                    let thread_id = std::thread::current().id();
                    enc.lock().unwrap().remove(&thread_id);
                });
            })
            .unwrap();
        let audio_frames_in_queue_thread = audio_frames_in_queue.clone();
        let audio_conversion_thread = std::thread::Builder::new()
            .name("av-convert-audio".to_string())
            .spawn(move || {
                let mut queued_audio = Vec::new();
                let mut cur_index = 0;
                while let Ok((frame, _)) = audio_receiver.recv() {
                    queued_audio.push(frame);

                    if queued_audio.len() == frame_size as usize {
                        let encoded_frame = AudioConverter
                            .process(&queued_audio, cur_index * frame_size as i64)
                            .unwrap();
                        queued_audio.clear();
                        encode_sender.send(AvFrame::Audio(encoded_frame)).unwrap();
                        cur_index += 1;
                        audio_frames_in_queue_thread
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
                if !queued_audio.is_empty() {
                    let encoded_frame = AudioConverter
                        .process(&queued_audio, cur_index * frame_size as i64)
                        .unwrap();
                    encode_sender.send(AvFrame::Audio(encoded_frame)).unwrap();
                }
            })
            .unwrap();

        Ok(Self {
            video_sender,
            cur_video_frame: 0,
            video_frame_buffer_id,

            audio_sender,
            cur_audio_frame: 0,
            audio_frame_buffer_id,

            video_frames_in_queue,
            max_video_frames_in_queue,
            audio_frames_in_queue,
            max_audio_frames_in_queue,

            _backend_data: Default::default(),

            _encoder_thread: JoinThread::new(encoder_thread),
            _video_conversion_thread: JoinThread::new(video_conversion_thread),
            _audio_conversion_thread: JoinThread::new(audio_conversion_thread),
        })
    }

    pub fn overloaded(&self) -> bool {
        self.video_frames_in_queue
            .load(std::sync::atomic::Ordering::Relaxed)
            >= self.max_video_frames_in_queue
            || self
                .audio_frames_in_queue
                .load(std::sync::atomic::Ordering::Relaxed)
                >= self.max_audio_frames_in_queue
    }
}

#[hiarc_safer_arc_mutex]
impl BackendFrameFetcher for AudioVideoEncoderImpl {
    #[hiarc_trait_is_immutable_self]
    fn next_frame(&mut self, frame_data: BackendPresentedImageDataRgba) {
        self.video_sender
            .send((frame_data.dest_data_buffer, self.cur_video_frame as i64))
            .unwrap();
        self.cur_video_frame += 1;
        self.video_frames_in_queue
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn current_fetch_index(&self) -> FetchCanvasIndex {
        FetchCanvasIndex::Offscreen(self.video_frame_buffer_id)
    }

    fn fetch_err(&self, err: FetchCanvasError) {
        match err {
            FetchCanvasError::CanvasNotFound => {
                // ignore for now
            }
            FetchCanvasError::DriverErr(err) => {
                panic!("err in video encoding: {err}");
            }
        }
    }
}

#[hiarc_safer_arc_mutex]
impl frame_fetcher_plugin::BackendFrameFetcher for AudioVideoEncoderImpl {
    #[hiarc_trait_is_immutable_self]
    fn next_frame(&mut self, frame_data: BackendAudioFrame) {
        self.audio_sender
            .send((frame_data, self.cur_audio_frame as i64))
            .unwrap();
        self.cur_audio_frame += 1;
    }

    fn current_fetch_index(&self) -> FetchSoundManagerIndex {
        FetchSoundManagerIndex::Offair(self.audio_frame_buffer_id)
    }

    fn fetch_err(&self, err: FetchSoundManagerError) {
        match err {
            FetchSoundManagerError::SoundManagerNotFound => {
                // ignore for now
            }
            FetchSoundManagerError::DriverErr(err) => {
                panic!("err in audio encoding: {err}");
            }
        }
    }
}

pub struct FfmpegEncoder {
    backend: Rc<GraphicsBackend>,
    sound_backend: Rc<SoundBackend>,
    encoder: Arc<AudioVideoEncoderImpl>,
}

impl AudioVideoEncoder for FfmpegEncoder {
    fn new(
        video_frame_buffer_id: OffscreenCanvasId,
        audio_frame_buffer_id: OffairSoundManagerId,
        file_path: &Path,
        backend: &Rc<GraphicsBackend>,
        sound_backend: &Rc<SoundBackend>,
        encoder_settings: EncoderSettings,
    ) -> anyhow::Result<Self> {
        let encoder = Arc::new(AudioVideoEncoderImpl::new(
            video_frame_buffer_id,
            audio_frame_buffer_id,
            file_path,
            encoder_settings,
        )?);

        backend.attach_frame_fetcher("av-encoder".into(), encoder.clone())?;
        sound_backend.attach_frame_fetcher("av-encoder".into(), encoder.clone())?;

        Ok(Self {
            backend: backend.clone(),
            sound_backend: sound_backend.clone(),
            encoder,
        })
    }

    fn overloaded(&self) -> bool {
        self.encoder.overloaded()
    }
}

impl Drop for FfmpegEncoder {
    fn drop(&mut self) {
        let _ = self.backend.detach_frame_fetcher("av-encoder".into());
        let _ = self.sound_backend.detach_frame_fetcher("av-encoder".into());
    }
}
