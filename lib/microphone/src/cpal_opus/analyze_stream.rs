use std::{
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use base::join_thread::JoinThread;
use sound::stream::{DecodeError, StreamDecoder};

use crate::stream::MicrophoneStream;

use super::sound_stream::{OPUS_10_MS, OPUS_SAMPLE_RATE, SoundStream};

pub struct AnalyzeStream {
    _inner: SoundStream,

    pub cur_loudest: Arc<RwLock<f32>>,

    // thread last
    _analyze_thread: JoinThread<()>,
}

impl AnalyzeStream {
    pub fn new(microphone: MicrophoneStream) -> Self {
        let inner = SoundStream::new(microphone, Default::default());
        let stream = inner.stream();

        let cur_loudest: Arc<RwLock<f32>> = Default::default();

        let cur_loudest_thread = cur_loudest.clone();

        let analyze_thread = std::thread::spawn(move || {
            let start_time = Instant::now();
            let mut last_sleep_consistent_time = Duration::ZERO;

            while let Ok(samples) = match stream.decode() {
                Ok(samples) => Ok(samples),
                Err(DecodeError::MustGenerateEmpty(_)) => Ok(Vec::new()),
                Err(DecodeError::Err(err)) => Err(err),
            } {
                if !samples.is_empty() {
                    // find loudest noise
                    let loudest = samples
                        .iter()
                        .max_by(|s1, s2| {
                            let s1 = s1.left.abs().max(s1.right.abs());
                            let s2 = s2.left.abs().max(s2.right.abs());
                            s1.total_cmp(&s2)
                        })
                        .map(|loudest| loudest.left.abs().max(loudest.right.abs()))
                        .unwrap_or_default();

                    *cur_loudest_thread.write().unwrap() = loudest;
                }
                let cur_time = start_time.elapsed();
                let time_until_sample_nanos = (Duration::from_secs(1).as_nanos() as u64
                    / OPUS_SAMPLE_RATE as u64)
                    * OPUS_10_MS as u64;

                let sleep_time_nanos = time_until_sample_nanos as i64
                    - (cur_time.as_nanos() as i64 - last_sleep_consistent_time.as_nanos() as i64);
                if sleep_time_nanos > 0 {
                    std::thread::sleep(Duration::from_nanos(sleep_time_nanos as u64));
                }

                last_sleep_consistent_time = Duration::from_nanos(
                    (cur_time.as_nanos() as i64 + sleep_time_nanos.clamp(-16666666, 16666666))
                        as u64,
                );
            }
        });

        Self {
            _inner: inner,
            _analyze_thread: JoinThread::new(analyze_thread),
            cur_loudest,
        }
    }
}
