use std::sync::Arc;

use ash::vk;
use hiarc::Hiarc;

use crate::backends::vulkan::frame_resources::FrameResources;

use super::logical_device::LogicalDevice;

#[derive(Debug, Hiarc)]
pub struct Semaphore {
    #[hiarc_skip_unsafe]
    semaphore: vk::Semaphore,
    pub is_timeline: bool,

    device: Arc<LogicalDevice>,
}

impl Semaphore {
    pub fn new(device: Arc<LogicalDevice>, is_timeline: bool) -> anyhow::Result<Arc<Self>> {
        let mut extra_info =
            vk::SemaphoreTypeCreateInfo::default().semaphore_type(vk::SemaphoreType::TIMELINE_KHR);

        let mut create_semaphore_info_builder = vk::SemaphoreCreateInfo::default();
        if is_timeline {
            create_semaphore_info_builder =
                create_semaphore_info_builder.push_next(&mut extra_info);
        }

        let semaphore = unsafe {
            device
                .device
                .create_semaphore(&create_semaphore_info_builder, None)
        }?;

        Ok(Arc::new(Self {
            semaphore,
            is_timeline,
            device,
        }))
    }

    pub fn semaphore(self: &Arc<Self>, resources: &mut FrameResources) -> vk::Semaphore {
        resources.semaphores.push(self.clone());
        self.semaphore
    }
}

impl Drop for Semaphore {
    fn drop(&mut self) {
        unsafe {
            self.device.device.destroy_semaphore(self.semaphore, None);
        }
    }
}
