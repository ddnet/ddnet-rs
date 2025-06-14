use std::sync::Arc;

use ash::vk;

use super::{
    buffer::Buffer, frame_resources::FrameResources, image::Image, logical_device::LogicalDevice,
};

pub fn image_barrier(
    frame_resources: &mut FrameResources,
    device: &LogicalDevice,
    command_buffer: vk::CommandBuffer,
    image: &Arc<Image>,
    mip_map_base: usize,
    mip_map_count: usize,
    layer_base: usize,
    layer_count: usize,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
) -> anyhow::Result<()> {
    let mut barrier = vk::ImageMemoryBarrier::default();
    barrier.old_layout = old_layout;
    barrier.new_layout = new_layout;
    barrier.src_queue_family_index = vk::QUEUE_FAMILY_IGNORED;
    barrier.dst_queue_family_index = vk::QUEUE_FAMILY_IGNORED;
    barrier.image = image.img(frame_resources);
    barrier.subresource_range.aspect_mask = vk::ImageAspectFlags::COLOR;
    barrier.subresource_range.base_mip_level = mip_map_base as u32;
    barrier.subresource_range.level_count = mip_map_count as u32;
    barrier.subresource_range.base_array_layer = layer_base as u32;
    barrier.subresource_range.layer_count = layer_count as u32;

    let source_stage;
    let destination_stage;

    let mut needs_dependency = false;

    if old_layout == vk::ImageLayout::UNDEFINED
        && new_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
    {
        barrier.src_access_mask = vk::AccessFlags::empty();
        barrier.dst_access_mask = vk::AccessFlags::TRANSFER_WRITE;

        source_stage = vk::PipelineStageFlags::TOP_OF_PIPE;
        destination_stage = vk::PipelineStageFlags::TRANSFER;
    } else if old_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
        && new_layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
    {
        barrier.src_access_mask = vk::AccessFlags::TRANSFER_WRITE;
        barrier.dst_access_mask = vk::AccessFlags::SHADER_READ;

        source_stage = vk::PipelineStageFlags::TRANSFER;
        destination_stage = vk::PipelineStageFlags::FRAGMENT_SHADER;
    } else if old_layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        && new_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
    {
        barrier.src_access_mask = vk::AccessFlags::SHADER_READ;
        barrier.dst_access_mask = vk::AccessFlags::TRANSFER_WRITE;

        source_stage = vk::PipelineStageFlags::FRAGMENT_SHADER;
        destination_stage = vk::PipelineStageFlags::TRANSFER;
    } else if old_layout == vk::ImageLayout::TRANSFER_SRC_OPTIMAL
        && new_layout == device.final_layout()
    {
        barrier.src_access_mask = vk::AccessFlags::TRANSFER_READ;
        barrier.dst_access_mask = vk::AccessFlags::MEMORY_READ;

        source_stage = vk::PipelineStageFlags::TRANSFER;
        destination_stage = vk::PipelineStageFlags::BOTTOM_OF_PIPE;
    } else if old_layout == device.final_layout()
        && new_layout == vk::ImageLayout::TRANSFER_SRC_OPTIMAL
    {
        barrier.src_access_mask = vk::AccessFlags::MEMORY_READ;
        barrier.dst_access_mask = vk::AccessFlags::TRANSFER_READ;

        source_stage = vk::PipelineStageFlags::BOTTOM_OF_PIPE;
        destination_stage = vk::PipelineStageFlags::TRANSFER;
    } else if old_layout == vk::ImageLayout::TRANSFER_SRC_OPTIMAL
        && new_layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
    {
        barrier.src_access_mask = vk::AccessFlags::TRANSFER_READ;
        barrier.dst_access_mask = vk::AccessFlags::MEMORY_READ;

        source_stage = vk::PipelineStageFlags::TRANSFER;
        destination_stage = vk::PipelineStageFlags::BOTTOM_OF_PIPE;
    } else if old_layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        && new_layout == vk::ImageLayout::TRANSFER_SRC_OPTIMAL
    {
        barrier.src_access_mask = vk::AccessFlags::MEMORY_READ;
        barrier.dst_access_mask = vk::AccessFlags::TRANSFER_READ;

        source_stage = vk::PipelineStageFlags::BOTTOM_OF_PIPE;
        destination_stage = vk::PipelineStageFlags::TRANSFER;
    } else if old_layout == device.final_layout()
        && new_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
    {
        barrier.src_access_mask = vk::AccessFlags::MEMORY_READ;
        barrier.dst_access_mask = vk::AccessFlags::TRANSFER_WRITE;

        source_stage = vk::PipelineStageFlags::BOTTOM_OF_PIPE;
        destination_stage = vk::PipelineStageFlags::TRANSFER;
    } else if old_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
        && new_layout == device.final_layout()
    {
        barrier.src_access_mask = vk::AccessFlags::TRANSFER_WRITE;
        barrier.dst_access_mask = vk::AccessFlags::MEMORY_READ;

        source_stage = vk::PipelineStageFlags::TRANSFER;
        destination_stage = vk::PipelineStageFlags::BOTTOM_OF_PIPE;
    } else if old_layout == vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        && new_layout == vk::ImageLayout::TRANSFER_SRC_OPTIMAL
    {
        barrier.src_access_mask = vk::AccessFlags::COLOR_ATTACHMENT_WRITE;
        barrier.dst_access_mask = vk::AccessFlags::TRANSFER_READ;

        source_stage = vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT;
        destination_stage = vk::PipelineStageFlags::TRANSFER;
    } else if old_layout == vk::ImageLayout::TRANSFER_SRC_OPTIMAL
        && new_layout == vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
    {
        barrier.src_access_mask = vk::AccessFlags::TRANSFER_READ;
        barrier.dst_access_mask = vk::AccessFlags::COLOR_ATTACHMENT_WRITE;

        source_stage = vk::PipelineStageFlags::TRANSFER;
        destination_stage = vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT;
    } else if old_layout == vk::ImageLayout::UNDEFINED && new_layout == vk::ImageLayout::GENERAL {
        barrier.src_access_mask = vk::AccessFlags::empty();
        barrier.dst_access_mask = vk::AccessFlags::MEMORY_READ;

        source_stage = vk::PipelineStageFlags::TOP_OF_PIPE;
        destination_stage = vk::PipelineStageFlags::TRANSFER;
    } else if old_layout == vk::ImageLayout::GENERAL
        && new_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
    {
        barrier.src_access_mask = vk::AccessFlags::MEMORY_READ;
        barrier.dst_access_mask = vk::AccessFlags::TRANSFER_WRITE;

        source_stage = vk::PipelineStageFlags::TRANSFER;
        destination_stage = vk::PipelineStageFlags::TRANSFER;
    } else if old_layout == vk::ImageLayout::TRANSFER_DST_OPTIMAL
        && new_layout == vk::ImageLayout::GENERAL
    {
        barrier.src_access_mask = vk::AccessFlags::TRANSFER_WRITE;
        barrier.dst_access_mask = vk::AccessFlags::MEMORY_READ;

        source_stage = vk::PipelineStageFlags::TRANSFER;
        destination_stage = vk::PipelineStageFlags::TRANSFER;
    } else if (old_layout == vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
        || old_layout == vk::ImageLayout::UNDEFINED
        || old_layout == device.final_layout())
        && new_layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
    {
        barrier.src_access_mask = vk::AccessFlags::COLOR_ATTACHMENT_WRITE;
        barrier.dst_access_mask = vk::AccessFlags::SHADER_READ;

        source_stage = vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT;
        destination_stage = vk::PipelineStageFlags::FRAGMENT_SHADER;

        needs_dependency = true;
    } else if old_layout == vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        && (new_layout == vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL
            || new_layout == device.final_layout())
    {
        barrier.src_access_mask =
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::SHADER_READ;
        barrier.dst_access_mask = vk::AccessFlags::COLOR_ATTACHMENT_WRITE;

        source_stage = vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
            | vk::PipelineStageFlags::FRAGMENT_SHADER;
        destination_stage = vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT;

        needs_dependency = true;
    } else if old_layout == vk::ImageLayout::UNDEFINED
        && new_layout == vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL
    {
        barrier.src_access_mask = vk::AccessFlags::MEMORY_WRITE | vk::AccessFlags::MEMORY_READ;
        barrier.dst_access_mask = vk::AccessFlags::MEMORY_WRITE | vk::AccessFlags::MEMORY_READ;

        source_stage = vk::PipelineStageFlags::ALL_GRAPHICS;
        destination_stage = vk::PipelineStageFlags::ALL_GRAPHICS;

        barrier.subresource_range.aspect_mask = vk::ImageAspectFlags::STENCIL;

        needs_dependency = true;
    } else {
        panic!("unsupported layout transition! old: {old_layout:?} -> new: {new_layout:?}");
    }

    unsafe {
        device.device.cmd_pipeline_barrier(
            command_buffer,
            source_stage,
            destination_stage,
            if needs_dependency {
                vk::DependencyFlags::BY_REGION
            } else {
                vk::DependencyFlags::empty()
            },
            &[],
            &[],
            &[barrier],
        );
    }

    image
        .layout
        .store(new_layout.into(), std::sync::atomic::Ordering::SeqCst);

    Ok(())
}

pub fn memory_barrier(
    frame_resources: &mut FrameResources,
    device: &Arc<LogicalDevice>,
    command_buffer: vk::CommandBuffer,
    buffer: &Arc<Buffer>,
    offset: vk::DeviceSize,
    size: vk::DeviceSize,
    buffer_access_type: vk::AccessFlags,
    before_command: bool,
    source_stage_flags: vk::PipelineStageFlags,
) -> anyhow::Result<()> {
    let mut barrier = vk::BufferMemoryBarrier::default();
    barrier.src_queue_family_index = vk::QUEUE_FAMILY_IGNORED;
    barrier.dst_queue_family_index = vk::QUEUE_FAMILY_IGNORED;
    barrier.buffer = buffer.get_buffer(frame_resources);
    barrier.offset = offset;
    barrier.size = size;

    let source_stage;
    let destination_stage;

    if before_command {
        barrier.src_access_mask = buffer_access_type;
        barrier.dst_access_mask = vk::AccessFlags::TRANSFER_WRITE;

        source_stage = source_stage_flags;
        destination_stage = vk::PipelineStageFlags::TRANSFER;
    } else {
        barrier.src_access_mask = vk::AccessFlags::TRANSFER_WRITE;
        barrier.dst_access_mask = buffer_access_type;

        source_stage = vk::PipelineStageFlags::TRANSFER;
        destination_stage = source_stage_flags;
    }

    unsafe {
        device.device.cmd_pipeline_barrier(
            command_buffer,
            source_stage,
            destination_stage,
            vk::DependencyFlags::empty(),
            &[],
            &[barrier],
            &[],
        );
    }

    Ok(())
}
