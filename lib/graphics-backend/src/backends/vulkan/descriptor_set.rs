use std::sync::Arc;

use ash::vk;
use hiarc::Hiarc;
use parking_lot::RwLock;

use super::{
    buffer::Buffer, descriptor_layout::DescriptorSetLayout, descriptor_pool::DescriptorPool,
    frame_resources::FrameResources, image_view::ImageView, sampler::Sampler,
};

#[derive(Debug, Hiarc)]
pub struct DescriptorSets {
    #[hiarc_skip_unsafe]
    sets: Vec<vk::DescriptorSet>,

    assigned_buffer: RwLock<Option<Arc<Buffer>>>,
    assigned_img_view: RwLock<Option<Arc<ImageView>>>,
    assigned_sampler: RwLock<Option<Arc<Sampler>>>,

    _layout: Arc<DescriptorSetLayout>,
    pub pool: Arc<DescriptorPool>,
}

impl DescriptorSets {
    pub fn new(
        pool: Arc<DescriptorPool>,

        set_count: usize,
        layout: &Arc<DescriptorSetLayout>,
    ) -> anyhow::Result<Arc<Self>> {
        let mut create_info = vk::DescriptorSetAllocateInfo::default();

        let layouts = vec![layout.layout; set_count];
        create_info = create_info.set_layouts(&layouts);

        let pool_g = pool.pool.lock();
        create_info = create_info.descriptor_pool(*pool_g);
        let sets = unsafe { pool.device.device.allocate_descriptor_sets(&create_info) }?;
        drop(pool_g);

        pool.cur_size
            .fetch_add(sets.len() as u64, std::sync::atomic::Ordering::SeqCst);

        Ok(Arc::new(Self {
            sets,
            pool,
            _layout: layout.clone(),
            assigned_buffer: Default::default(),
            assigned_img_view: Default::default(),
            assigned_sampler: Default::default(),
        }))
    }

    pub fn set(self: &Arc<Self>, frame_resources: &mut FrameResources) -> vk::DescriptorSet {
        frame_resources.descriptor_sets.push(self.clone());

        self.sets[0]
    }

    fn assign_buffer_to_sets_impl(
        &self,
        buffer: &Arc<Buffer>,
        offset: vk::DeviceSize,
        range_per_set: vk::DeviceSize,
        ty: vk::DescriptorType,
    ) -> usize {
        let mut buffer_infos: Vec<vk::DescriptorBufferInfo> = Vec::with_capacity(self.sets.len());
        let mut descriptor_writes: Vec<vk::WriteDescriptorSet> =
            Vec::with_capacity(self.sets.len());
        let raw_buffer = buffer.get_buffer(&mut FrameResources::new(None));
        for i in 0..self.sets.len() {
            buffer_infos.push(
                vk::DescriptorBufferInfo::default()
                    .buffer(raw_buffer)
                    .offset(offset + (range_per_set * i as vk::DeviceSize))
                    .range(range_per_set),
            );
        }

        for i in 0..self.sets.len() {
            descriptor_writes.push(
                vk::WriteDescriptorSet::default()
                    .dst_set(self.sets[i])
                    .dst_binding(0)
                    .dst_array_element(0)
                    .descriptor_type(ty)
                    .buffer_info(&buffer_infos[i..i + 1]),
            );
        }

        unsafe {
            self.pool
                .device
                .device
                .update_descriptor_sets(&descriptor_writes, &[]);
        }

        *self.assigned_buffer.write() = Some(buffer.clone());

        self.sets.len()
    }

    pub fn assign_uniform_buffer_to_sets(
        &self,
        buffer: &Arc<Buffer>,
        offset: vk::DeviceSize,
        range_per_set: vk::DeviceSize,
    ) -> usize {
        self.assign_buffer_to_sets_impl(
            buffer,
            offset,
            range_per_set,
            vk::DescriptorType::UNIFORM_BUFFER,
        )
    }

    pub fn assign_shader_storage_buffer_to_sets(
        &self,
        buffer: &Arc<Buffer>,
        offset: vk::DeviceSize,
        range_per_set: vk::DeviceSize,
    ) -> usize {
        self.assign_buffer_to_sets_impl(
            buffer,
            offset,
            range_per_set,
            vk::DescriptorType::STORAGE_BUFFER,
        )
    }

    pub fn assign_texture_and_sampler_combined(
        &self,
        image_view: &Arc<ImageView>,
        sampler: &Arc<Sampler>,
    ) {
        let mut image_info = vk::DescriptorImageInfo::default();
        image_info.image_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
        image_info.image_view = image_view.view(&mut FrameResources::new(None));
        image_info.sampler = sampler.sampler(&mut FrameResources::new(None));
        let image_infos = [image_info];

        let mut descriptor_writes = vk::WriteDescriptorSet::default();
        descriptor_writes.dst_set = self.sets[0];
        descriptor_writes.dst_binding = 0;
        descriptor_writes.dst_array_element = 0;
        descriptor_writes.descriptor_type = vk::DescriptorType::COMBINED_IMAGE_SAMPLER;
        descriptor_writes = descriptor_writes.image_info(&image_infos);

        unsafe {
            self.pool
                .device
                .device
                .update_descriptor_sets(&[descriptor_writes], &[]);
        }

        *self.assigned_img_view.write() = Some(image_view.clone());
        *self.assigned_sampler.write() = Some(sampler.clone());
    }

    pub fn assign_texture(&self, image_view: &Arc<ImageView>) {
        let mut image_info = vk::DescriptorImageInfo::default();
        image_info.image_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
        image_info.image_view = image_view.view(&mut FrameResources::new(None));
        let image_infos = [image_info];

        let mut descriptor_writes = vk::WriteDescriptorSet::default();
        descriptor_writes.dst_set = self.sets[0];
        descriptor_writes.dst_binding = 0;
        descriptor_writes.dst_array_element = 0;
        descriptor_writes.descriptor_type = vk::DescriptorType::SAMPLED_IMAGE;
        descriptor_writes = descriptor_writes.image_info(&image_infos);

        unsafe {
            self.pool
                .device
                .device
                .update_descriptor_sets(&[descriptor_writes], &[]);
        }

        *self.assigned_img_view.write() = Some(image_view.clone());
    }

    pub fn assign_sampler(&self, sampler: &Arc<Sampler>) {
        let mut image_info = vk::DescriptorImageInfo::default();
        image_info.image_layout = vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL;
        image_info.sampler = sampler.sampler(&mut FrameResources::new(None));
        let image_infos = [image_info];

        let mut descriptor_writes = vk::WriteDescriptorSet::default();
        descriptor_writes.dst_set = self.sets[0];
        descriptor_writes.dst_binding = 0;
        descriptor_writes.dst_array_element = 0;
        descriptor_writes.descriptor_type = vk::DescriptorType::SAMPLER;
        descriptor_writes.descriptor_count = 1;
        descriptor_writes = descriptor_writes.image_info(&image_infos);

        unsafe {
            self.pool
                .device
                .device
                .update_descriptor_sets(&[descriptor_writes], &[]);
        }

        *self.assigned_sampler.write() = Some(sampler.clone());
    }
}

impl Drop for DescriptorSets {
    fn drop(&mut self) {
        let pool = *self.pool.pool.lock();

        unsafe {
            self.pool
                .device
                .device
                .free_descriptor_sets(pool, &self.sets)
                .unwrap();
        }
        self.pool
            .cur_size
            .fetch_sub(self.sets.len() as u64, std::sync::atomic::Ordering::SeqCst);
    }
}

#[derive(Debug, Hiarc)]
pub struct DescriptorSet {
    sets: Arc<DescriptorSets>,
    set_index: usize,
}

impl DescriptorSet {
    pub fn set(&self, frame_resources: &mut FrameResources) -> vk::DescriptorSet {
        frame_resources.descriptor_sets.push(self.sets.clone());

        self.sets.sets[self.set_index]
    }
}

pub fn split_descriptor_sets(sets: &Arc<DescriptorSets>) -> Vec<Arc<DescriptorSet>> {
    sets.sets
        .iter()
        .enumerate()
        .map(|(index, _)| {
            Arc::new(DescriptorSet {
                sets: sets.clone(),
                set_index: index,
            })
        })
        .collect()
}
