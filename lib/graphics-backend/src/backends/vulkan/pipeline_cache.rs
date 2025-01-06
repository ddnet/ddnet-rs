use std::sync::Arc;

use anyhow::anyhow;
use ash::vk;
use base_io::{io::IoFileSys, runtime::IoRuntimeTask};
use hiarc::Hiarc;
use serde::{Deserialize, Serialize};

use crate::{backends::types::BackendWriteFiles, cache::get_backend_cache};

use super::logical_device::LogicalDevice;

const PIPELINE_CACHE: &str = "vulkan/pipeline.cache";

#[derive(Debug, Hiarc)]
pub struct PipelineCacheInner {
    #[hiarc_skip_unsafe]
    pub cache: vk::PipelineCache,

    device: Arc<LogicalDevice>,
}

impl Drop for PipelineCacheInner {
    fn drop(&mut self) {
        unsafe { self.device.device.destroy_pipeline_cache(self.cache, None) };
    }
}

pub const PIPELINE_CACHE_WRAPPER_VERSION: u64 = 1;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineCacheHeaderWrapper {
    version: u64,
    /// from [`vk::PhysicalDeviceProperties`]
    vendor_id: u32,
    /// from [`vk::PhysicalDeviceProperties`]       
    device_id: u32,
    /// from [`vk::PhysicalDeviceProperties`]     
    driver_version: u32,
    /// from [`vk::PhysicalDeviceProperties`]    
    pipeline_cache_uuid: [u8; vk::UUID_SIZE],
    ptr_size: u64, // size of a pointer on this system
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineCacheWrapper {
    header: PipelineCacheHeaderWrapper,

    /// actual pipeline cache data
    data: Vec<u8>,
}

#[derive(Debug, Hiarc)]
pub struct PipelineCache {
    pub(crate) inner: Arc<PipelineCacheInner>,

    write_files: BackendWriteFiles,
}

impl PipelineCache {
    fn cache_header(props: &vk::PhysicalDeviceProperties) -> PipelineCacheHeaderWrapper {
        PipelineCacheHeaderWrapper {
            version: PIPELINE_CACHE_WRAPPER_VERSION,
            vendor_id: props.vendor_id,
            device_id: props.device_id,
            driver_version: props.driver_version,
            pipeline_cache_uuid: props.pipeline_cache_uuid,
            ptr_size: std::mem::size_of::<*const u8>() as u64,
        }
    }

    pub fn new(
        device: Arc<LogicalDevice>,
        previous_cache: Option<&Vec<u8>>,
        write_files: BackendWriteFiles,
    ) -> anyhow::Result<Self> {
        let mut create_info = vk::PipelineCacheCreateInfo::default();
        let previous_cache = previous_cache.map(|cache| {
            let (cache, _) = bincode::serde::decode_from_slice::<PipelineCacheWrapper, _>(
                cache,
                bincode::config::standard(),
            )?;

            let props = &device.phy_device.raw_device_props;
            let header = Self::cache_header(props);
            if cache.header != header {
                return Err(anyhow!("header not compatible"));
            }

            anyhow::Ok(cache)
        });
        if let Some(Ok(data)) = &previous_cache {
            create_info = create_info.initial_data(&data.data);
        }

        let cache = unsafe { device.device.create_pipeline_cache(&create_info, None) };

        let cache = match cache {
            Ok(cache) => cache,
            Err(_) => {
                // continue with an empty cache
                let create_info = vk::PipelineCacheCreateInfo::default();
                unsafe { device.device.create_pipeline_cache(&create_info, None)? }
            }
        };

        Ok(Self {
            inner: Arc::new(PipelineCacheInner { cache, device }),
            write_files,
        })
    }

    pub fn load_previous_cache(io: &IoFileSys) -> IoRuntimeTask<Option<Vec<u8>>> {
        let fs = io.fs.clone();
        io.rt.spawn(async move {
            let cache = get_backend_cache(&fs).await;
            let res = cache.read_named(PIPELINE_CACHE.as_ref()).await.ok();
            Ok(res)
        })
    }
}

impl Drop for PipelineCache {
    fn drop(&mut self) {
        unsafe {
            // fail safe, either it works or it doesn't, no need to handle the error
            if let Ok(cache) = self
                .inner
                .device
                .device
                .get_pipeline_cache_data(self.inner.cache)
            {
                let props = &self.inner.device.phy_device.raw_device_props;
                if let Ok(cache) = bincode::serde::encode_to_vec(
                    PipelineCacheWrapper {
                        header: Self::cache_header(props),
                        data: cache,
                    },
                    bincode::config::standard(),
                ) {
                    self.write_files.lock().insert(PIPELINE_CACHE.into(), cache);
                }
            }
        };
    }
}
