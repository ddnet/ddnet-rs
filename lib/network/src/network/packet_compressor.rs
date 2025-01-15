use std::io::{Read, Write};

use anyhow::anyhow;
use async_trait::async_trait;
use header::CompressHeader;
use pool::mt_pool::Pool;
use types::DecompressionByteLimit;

#[cfg(feature = "brotli")]
pub mod brotli;

pub mod header;
pub mod types;

use super::{connection::NetworkConnectionId, plugins::NetworkPluginPacket};

/// A network plugin, that can compress packets using zstd.
/// Good in speed, size depends on dictionary.
#[derive(Debug)]
pub struct ZstdNetworkPacketCompressor {
    helper_pool: Pool<Vec<u8>>,

    send_dict: Option<Vec<u8>>,
    recv_dict: Option<Vec<u8>>,

    limit: DecompressionByteLimit,
}

impl Default for ZstdNetworkPacketCompressor {
    fn default() -> Self {
        Self::new()
    }
}

impl ZstdNetworkPacketCompressor {
    pub fn new() -> Self {
        Self {
            helper_pool: Pool::builder()
                .with_limit(64.try_into().unwrap())
                .with_capacity(64)
                .build(),
            send_dict: None,
            recv_dict: None,

            limit: Default::default(),
        }
    }

    pub fn new_with_dict(send_dict: Vec<u8>, recv_dict: Vec<u8>) -> Self {
        Self {
            helper_pool: Pool::builder()
                .with_limit(64.try_into().unwrap())
                .with_capacity(64)
                .build(),
            send_dict: Some(send_dict),
            recv_dict: Some(recv_dict),

            limit: Default::default(),
        }
    }

    pub fn with_limit(mut self, limit: DecompressionByteLimit) -> Self {
        self.limit = limit;
        self
    }
}

#[async_trait]
impl NetworkPluginPacket for ZstdNetworkPacketCompressor {
    async fn prepare_write(
        &self,
        _id: &NetworkConnectionId,
        buffer: &mut Vec<u8>,
    ) -> anyhow::Result<()> {
        let mut helper = self.helper_pool.new();
        let helper: &mut Vec<_> = helper.as_mut();

        const COMPRESSION_LEVEL: i32 = 0;
        let mut encoder = if let Some(dict) = &self.send_dict {
            zstd::Encoder::with_dictionary(&mut *helper, COMPRESSION_LEVEL, dict)?
        } else {
            zstd::Encoder::new(&mut *helper, COMPRESSION_LEVEL)?
        };
        encoder.write_all(buffer)?;
        encoder.finish()?;

        let header = CompressHeader {
            size: helper.len().min(buffer.len()),
            is_compressed: helper.len() < buffer.len(),
        };

        let mut size_helper = self.helper_pool.new();
        let size_helper: &mut Vec<_> = size_helper.as_mut();
        bincode::serde::encode_into_std_write(&header, size_helper, bincode::config::standard())?;
        if header.is_compressed {
            size_helper.append(helper);
        } else {
            size_helper.append(buffer);
        }

        std::mem::swap(buffer, size_helper);
        Ok(())
    }
    async fn prepare_read(
        &self,
        _id: &NetworkConnectionId,
        buffer: &mut Vec<u8>,
    ) -> anyhow::Result<()> {
        let (header, read_size) = match self.limit {
            DecompressionByteLimit::FourMegaBytes => {
                bincode::serde::decode_from_slice::<CompressHeader, _>(
                    buffer,
                    // use a high limit, since the packet size is already limited by the stream window
                    bincode::config::standard().with_limit::<{ 1024 * 1024 * 4 }>(),
                )?
            }
            DecompressionByteLimit::OneGigaByte => {
                bincode::serde::decode_from_slice::<CompressHeader, _>(
                    buffer,
                    // use a high limit, since the packet size is already limited by the stream window
                    bincode::config::standard().with_limit::<{ 1024 * 1024 * 1024 }>(),
                )?
            }
        };

        if header.is_compressed {
            let mut helper = self.helper_pool.new();
            let helper: &mut Vec<_> = helper.as_mut();

            let decode_buffer_slice = buffer
                .get(read_size..read_size + header.size)
                .ok_or_else(|| anyhow!("header slice out of bounds"))?;
            let decode_buffer = std::io::BufReader::new(decode_buffer_slice);

            let mut decoder = if let Some(dict) = &self.recv_dict {
                zstd::Decoder::with_dictionary(decode_buffer, dict)?
            } else {
                zstd::Decoder::new(decode_buffer_slice)?
            };
            decoder.read_to_end(helper)?;
            decoder.finish();

            std::mem::swap(buffer, helper);
        } else {
            buffer.splice(0..read_size, []);
        }

        Ok(())
    }
}

/// A network plugin, that can compress packets.
pub type DefaultNetworkPacketCompressor = ZstdNetworkPacketCompressor;
