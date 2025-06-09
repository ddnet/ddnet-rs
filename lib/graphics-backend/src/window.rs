use std::{ffi::CStr, sync::Arc};

use ash::{prelude::VkResult, vk};
use either::Either;
use hiarc::Hiarc;
use native::native::{
    app::{MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH},
    NativeDisplayBackend, PhysicalSize,
};
use raw_window_handle::{DisplayHandle, HasDisplayHandle, HasWindowHandle, WindowHandle};

use crate::backends::vulkan::{
    frame_resources::FrameResources,
    image::Image,
    instance::Instance,
    logical_device::LogicalDevice,
    memory::MemoryImageBlock,
    queue::Queue,
    surface::SurfaceKHR,
    swapchain::{SwapchainKHR, SwapchainLostSurface},
    vulkan_allocator::VulkanAllocator,
};

#[derive(Debug, Hiarc)]
pub struct BackendDisplayRequirements {
    pub extensions: Vec<String>,
    pub is_headless: bool,
}

pub enum BackendRawDisplayHandle {
    Winit { handle: NativeDisplayBackend },
    Headless,
}

impl BackendRawDisplayHandle {
    fn vk_display_extensions(
        handle: &NativeDisplayBackend,
    ) -> VkResult<&'static [*const libc::c_char]> {
        use ash::khr::*;
        let extensions = match handle {
            NativeDisplayBackend::Windows => {
                const WINDOWS_EXTS: [*const libc::c_char; 2] =
                    [surface::NAME.as_ptr(), win32_surface::NAME.as_ptr()];
                &WINDOWS_EXTS
            }

            NativeDisplayBackend::Wayland => {
                const WAYLAND_EXTS: [*const libc::c_char; 2] =
                    [surface::NAME.as_ptr(), wayland_surface::NAME.as_ptr()];
                &WAYLAND_EXTS
            }

            NativeDisplayBackend::Xlib => {
                const XLIB_EXTS: [*const libc::c_char; 2] =
                    [surface::NAME.as_ptr(), xlib_surface::NAME.as_ptr()];
                &XLIB_EXTS
            }

            NativeDisplayBackend::Android => {
                const ANDROID_EXTS: [*const libc::c_char; 2] =
                    [surface::NAME.as_ptr(), android_surface::NAME.as_ptr()];
                &ANDROID_EXTS
            }

            NativeDisplayBackend::Apple => {
                const METAL_EXTS: [*const libc::c_char; 2] = [
                    surface::NAME.as_ptr(),
                    ash::ext::metal_surface::NAME.as_ptr(),
                ];
                &METAL_EXTS
            }
            NativeDisplayBackend::Unknown(handle) => {
                ash_window::enumerate_required_extensions(*handle)?
            }
        };

        Ok(extensions)
    }

    pub fn enumerate_required_vk_extensions(&self) -> Result<Vec<String>, vk::Result> {
        match self {
            Self::Winit { handle } => {
                let mut vk_extensions = Vec::<String>::new();
                let ext_list = Self::vk_display_extensions(handle)?;

                for ext in ext_list {
                    let ext_name = unsafe { CStr::from_ptr(*ext).to_str().unwrap().to_string() };
                    vk_extensions.push(ext_name);
                }

                Ok(vk_extensions)
            }
            Self::Headless => Ok(Default::default()),
        }
    }

    pub fn is_headless(&self) -> bool {
        match self {
            Self::Winit { .. } => false,
            Self::Headless => true,
        }
    }
}

pub enum BackendWindow<'a> {
    Winit { window: &'a native::native::Window },
    Headless { width: u32, height: u32 },
}

impl BackendWindow<'_> {
    pub fn is_headless(&self) -> bool {
        match self {
            BackendWindow::Winit { .. } => false,
            BackendWindow::Headless { .. } => true,
        }
    }

    pub fn create_fake_headless_surface() -> BackendSurfaceAndHandles<'static> {
        BackendSurfaceAndHandles::Headless {
            width: 64,
            height: 64,
            surface: Default::default(),
            should_render: false,
        }
    }

    pub fn create_surface(
        &self,
        entry: &ash::Entry,
        instance: &ash::Instance,
    ) -> Result<BackendSurfaceAndHandles<'_>, vk::Result> {
        match self {
            BackendWindow::Winit { window } => {
                if let Ok((dh, wh)) = window
                    .display_handle()
                    .and_then(|dh| window.window_handle().map(|wh| (dh, wh)))
                {
                    Ok(BackendSurfaceAndHandles::Winit {
                        surface: SurfaceKHR::new(entry, instance)
                            .map_err(|_| vk::Result::ERROR_UNKNOWN)?,
                        display_handle: dh,
                        window_handle: wh,
                    })
                }
                // fall back to a headless surface
                else {
                    Ok(Self::create_fake_headless_surface())
                }
            }
            BackendWindow::Headless { width, height } => Ok(BackendSurfaceAndHandles::Headless {
                width: *width,
                height: *height,
                surface: Default::default(),
                should_render: true,
            }),
        }
    }

    pub fn inner_size(&self) -> PhysicalSize<u32> {
        match self {
            BackendWindow::Winit { window } => window.inner_size().clamp(
                PhysicalSize {
                    width: MIN_WINDOW_WIDTH,
                    height: MIN_WINDOW_HEIGHT,
                },
                PhysicalSize {
                    width: u32::MAX,
                    height: u32::MAX,
                },
            ),
            BackendWindow::Headless { width, height } => PhysicalSize::<u32>::new(*width, *height),
        }
    }

    pub fn scale_factor(&self) -> f64 {
        match self {
            BackendWindow::Winit { window } => window.scale_factor().clamp(0.0001, f64::MAX),
            BackendWindow::Headless { .. } => 1.0,
        }
    }
}

#[derive(Debug, Hiarc, Default)]
pub struct BackendSurfaceHeadless {
    images: Vec<(Arc<Image>, MemoryImageBlock)>,
}

impl BackendSurfaceHeadless {
    fn create_surface_images_headless(
        &mut self,
        mem_allocator: &Arc<parking_lot::Mutex<VulkanAllocator>>,
        width: u32,
        height: u32,
    ) {
        let swap_chain_count = 2;

        self.images.reserve(swap_chain_count);

        let img_format = vk::Format::B8G8R8A8_UNORM;
        (0..swap_chain_count).for_each(|_| {
            let img_res = mem_allocator.lock().create_image_ex(
                width,
                height,
                1,
                1,
                img_format,
                vk::ImageTiling::OPTIMAL,
                vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::INPUT_ATTACHMENT
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::TRANSFER_DST,
                None,
            );
            if img_res.is_err() {
                panic!("failed to allocate images");
            }

            let (img, img_mem) = img_res.unwrap();

            self.images.push((img, img_mem));
        });
    }
}

#[derive(Debug, Hiarc)]
pub enum BackendSurfaceAndHandles<'a> {
    Winit {
        surface: Arc<SurfaceKHR>,
        #[hiarc_skip_unsafe]
        display_handle: DisplayHandle<'a>,
        #[hiarc_skip_unsafe]
        window_handle: WindowHandle<'a>,
    },
    Headless {
        width: u32,
        height: u32,
        surface: BackendSurfaceHeadless,
        /// if the headless surface was created as a result of a missing real surface
        /// it should not be rendered to if possible
        should_render: bool,
    },
}

impl BackendSurfaceAndHandles<'_> {
    /// # Safety
    /// see [`ash_window::create_surface`]
    pub unsafe fn create_vk_surface(
        self,
        entry: &ash::Entry,
        instance: &ash::Instance,
        mem_allocator: &Arc<parking_lot::Mutex<VulkanAllocator>>,
    ) -> anyhow::Result<BackendSurface> {
        match self {
            Self::Winit {
                mut surface,
                display_handle,
                window_handle,
            } => {
                let surf = ash_window::create_surface(
                    entry,
                    instance,
                    display_handle.as_raw(),
                    window_handle.as_raw(),
                    None,
                )?;
                surface = SurfaceKHR::from_existing(surface.ash_surface.clone(), surf);
                Ok(BackendSurface::Winit { surface })
            }
            Self::Headless {
                mut surface,
                width,
                height,
                should_render,
            } => {
                surface.create_surface_images_headless(mem_allocator, width, height);
                Ok(BackendSurface::Headless {
                    width,
                    height,
                    surface,
                    should_render,
                })
            }
        }
    }
}

#[derive(Debug, Hiarc)]
pub enum BackendSurface {
    Winit {
        surface: Arc<SurfaceKHR>,
    },
    Headless {
        width: u32,
        height: u32,
        surface: BackendSurfaceHeadless,
        /// if the headless surface was created as a result of a missing real surface
        /// it should not be rendered to if possible
        should_render: bool,
    },
}

impl BackendSurface {
    pub fn create_swapchain(
        &self,
        instance: &Arc<Instance>,
        device: &Arc<LogicalDevice>,
        queue: &Arc<Queue>,
    ) -> Result<BackendSwapchain, vk::Result> {
        match self {
            Self::Winit { .. } => Ok(BackendSwapchain::Winit {
                swapchain: SwapchainKHR::new(&instance.vk_instance, &device.device)
                    .map_err(|_| vk::Result::ERROR_UNKNOWN)?,
                device: device.clone(),
                queue: queue.clone(),
                out_of_date: false,
            }),
            Self::Headless {
                surface,
                should_render,
                ..
            } => Ok(BackendSwapchain::Headless {
                images: surface
                    .images
                    .iter()
                    .map(|(img, _)| img.img(&mut FrameResources::new(None)))
                    .collect(),
                device: device.clone(),
                queue: queue.clone(),
                can_render: *should_render,
                old_swapchain: None,
            }),
        }
    }

    pub unsafe fn get_physical_device_surface_support(
        &self,
        physical_device: vk::PhysicalDevice,
        queue_family_index: u32,
    ) -> VkResult<bool> {
        match self {
            BackendSurface::Winit { surface } => {
                surface.ash_surface.get_physical_device_surface_support(
                    physical_device,
                    queue_family_index,
                    surface.surface,
                )
            }
            BackendSurface::Headless { .. } => Ok(true),
        }
    }

    pub unsafe fn get_physical_device_surface_formats(
        &self,
        physical_device: vk::PhysicalDevice,
    ) -> VkResult<Vec<vk::SurfaceFormatKHR>> {
        match self {
            BackendSurface::Winit { surface } => surface
                .ash_surface
                .get_physical_device_surface_formats(physical_device, surface.surface),
            BackendSurface::Headless { .. } => Ok(vec![vk::SurfaceFormatKHR {
                format: vk::Format::B8G8R8A8_UNORM,
                color_space: vk::ColorSpaceKHR::SRGB_NONLINEAR,
            }]),
        }
    }

    pub unsafe fn get_physical_device_surface_present_modes(
        &self,
        physical_device: vk::PhysicalDevice,
    ) -> VkResult<Vec<vk::PresentModeKHR>> {
        match self {
            BackendSurface::Winit { surface } => surface
                .ash_surface
                .get_physical_device_surface_present_modes(physical_device, surface.surface),
            BackendSurface::Headless { .. } => Ok(vec![vk::PresentModeKHR::IMMEDIATE]),
        }
    }

    pub unsafe fn get_physical_device_surface_capabilities(
        &self,
        physical_device: vk::PhysicalDevice,
    ) -> VkResult<vk::SurfaceCapabilitiesKHR> {
        match self {
            BackendSurface::Winit { surface } => surface
                .ash_surface
                .get_physical_device_surface_capabilities(physical_device, surface.surface),
            BackendSurface::Headless { width, height, .. } => {
                let ext = vk::Extent2D {
                    width: *width,
                    height: *height,
                };
                // use build here, but make sure the lifetime is 'static
                Ok(vk::SurfaceCapabilitiesKHR::default()
                    .min_image_count(2)
                    .max_image_count(2)
                    .current_extent(ext)
                    .max_image_extent(ext)
                    .min_image_extent(ext)
                    .supported_usage_flags(
                        vk::ImageUsageFlags::COLOR_ATTACHMENT
                            | vk::ImageUsageFlags::TRANSFER_SRC
                            | vk::ImageUsageFlags::TRANSFER_DST,
                    ))
            }
        }
    }

    pub fn can_render(&self) -> bool {
        match self {
            BackendSurface::Winit { .. } => true,
            BackendSurface::Headless { should_render, .. } => *should_render,
        }
    }

    pub fn replace(&mut self, new: Self) {
        drop(std::mem::replace(self, new));
    }
}

#[derive(Debug, Hiarc)]
pub enum BackendSwapchain {
    Winit {
        swapchain: Arc<SwapchainKHR>,

        device: Arc<LogicalDevice>,
        queue: Arc<Queue>,

        /// If a vulkan command (usually queue present) returned `VK_ERROR_OUT_OF_DATE_KHR`
        /// then this is `true`
        out_of_date: bool,
    },
    Headless {
        images: Vec<vk::Image>,
        device: Arc<LogicalDevice>,
        queue: Arc<Queue>,
        can_render: bool,
        /// An old surface from a non-headless client.
        ///
        /// If `None` that always means that this is a pure headless swapchain and
        /// cannot be upgraded ot a non headless one (returns an error in such cases).
        ///
        /// If a vulkan command (usually queue present) returned `VK_ERROR_OUT_OF_DATE_KHR`
        /// then this is set to `Some(Either::Right(SwapchainLostSurface))`.
        old_swapchain: Option<Either<Arc<SwapchainKHR>, SwapchainLostSurface>>,
    },
}

impl BackendSwapchain {
    pub unsafe fn queue_present(
        &self,
        queue: vk::Queue,
        present_info: vk::PresentInfoKHR,
    ) -> VkResult<bool> {
        match self {
            BackendSwapchain::Winit { swapchain, .. } => {
                let swap_chains = [swapchain.swapchain];
                swapchain
                    .ash_swapchain
                    .queue_present(queue, &present_info.swapchains(&swap_chains))
            }
            BackendSwapchain::Headless { .. } => Ok(false),
        }
    }

    pub unsafe fn acquire_next_image(
        &self,
        timeout: u64,
        semaphore: vk::Semaphore,
        fence: vk::Fence,
    ) -> VkResult<(u32, bool)> {
        match self {
            BackendSwapchain::Winit { swapchain, .. } => swapchain
                .ash_swapchain
                .acquire_next_image(swapchain.swapchain, timeout, semaphore, fence),
            BackendSwapchain::Headless {
                device,
                queue,
                can_render,
                ..
            } => {
                // TODO: remove this wait idle call. better do it over semaphores
                let queue_guard = queue.queues.lock();
                device.device.device_wait_idle().unwrap();
                drop(queue_guard);
                if *can_render {
                    if semaphore != vk::Semaphore::null() {
                        let counter = device
                            .device
                            .get_semaphore_counter_value(semaphore)
                            .unwrap();
                        let signal_info = vk::SemaphoreSignalInfo::default()
                            .semaphore(semaphore)
                            .value(counter + 1);
                        device.device.signal_semaphore(&signal_info).unwrap();
                    }
                    if fence != vk::Fence::null() {
                        device.device.reset_fences(&[fence]).unwrap();
                    }
                }
                Ok((0, false))
            }
        }
    }

    pub unsafe fn create_swapchain(
        &mut self,
        surface: &BackendSurface,
        mut swap_info: vk::SwapchainCreateInfoKHR,
    ) -> VkResult<()> {
        let mut res = Ok(());
        replace_with::replace_with_or_abort(self, |self_| {
            match self_ {
                Self::Winit {
                    swapchain,
                    device,
                    queue,
                    out_of_date,
                } => match surface {
                    BackendSurface::Winit { surface } => {
                        let old_swap_chain = swapchain;
                        let ash_swapchain = old_swap_chain.ash_swapchain.clone();

                        let new_surface = surface.surface;
                        swap_info.surface = new_surface;
                        if out_of_date {
                            drop(old_swap_chain);
                        } else if old_swap_chain.surface == new_surface {
                            swap_info.old_swapchain = old_swap_chain.swapchain;
                        }

                        match SwapchainKHR::new_with_alloc(
                            ash_swapchain.clone(),
                            swap_info,
                            new_surface,
                        ) {
                            Ok(swapchain) => Self::Winit {
                                swapchain,
                                device,
                                queue,
                                out_of_date,
                            },
                            Err(err) => {
                                res = Err(err);

                                Self::Headless {
                                    images: Default::default(),
                                    device,
                                    queue,
                                    can_render: false,
                                    old_swapchain: Some(Either::Right(SwapchainLostSurface {
                                        ash_swapchain,
                                    })),
                                }
                            }
                        }
                    }
                    BackendSurface::Headless {
                        surface,
                        should_render,
                        ..
                    } => Self::Headless {
                        images: surface
                            .images
                            .iter()
                            .map(|(img, _)| img.img(&mut FrameResources::new(None)))
                            .collect(),
                        device: device.clone(),
                        queue: queue.clone(),
                        can_render: *should_render,
                        old_swapchain: Some(if out_of_date {
                            Either::Right(SwapchainLostSurface {
                                ash_swapchain: swapchain.ash_swapchain.clone(),
                            })
                        } else {
                            Either::Left(swapchain.clone())
                        }),
                    },
                },
                Self::Headless {
                    old_swapchain,
                    queue,
                    device,
                    can_render,
                    images,
                } => match surface {
                    BackendSurface::Winit { surface } => {
                        let old_swapchain = match old_swapchain {
                            Some(old_swapchain) => old_swapchain,
                            None => {
                                res = Err(vk::Result::ERROR_FEATURE_NOT_PRESENT);
                                return Self::Headless {
                                    images,
                                    device,
                                    queue,
                                    can_render,
                                    old_swapchain,
                                };
                            }
                        };
                        let new_surface = surface.surface;
                        swap_info.surface = new_surface;

                        let ash_swapchain = match &old_swapchain {
                            Either::Left(old_swapchain) => old_swapchain.ash_swapchain.clone(),
                            Either::Right(old_swapchain) => old_swapchain.ash_swapchain.clone(),
                        };

                        // check if old swapchain should be reused
                        if let Either::Left(old_swapchain) = &old_swapchain {
                            if old_swapchain.surface == new_surface {
                                swap_info.old_swapchain = old_swapchain.swapchain;
                            }
                        }

                        match SwapchainKHR::new_with_alloc(ash_swapchain, swap_info, new_surface) {
                            Ok(swapchain) => Self::Winit {
                                swapchain,
                                device: device.clone(),
                                queue: queue.clone(),
                                out_of_date: false,
                            },
                            Err(err) => {
                                res = Err(err);
                                Self::Headless {
                                    images,
                                    device,
                                    queue,
                                    can_render,
                                    old_swapchain: Some(old_swapchain),
                                }
                            }
                        }
                    }
                    BackendSurface::Headless { .. } => Self::Headless {
                        images,
                        device,
                        queue,
                        can_render,
                        old_swapchain,
                    },
                },
            }
        });
        res
    }

    pub unsafe fn get_swapchain_images(&self) -> VkResult<Vec<vk::Image>> {
        match self {
            BackendSwapchain::Winit { swapchain, .. } => swapchain
                .ash_swapchain
                .get_swapchain_images(swapchain.swapchain),
            BackendSwapchain::Headless { images, .. } => Ok(images.clone()),
        }
    }

    pub fn out_of_date_ntf(&mut self) {
        match self {
            BackendSwapchain::Winit { out_of_date, .. } => {
                *out_of_date = true;
            }
            BackendSwapchain::Headless { old_swapchain, .. } => {
                *old_swapchain = old_swapchain.take().map(|s| match s {
                    Either::Left(swapchain) => Either::Right(SwapchainLostSurface {
                        ash_swapchain: swapchain.ash_swapchain.clone(),
                    }),
                    Either::Right(surfacelost) => Either::Right(surfacelost),
                });
            }
        }
    }
}
