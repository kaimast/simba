use std::process::exit;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use instant::Instant;

use simba::Simulation;

use wgpu::{LoadOp, RenderPassDescriptor, StoreOp, SurfaceConfiguration, TextureUsages};

use crate::graphics::Graphics;
use crate::scene::SceneManager;
use crate::ui::{CursorPosition, UiEvents, UiMessages, UiRenderLoop};

pub struct RenderContext<'a> {
    pub surface: wgpu::Surface<'a>,
    pub depth_buffer: wgpu::Texture,
}

pub struct RenderLoop<'a> {
    graphics: Arc<Graphics>,
    ui_render_loop: UiRenderLoop,
    window: Arc<winit::window::Window>,
    scene_mgr: Arc<SceneManager>,
    render_context: RenderContext<'a>,
    stop_flag: Arc<AtomicBool>,
}

impl<'a> RenderLoop<'a> {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        graphics: Arc<Graphics>,
        ui_messages: Arc<UiMessages>,
        ui_events: Arc<UiEvents>,
        cursor_position: Arc<CursorPosition>,
        window: winit::window::Window,
        surface: wgpu::Surface<'a>,
        simulation: Arc<Simulation>,
        scene_mgr: Arc<SceneManager>,
        stop_flag: Arc<AtomicBool>,
    ) -> Self {
        let renderer = graphics.get_renderer();
        let adapter = renderer.get_adapter();
        let window = Arc::new(window);

        let depth_buffer = {
            let device = renderer.get_device();
            let geometry = renderer.get_geometry();

            log::debug!("Creating render surface");
            Self::update_surface(&surface, adapter, device, &geometry.window_size);

            log::debug!("Creating depth buffer");
            Self::make_depth_buffer(device, &geometry.window_size)
        };

        let ui_render_loop = UiRenderLoop::new(
            renderer,
            ui_messages,
            ui_events,
            cursor_position,
            window.clone(),
            simulation,
            scene_mgr.clone(),
        )
        .await;

        let render_context = RenderContext {
            surface,
            depth_buffer,
        };

        Self {
            graphics,
            window,
            ui_render_loop,
            scene_mgr,
            render_context,
            stop_flag,
        }
    }

    #[tracing::instrument(skip(self))]
    pub async fn run(&mut self) {
        let mut last_frame_time = Instant::now();

        //TODO only draw when something changes
        while !self.stop_flag.load(Ordering::Relaxed) {
            // Don't draw too frequently
            let start = Instant::now();
            let elapsed = start - last_frame_time;

            self.scene_mgr.update();
            self.draw(elapsed.as_secs_f64()).await;

            last_frame_time = start;
        }
    }

    #[tracing::instrument(skip(self))]
    async fn draw(&mut self, elapsed: f64) {
        let renderer = self.graphics.get_renderer();
        let adapter = renderer.get_adapter();
        let device = renderer.get_device();

        let geometry = {
            let mut geometry = renderer.get_geometry();
            if geometry.dirty {
                geometry.dirty = false;

                Self::update_surface(
                    &self.render_context.surface,
                    adapter,
                    device,
                    &geometry.window_size,
                );

                self.render_context.depth_buffer =
                    Self::make_depth_buffer(device, &geometry.window_size);
            }
            (*geometry).clone()
        };

        log::trace!("Preparing to draw next frame");

        // Clears the screen and gives us a buffer to render to
        let swap_frame = match self.render_context.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Outdated) => {
                log::debug!("Got outdated frame. Window might be resizing...");
                return;
            }
            Err(wgpu::SurfaceError::Timeout) => {
                log::debug!("Got swap chain timeout. Retrying..");
                return;
            }
            Err(error) => {
                log::error!("Got unexpected swap chain error: {error}");
                exit(-1);
            }
        };

        let surface_view = {
            swap_frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default())
        };

        let queue = renderer.get_render_queue();

        let mut commands: Vec<wgpu::CommandBuffer> = vec![];
        {
            let mut encoder = renderer.make_command_encoder();
            encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("main render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: LoadOp::Clear(wgpu::Color {
                            r: 0.9,
                            g: 0.9,
                            b: 0.9,
                            a: 1.0,
                        }),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            commands.push(encoder.finish())
        }

        log::trace!("Drawing scene");
        let mut scene_commands = {
            let (camera, drawables) = self.scene_mgr.get_drawables().await;
            self.graphics
                .draw(&surface_view, elapsed, camera, drawables)
                .await
        };

        commands.append(&mut scene_commands);
        queue.submit(commands);

        // Draw UI on top of the rest
        log::trace!("Drawing UI");
        self.ui_render_loop
            .update_and_draw(geometry, &self.window, &surface_view)
            .await;

        drop(surface_view);

        log::trace!("Presenting frame");
        swap_frame.present();
    }

    fn make_depth_buffer(
        device: &wgpu::Device,
        size: &winit::dpi::PhysicalSize<u32>,
    ) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth Buffer"),
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    }

    fn update_surface(
        surface: &wgpu::Surface,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        size: &winit::dpi::PhysicalSize<u32>,
    ) {
        let format = *surface
            .get_capabilities(adapter)
            .formats
            .first()
            .expect("No supported texture format found");

        surface.configure(device, &SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width,
            height: size.height,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            present_mode: wgpu::PresentMode::AutoVsync,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        })
    }
}
