use tokio::sync::Mutex;

use winit::dpi::PhysicalSize;
use winit::window::Window;

use wgpu::util::power_preference_from_env;

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::{Mutex as PlMutex, MutexGuard as PlMutexGuard};

mod program;
pub use program::Program;

use anyhow::Context;

use super::{circle, line, rectangle};

#[derive(Clone, Debug)]
pub struct Geometry {
    pub window_size: PhysicalSize<u32>,
    pub scale_factor: f64,
    pub dirty: bool,
}

pub struct Renderer {
    adapter: wgpu::Adapter,
    queue: wgpu::Queue,
    texture_format: wgpu::TextureFormat,
    device: wgpu::Device,
    geometry: PlMutex<Geometry>,
    programs: Mutex<HashMap<String, Arc<Program>>>,
    materials: Mutex<HashMap<String, Arc<Material>>>,
}

pub struct Material {
    pub pipeline: wgpu::RenderPipeline,
    pub uniform_bind_group_layout: wgpu::BindGroupLayout,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
}

impl Renderer {
    pub async fn new<'a>(window: &Window) -> anyhow::Result<(Self, wgpu::Surface<'a>)> {
        let backends = wgpu::util::backend_bits_from_env().unwrap_or(wgpu::Backends::all());
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });

        log::debug!("Set WGPU backends to {backends:?}");

        log::info!("Initializing WebGPU renderer");
        //FIXME use safe variant here...
        let surface = unsafe {
            let target = wgpu::SurfaceTargetUnsafe::from_window(window)?;
            instance.create_surface_unsafe(target)?
        };
        let window_size = window.inner_size();
        let scale_factor = window.scale_factor();

        log::debug!("Detected window_size={window_size:?} and scale_factor={scale_factor}");

        // Using high power breaks Wayland on prime currently
        let adapter: wgpu::Adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: power_preference_from_env()
                    .unwrap_or(wgpu::PowerPreference::LowPower),
                force_fallback_adapter: false,
            })
            .await
            .with_context(|| "Failed to find a GPU")?;

        let limits = wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits());

        log::debug!("Setting up graphics device \"{}\"", adapter.get_info().name);
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("SimBA graphics device"),
                    required_features: wgpu::Features::default(),
                    required_limits: limits,
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .expect("Failed to get graphics device");
        //            .with_context(|| "Get graphics device")?;

        log::debug!("Creating shader programs");
        let mut programs = HashMap::new();
        programs.insert(
            "circle".to_string(),
            Arc::new(circle::create_program(&device)),
        );
        programs.insert("line".to_string(), Arc::new(line::create_program(&device)));
        programs.insert(
            "rectangle".to_string(),
            Arc::new(rectangle::create_program(&device)),
        );

        let geometry = PlMutex::new(Geometry {
            window_size,
            scale_factor,
            dirty: false,
        });

        let texture_format = *surface
            .get_capabilities(&adapter)
            .formats
            .first()
            .with_context(|| "Get texture format")?;

        let obj = Self {
            geometry,
            adapter,
            queue,
            texture_format,
            device,
            programs: Mutex::new(programs),
            materials: Mutex::new(HashMap::new()),
        };

        log::debug!("Creating materials");
        {
            let mut materials = obj.materials.lock().await;
            materials.insert(
                "circle".to_string(),
                Arc::new(circle::create_material(&obj).await),
            );
            materials.insert(
                "line".to_string(),
                Arc::new(line::create_material(&obj).await),
            );
            materials.insert(
                "rectangle".to_string(),
                Arc::new(rectangle::create_material(&obj).await),
            );
        }

        Ok((obj, surface))
    }

    pub fn set_scale_factor(&self, scale_factor: f64) {
        log::debug!("Scale factor changed to {scale_factor}.");
        let mut geometry = self.geometry.lock();
        geometry.scale_factor = scale_factor;
        geometry.dirty = true;
    }

    /// Note: Calling this while holding other locks (e.g., to device) may cause a deadlock
    pub fn set_window_size(&self, size: PhysicalSize<u32>) {
        log::debug!("Window was resized to {size:?}.");
        let mut geometry = self.geometry.lock();
        geometry.window_size = size;
        geometry.dirty = true;
    }

    pub fn get_adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    pub fn get_geometry(&self) -> PlMutexGuard<'_, Geometry> {
        self.geometry.lock()
    }

    pub async fn get_program(&self, name: &str) -> Arc<Program> {
        let programs = self.programs.lock().await;

        programs.get(name).expect("No such program").clone()
    }

    pub async fn get_material(&self, name: &str) -> Arc<Material> {
        self.materials
            .lock()
            .await
            .get(name)
            .expect("No such material")
            .clone()
    }

    pub fn get_texture_format(&self) -> wgpu::TextureFormat {
        self.texture_format
    }

    pub fn get_device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn get_render_queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    #[tracing::instrument(skip(self))]
    pub fn make_command_encoder(&self) -> wgpu::CommandEncoder {
        self.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None })
    }
}
