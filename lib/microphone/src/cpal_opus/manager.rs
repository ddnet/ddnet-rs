use super::noise_gate::NoiseGateAndBooster;
use super::sound_stream::{OPUS_10_MS, OPUS_SAMPLE_RATE};
use crate::stream::MicrophoneStream;
use crate::stream_sample::StreamSample;
use crate::traits::MicrophoneStreamRaii;
use crate::types::{MicrophoneDevices, MicrophoneHosts, MicrophoneNoiseFilterSettings};
use anyhow::anyhow;
use base::join_thread::JoinThread;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam::channel::{Sender, bounded};
use df::tract::DfParams;
use rubato::{Resampler, SincInterpolationParameters, SincInterpolationType, WindowFunction};

pub struct MicrophoneStreamInner {
    // keep stream alive RAII
    _stream: cpal::Stream,
    // keep encoder alive RAII
    _encoder_thread: JoinThread<()>,
}

impl MicrophoneStreamRaii for MicrophoneStreamInner {}

#[derive(Debug, Default)]
pub struct MicrophoneManager {}

impl crate::traits::Microphone for MicrophoneManager {
    fn hosts(&self) -> MicrophoneHosts {
        MicrophoneHosts {
            hosts: cpal::available_hosts()
                .iter()
                .map(|host| host.name().to_string())
                .collect(),
            default: cpal::default_host().id().name().to_string(),
        }
    }

    fn devices(&self, host: &str) -> anyhow::Result<MicrophoneDevices> {
        let host = Self::host_from_str(host)?;
        Ok(MicrophoneDevices {
            devices: host
                .devices()?
                .map(|device| anyhow::Ok(device.name()?))
                .collect::<anyhow::Result<Vec<_>>>()?,
            default: host.default_input_device().map(|d| d.name()).transpose()?,
        })
    }

    fn stream_opus(
        &self,
        host: &str,
        device: &str,
        settings: MicrophoneNoiseFilterSettings,
    ) -> anyhow::Result<MicrophoneStream> {
        let host = Self::host_from_str(host)?;
        let device = Self::device_from_str(&host, device)?;

        log::info!("Loaded microphone device: {}", device.name()?);

        let config = device.default_input_config()?;
        log::info!("Default input config: {:?}", config);

        let (sender, receiver) = bounded(4096);
        let (encoder_thread_sender, encoder_thread_receiver) = bounded::<Vec<f32>>(4096);

        let err_fn = move |err| {
            log::error!("an error occurred on stream: {}", err);
        };

        let mut writer_helper: Vec<u8> = Vec::new();
        let mut pending_helper: Vec<i16> = Vec::new();
        let mut pending_unprocessed_chunks: Vec<f32> = Vec::new();
        let mut helper_chunks: Vec<f32> = Vec::new();
        let mut helper_stream_out: Vec<f32> = Vec::new();
        let mut helper_nf_out: Vec<f32> = Vec::new();
        let mut pending_nf_chunks: Vec<f32> = Vec::new();
        let mut helper_nf_chunks: Vec<f32> = Vec::new();
        let mut helper_write_chunks: Vec<Vec<f32>> = Vec::new();

        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Cubic,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };

        const CHUNK_SIZE_RESAMPLER: usize = 480;
        const NF_CHUNK_SIZE: usize = OPUS_10_MS;

        let input_sample_rate = config.sample_rate().0;
        let mut resampler = rubato::SincFixedIn::<f32>::new(
            OPUS_SAMPLE_RATE as f64 / input_sample_rate as f64,
            2.0,
            params,
            CHUNK_SIZE_RESAMPLER,
            1,
        )?;

        let encoder_thread = std::thread::spawn(move || {
            let mut enc = opus::Encoder::new(
                OPUS_SAMPLE_RATE as u32,
                opus::Channels::Mono,
                opus::Application::Voip,
            )
            .unwrap();
            enc.set_inband_fec(true).unwrap();

            let mut noise_gate = NoiseGateAndBooster::new(
                settings.noise_gate.open_threshold as f32,
                settings.noise_gate.close_threshold as f32,
                48000.0,
                1,
                150.0,
                25.0,
                150.0,
                settings.boost as f32,
            );

            let mut df = if let Some(nf) = settings.nf {
                let df = DfParams::default();
                let mut rp = df::tract::RuntimeParams::default_with_ch(1)
                    .with_post_filter(0.02)
                    .with_atten_lim(nf.attenuation as f32);
                rp.min_db_thresh = nf.processing_threshold as f32;
                Some(df::tract::DfTract::new(df, &rp).unwrap())
            } else {
                None
            };

            let (sender_opus, receiver_opus) = bounded::<Vec<f32>>(4096);
            let _opus_thread = std::thread::spawn(move || {
                while let Ok(chunk) = receiver_opus.recv() {
                    Self::write_data(
                        Self::encode_data::<f32>(
                            chunk,
                            &|v| ((*v as f64) * i16::MAX as f64).round() as i16,
                            &mut enc,
                            &mut writer_helper,
                            &mut pending_helper,
                        ),
                        &sender,
                    );
                }
            });

            // drop initial events, initialization of all noise filters etc
            // can take a bit.
            while encoder_thread_receiver.try_recv().is_ok() {}

            while let Ok(data) = encoder_thread_receiver.recv() {
                pending_unprocessed_chunks.extend(data);

                helper_chunks.clear();
                std::mem::swap(&mut pending_unprocessed_chunks, &mut helper_chunks);
                let data_chunks = helper_chunks.chunks_exact(CHUNK_SIZE_RESAMPLER);
                pending_unprocessed_chunks.extend(data_chunks.remainder());

                data_chunks.for_each(|data| {
                    helper_stream_out.clear();

                    let channels = [data];

                    helper_stream_out.resize(channels[0].len() * 2, 0.0);

                    let (_, s1) = resampler
                        .process_into_buffer(&channels, &mut [&mut helper_stream_out], None)
                        .unwrap();

                    helper_nf_chunks.clear();
                    pending_nf_chunks.extend(&helper_stream_out[0..s1]);
                    std::mem::swap(&mut helper_nf_chunks, &mut pending_nf_chunks);

                    let nf_chunks = helper_nf_chunks.chunks_exact(NF_CHUNK_SIZE);
                    pending_nf_chunks.extend(nf_chunks.remainder());

                    helper_write_chunks.resize_with(nf_chunks.len(), Default::default);
                    nf_chunks.zip(helper_write_chunks.iter_mut()).for_each(
                        |(nf_chunk, write_chunk)| {
                            write_chunk.resize(NF_CHUNK_SIZE, 0.0);
                            noise_gate.process_frame(nf_chunk, write_chunk);

                            helper_nf_out.resize(NF_CHUNK_SIZE, 0.0);

                            let nf_in = ndarray::ArrayView2::from_shape(
                                (1, write_chunk.len()),
                                write_chunk,
                            )
                            .unwrap();
                            let nf_out = ndarray::ArrayViewMut2::from_shape(
                                (1, helper_nf_out.len()),
                                &mut helper_nf_out,
                            )
                            .unwrap();

                            if let Some(df) = &mut df {
                                if let Err(err) = df.process(nf_in, nf_out) {
                                    log::info!("Err from noise filter: {err}");
                                }
                            } else {
                                helper_nf_out.copy_from_slice(nf_chunk);
                            }

                            sender_opus.send(helper_nf_out.clone()).unwrap();
                        },
                    );
                });
            }
        });

        fn transcoder<'a, T: Copy>(
            data: &'a [T],
            channel_count: u16,
            transcode: impl Fn(T) -> f32 + 'a,
        ) -> Box<dyn Iterator<Item = f32> + 'a> {
            if channel_count >= 2 {
                Box::new(
                    data.chunks_exact(channel_count as usize)
                        .map(|chunk| chunk[0])
                        .map(transcode),
                )
            } else {
                Box::new(data.iter().copied().map(transcode))
            }
        }

        let channel_count = config.channels();
        let stream = match config.sample_format() {
            cpal::SampleFormat::I8 => device.build_input_stream(
                &config.clone().into(),
                move |data: &[u8], _: &_| {
                    // ignore if sending fails, not problem of this caller
                    let _ = encoder_thread_sender.try_send(
                        transcoder(data, channel_count, |v| v as f32 / u8::MAX as f32).collect(),
                    );
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I16 => device.build_input_stream(
                &config.clone().into(),
                move |data: &[i16], _: &_| {
                    // ignore if sending fails, not problem of this caller
                    let _ = encoder_thread_sender.try_send(
                        transcoder(data, channel_count, |v| (v as f64 / i16::MAX as f64) as f32)
                            .collect(),
                    );
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I32 => device.build_input_stream(
                &config.clone().into(),
                move |data: &[i32], _: &_| {
                    // ignore if sending fails, not problem of this caller
                    let _ = encoder_thread_sender.try_send(
                        transcoder(data, channel_count, |v| (v as f64 / i32::MAX as f64) as f32)
                            .collect(),
                    );
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.clone().into(),
                move |data: &[f32], _: &_| {
                    // ignore if sending fails, not problem of this caller
                    let _ = encoder_thread_sender
                        .try_send(transcoder(data, channel_count, |v| v).collect());
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::F64 => device.build_input_stream(
                &config.clone().into(),
                move |data: &[f64], _: &_| {
                    // ignore if sending fails, not problem of this caller
                    let _ = encoder_thread_sender
                        .try_send(transcoder(data, channel_count, |v| v as f32).collect());
                },
                err_fn,
                None,
            )?,
            sample_format => {
                return Err(anyhow::Error::msg(format!(
                    "Unsupported sample format '{sample_format}'"
                )));
            }
        };

        stream.play()?;

        Ok(MicrophoneStream {
            opus_receiver: receiver,
            inner: Box::new(MicrophoneStreamInner {
                _stream: stream,
                _encoder_thread: JoinThread::new(encoder_thread),
            }),
        })
    }
}

impl MicrophoneManager {
    fn host_from_str(host: &str) -> anyhow::Result<cpal::Host> {
        let Some(host_id) = cpal::available_hosts()
            .into_iter()
            .find(|host_id| host_id.name() == host)
        else {
            return Err(anyhow!("Selected host not found"));
        };
        Ok(cpal::host_from_id(host_id)?)
    }

    fn device_from_str(host: &cpal::Host, device: &str) -> anyhow::Result<cpal::Device> {
        let Some(device) = host
            .devices()?
            .find(|d| d.name().is_ok_and(|name| name == device))
        else {
            return Err(anyhow!("Selected host not found"));
        };
        Ok(device)
    }

    fn encode_data<T>(
        chunk: impl AsRef<[T]>,
        transcode: &impl Fn(&T) -> i16,
        encoder: &mut opus::Encoder,
        writer_helper: &mut Vec<u8>,
        helper_input: &mut Vec<i16>,
    ) -> StreamSample {
        helper_input.clear();
        helper_input.extend(chunk.as_ref().iter().map(transcode));
        let chunk = helper_input.as_slice();
        {
            writer_helper.resize(std::mem::size_of_val(chunk), 0);
            let data = encoder.encode(chunk, writer_helper);
            if let Err(err) = &data {
                log::error!(
                    "failed to encode data: {err}, chunks of size {}",
                    chunk.len()
                );
            }
            let size = data.ok().unwrap();
            StreamSample {
                data: writer_helper[0..size].to_vec(),
            }
        }
    }

    fn write_data(input: StreamSample, sender: &Sender<StreamSample>) {
        if let Err(err) = sender.send(input) {
            log::info!("stream sending ended: {err}");
        }
    }
}
