use std::{
    ffi::{CStr, CString},
    ops::Deref,
    sync::{Arc, atomic::AtomicU64},
};

use anyhow::anyhow;
use ash::vk;
use config::config::AtomicGfxDebugModes;
use hiarc::Hiarc;

use super::phy_device::PhyDevice;

#[derive(Hiarc)]
pub struct DeviceWrapper(#[hiarc_skip_unsafe] ash::Device);

impl Deref for DeviceWrapper {
    type Target = ash::Device;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for DeviceWrapper {
    fn drop(&mut self) {
        unsafe {
            self.0.destroy_device(None);
        }
    }
}

#[derive(Hiarc)]
pub struct LogicalDevice {
    pub texture_memory_usage: Arc<AtomicU64>,
    pub buffer_memory_usage: Arc<AtomicU64>,
    pub stream_memory_usage: Arc<AtomicU64>,
    pub staging_memory_usage: Arc<AtomicU64>,

    pub is_headless: bool,

    #[hiarc_skip_unsafe]
    pub dbg: Arc<AtomicGfxDebugModes>,
    // has to outlive the memory allocator
    pub device: DeviceWrapper,
    // has to outlive the logical device
    pub phy_device: Arc<PhyDevice>,
}

impl std::fmt::Debug for LogicalDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogicalDevice")
            .field("phy_device", &self.phy_device)
            .finish()
    }
}

impl LogicalDevice {
    pub fn new(
        phy_gpu: Arc<PhyDevice>,
        graphics_queue_index: u32,
        instance: &ash::Instance,
        is_headless: bool,

        dbg: Arc<AtomicGfxDebugModes>,

        texture_memory_usage: Arc<AtomicU64>,
        buffer_memory_usage: Arc<AtomicU64>,
        stream_memory_usage: Arc<AtomicU64>,
        staging_memory_usage: Arc<AtomicU64>,
    ) -> anyhow::Result<Arc<Self>> {
        let device =
            Self::create_logical_device(&phy_gpu, graphics_queue_index, instance, is_headless)?;

        Ok(Arc::new(Self {
            device: DeviceWrapper(device.clone()),
            phy_device: phy_gpu,

            is_headless,

            texture_memory_usage,
            buffer_memory_usage,
            stream_memory_usage,
            staging_memory_usage,

            dbg,
        }))
    }

    fn create_logical_device(
        phy_gpu: &Arc<PhyDevice>,
        graphics_queue_index: u32,
        instance: &ash::Instance,
        is_headless: bool,
    ) -> anyhow::Result<ash::Device> {
        let dev_prop_list =
            unsafe { instance.enumerate_device_extension_properties(phy_gpu.cur_device) }?;

        let mut dev_prop_cnames_helper = Vec::<CString>::new();
        let our_dev_ext = Self::our_device_extensions(is_headless);

        for cur_ext_prop in &dev_prop_list {
            let ext_name = unsafe {
                CStr::from_ptr(cur_ext_prop.extension_name.as_ptr())
                    .to_str()
                    .unwrap()
                    .to_string()
            };
            let it = our_dev_ext.get(&ext_name);
            if let Some(str) = it {
                dev_prop_cnames_helper
                    .push(unsafe { CString::from_vec_unchecked(str.as_bytes().to_vec()) });
            }
        }

        let queue_prio = [1.0];
        let vk_queue_create_info = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(graphics_queue_index)
            .queue_priorities(&queue_prio)];

        let mut timeline_semaphore_features =
            vk::PhysicalDeviceTimelineSemaphoreFeatures::default().timeline_semaphore(true);

        let mut vk_create_info =
            vk::DeviceCreateInfo::default().queue_create_infos(&vk_queue_create_info);

        if is_headless {
            vk_create_info = vk_create_info.push_next(&mut timeline_semaphore_features);
        }
        let res = unsafe {
            instance.create_device(
                phy_gpu.cur_device,
                &vk_create_info.enabled_extension_names(
                    &dev_prop_cnames_helper
                        .iter()
                        .map(|s| s.as_ptr() as _)
                        .collect::<Vec<_>>(),
                ),
                None,
            )
        }
        .map_err(|err| anyhow!("creating logical device failed: {err}"))?;

        drop(dev_prop_cnames_helper);

        Ok(res)
    }

    fn our_device_extensions(is_headless: bool) -> std::collections::BTreeSet<String> {
        let mut our_ext: std::collections::BTreeSet<String> = Default::default();
        if is_headless {
            our_ext.insert(
                vk::KHR_TIMELINE_SEMAPHORE_NAME
                    .to_str()
                    .unwrap()
                    .to_string(),
            );
        } else {
            our_ext.insert(vk::KHR_SWAPCHAIN_NAME.to_str().unwrap().to_string());
        }
        our_ext
    }

    #[must_use]
    pub fn final_layout(&self) -> vk::ImageLayout {
        if self.is_headless {
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        } else {
            vk::ImageLayout::PRESENT_SRC_KHR
        }
    }
}
