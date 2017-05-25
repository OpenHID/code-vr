mod text;

use winit::{WindowBuilder, get_available_monitors, get_primary_monitor, Event, ElementState};
use vulkano_win::{Window, VkSurfaceBuild, required_extensions};
use vulkano::instance::{Instance, PhysicalDevice};
use vulkano::device::{Queue, Device, DeviceExtensions};
use vulkano::swapchain::{Swapchain, SurfaceTransform, PresentMode};
use vulkano::image::SwapchainImage;
use vulkano::image::attachment::AttachmentImage;
use vulkano::framebuffer::Framebuffer;
use vulkano::command_buffer::{PrimaryCommandBufferBuilder, Submission, submit};
use vulkano::format;

use std::clone::Clone;
use std::sync::Arc;
use std::time::Duration;
use std::collections::HashMap;
use engine::config::Config;

mod render_pass {

    use vulkano::format;
    use vulkano::format::Format;

    single_pass_renderpass!{
    attachments: {
        color: {
            load: Clear,
            store: Store,
            format: Format,
        },
        depth: {
            load: Clear,
            store: DontCare,
            format: format::D16Unorm,
            }
        },
        pass: {
            color: [color],
            depth_stencil: {depth}
        }
    }
}

pub struct Renderer {
    config: Arc<Config>,
    window: Arc<Window>,
    instance: Arc<Instance>,
    physical_device: usize,
    device: Arc<Device>,
    swapchain: Arc<Swapchain>,
    images: Vec<Arc<SwapchainImage>>,
    depth_buffer: Arc<AttachmentImage<format::D16Unorm>>,
    render_pass: Arc<render_pass::CustomRenderPass>,
    framebuffers: Vec<Arc<Framebuffer<render_pass::CustomRenderPass>>>,
    submissions: Vec<Arc<Submission>>,
    queue: Arc<Queue>
}

impl Renderer {
    pub fn new(window_builder: WindowBuilder, config: Arc<Config>) -> (Renderer, Arc<Window>) {
        
        let instance = {
            let extensions = required_extensions();
            Instance::new(None, &extensions, None).expect("Failed to create Vulkan instance.")
        };

        let ins = &instance.clone();

        let physical = PhysicalDevice::enumerate(&ins)
            .next()
            .expect("No vulkan device is available.");

        let physical_device = physical.index();

        let window = Arc::new(window_builder.build_vk_surface(&instance).unwrap());

        let queue = physical
            .queue_families()
            .find(|q| q.supports_graphics() && window.surface().is_supported(q).unwrap_or(false))
            .expect("Couldn't find a graphical queue family.");

        // Logical Device, Queues
        let (device, mut queues) = {
            let device_ext = DeviceExtensions {
                khr_swapchain: true,
                ..DeviceExtensions::none()
            };

            Device::new(&physical,
                        physical.supported_features(),
                        &device_ext,
                        [(queue, 0.5)].iter().cloned())
                    .expect("failed to create device")
        };

        // Device Queue
        let queue = queues.next().unwrap();

        // Swapchain, Swapchain Images
        let (swapchain, images) =
            create_swapchain(&window, &physical, &device, &queue, None, &config);

        let depth_buffer =
            AttachmentImage::transient(&device, images[0].dimensions(), format::D16Unorm).unwrap();

        // Render Pass
        let render_pass =
            render_pass::CustomRenderPass::new(&device,
                                               &render_pass::Formats {
                                                    // Use the format of the images and one sample.
                                                    color: (images[0].format(), 1),
                                                    depth: (format::D16Unorm, 1),
                                                })
                    .unwrap();

        let framebuffers = images
            .iter()
            .map(|image| {
                let attachments = render_pass::AList {
                    color: &image,
                    depth: &depth_buffer,
                };

                Framebuffer::new(&render_pass,
                                 [image.dimensions()[0], image.dimensions()[1], 1],
                                 attachments)
                        .unwrap()
            })
            .collect::<Vec<_>>();

        // Queue Submissions
        let submissions = Vec::new();

        (Renderer {
            instance,
            physical_device,
            device,
            swapchain,
            images,
            depth_buffer,
            framebuffers,
            render_pass,
            submissions,
            queue,
            window: window.clone(),
            config
        }, window)
    }

    pub fn resize(&mut self) {
                    let (swapchain, images) =
                        create_swapchain(&self.window, 
                                         &PhysicalDevice::from_index(&self.instance, self.physical_device).unwrap(),
                                         &self.device,
                                         &self.queue,
                                         Some(&self.swapchain),
                                         &self.config);
                    self.swapchain = swapchain;
                    self.images = images;
                    self.depth_buffer = AttachmentImage::transient(&self.device,
                                                                   self.images[0].dimensions(),
                                                                   format::D16Unorm).unwrap();
                    self.framebuffers = self.images
                        .iter()
                        .map(|image| {
                            let attachments = render_pass::AList {
                                color: &image,
                                depth: &self.depth_buffer,
                            };

                            Framebuffer::new(&self.render_pass,
                                             [image.dimensions()[0], image.dimensions()[1], 1],
                                             attachments)
                                    .unwrap()
                        })
                        .collect::<Vec<_>>();
    }

    pub fn render(&mut self) {
                let command_buffers = self.framebuffers
            .iter()
            .map(|framebuffer| {
                PrimaryCommandBufferBuilder::new(&self.device, self.queue.family())
                    .draw_inline(&self.render_pass,
                                 &framebuffer,
                                 render_pass::ClearValues {
                                     color: [0.2, 0.4, 0.8, 1.0],
                                     depth: 1.0,
                                 })
                    .draw_end()
                    .build()
            })
            .collect::<Vec<_>>();
        let image_num = self.swapchain
            .acquire_next_image(Duration::new(1, 0))
            .unwrap();

        // @TODO build command buffers with threads and submit the changes in main thread (here)
        self.submissions
            .push(submit(&command_buffers[image_num], &self.queue).unwrap());

        self.swapchain.present(&self.queue, image_num).unwrap();
    }
}


/// Sets up and creates a swapchain
fn create_swapchain(window: &Window,
                    physical_device: &PhysicalDevice,
                    device: &Arc<Device>,
                    queue: &Arc<Queue>,
                    old: Option<&Arc<Swapchain>>,
                    config: &Config)
                    -> (Arc<Swapchain>, Vec<Arc<SwapchainImage>>) {
    {
        let caps = window
            .surface()
            .get_capabilities(&physical_device)
            .expect("failed to get surface capabilities");

            


        let dimensions = if config.window.resolution[0] <= 240 ||
                            config.window.resolution[1] <= 240 {

            let min = caps.min_image_extent;

            let extent = caps.current_extent.unwrap_or([800, 600]);

            if extent[0] < min[0] || extent[1] < min[1] {
                min
            }
            else {
                extent
            }
        } else {
            config.window.resolution
        };


        let present = if config.graphics.vsync &&
                         caps.present_modes.supports(PresentMode::Mailbox) {
            PresentMode::Mailbox
        } else {
            caps.present_modes.iter().next().unwrap()
        };

        let alpha = caps.supported_composite_alpha.iter().next().unwrap();

        let format = caps.supported_formats[0].0;

        Swapchain::new(&device,
                       &window.surface(),
                       caps.min_image_count,
                       format,
                       dimensions,
                       1,
                       &caps.supported_usage_flags,
                       queue,
                       SurfaceTransform::Identity,
                       alpha,
                       present,
                       true,
                       old)
                .expect("failed to create swapchain")
    }
}