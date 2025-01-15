use anyhow::anyhow;
use async_trait::async_trait;
use pool::mt_pool::Pool;

use crate::network::{connection::NetworkConnectionId, plugins::NetworkPluginPacket};

use super::header::CompressHeader;

/// A network plugin, that can compress packets using brotli.
/// Good in size, bad in speed
#[derive(Debug)]
pub struct BrotliNetworkPacketCompressor {
    helper_pool: Pool<Vec<u8>>,

    limit: DecompressionByteLimit,
}

impl Default for BrotliNetworkPacketCompressor {
    fn default() -> Self {
        Self::new()
    }
}

impl BrotliNetworkPacketCompressor {
    pub fn new() -> Self {
        Self {
            helper_pool: Pool::builder()
                .with_limit(64.try_into().unwrap())
                .with_capacity(64)
                .build(),
        }
    }

    pub fn with_limit(mut self, limit: DecompressionByteLimit) -> Self {
        self.limit = limit;
        self
    }
}

#[async_trait]
impl NetworkPluginPacket for BrotliNetworkPacketCompressor {
    async fn prepare_write(
        &self,
        _id: &NetworkConnectionId,
        buffer: &mut Vec<u8>,
    ) -> anyhow::Result<()> {
        let mut helper = self.helper_pool.new();
        let helper: &mut Vec<_> = helper.as_mut();

        brotli::CompressorWriter::new(&mut *helper, 4096, 8, 22).write_all(buffer)?;

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

            brotli::Decompressor::new(
                buffer
                    .get(read_size..read_size + header.size)
                    .ok_or_else(|| anyhow!("header slice out of bounds"))?,
                4096,
            )
            .read_to_end(helper)?;

            std::mem::swap(buffer, helper);
        } else {
            buffer.splice(0..read_size, []);
        }

        Ok(())
    }
}
