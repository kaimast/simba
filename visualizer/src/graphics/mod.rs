mod data;
pub use data::*;

mod render_loop;
pub use render_loop::RenderLoop;

mod rectangle;
pub use rectangle::RectangleStyle;

mod circle;
pub use circle::CircleStyle;

mod line;
pub use line::LineStyle;

mod camera;
pub use camera::{Camera, InputDirection};

mod renderer;
pub use renderer::{Geometry, Material, Program, Renderer};

mod drawable;
pub use drawable::Drawable;

use std::sync::atomic::Ordering;

use winit::window::Window;

use wgpu::util::DeviceExt;

use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use glam::{Mat4, Vec2};

pub type DrawableId = u64;

pub const MAX_Z_INDEX: u16 = 100;

pub struct Graphics {
    vp_buffer: Arc<wgpu::Buffer>,
    next_drawable_id: AtomicU64,
    renderer: Arc<Renderer>,
}

impl Graphics {
    pub async fn new<'a>(window: &Window) -> anyhow::Result<(Self, wgpu::Surface<'a>)> {
        let (renderer, surface) = Renderer::new(window).await?;

        let next_drawable_id = AtomicU64::new(1);

        let vp_buffer = {
            let device = renderer.get_device();

            let view_matrix = Mat4::ZERO;
            let proj_matrix = Mat4::ZERO;

            let view_bytes = view_matrix.to_cols_array();
            let proj_bytes = proj_matrix.to_cols_array();

            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                contents: [
                    bytemuck::cast_slice(&view_bytes),
                    bytemuck::cast_slice(&proj_bytes),
                ]
                .concat()
                .as_slice(),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
                label: Some("Camera View&Projection buffer"),
            });

            Arc::new(buffer)
        };

        Ok((
            Self {
                renderer: Arc::new(renderer),
                next_drawable_id,
                vp_buffer,
            },
            surface,
        ))
    }

    pub fn get_renderer(&self) -> Arc<Renderer> {
        self.renderer.clone()
    }

    pub async fn create_camera(&self, min_pos: Vec2, max_pos: Vec2) -> Arc<Camera> {
        Arc::new(
            Camera::new(
                self.renderer.clone(),
                self.vp_buffer.clone(),
                min_pos,
                max_pos,
            )
            .await,
        )
    }

    pub async fn create_rectangle(
        &self,
        center: glam::Vec2,
        z_index: u16,
        style: RectangleStyle,
    ) -> Arc<Drawable> {
        let drawable_id = self.next_drawable_id.fetch_add(1, Ordering::SeqCst);
        let vp_buffer = self.vp_buffer.clone();

        Arc::new(
            rectangle::new_drawable(
                drawable_id,
                center,
                z_index,
                style,
                self.renderer.clone(),
                vp_buffer,
            )
            .await,
        )
    }

    pub async fn create_circle(
        &self,
        center: glam::Vec2,
        z_index: u16,
        style: CircleStyle,
    ) -> Arc<Drawable> {
        let drawable_id = self.next_drawable_id.fetch_add(1, Ordering::SeqCst);
        let vp_buffer = self.vp_buffer.clone();

        Arc::new(
            circle::new_drawable(
                drawable_id,
                center,
                z_index,
                style,
                self.renderer.clone(),
                vp_buffer,
            )
            .await,
        )
    }

    pub async fn create_line(
        &self,
        start: glam::Vec2,
        end: glam::Vec2,
        z_index: u16,
        style: LineStyle,
    ) -> Arc<Drawable> {
        let drawable_id = self.next_drawable_id.fetch_add(1, Ordering::SeqCst);
        let vp_buffer = self.vp_buffer.clone();

        Arc::new(
            line::new_drawable(
                drawable_id,
                start,
                end,
                z_index,
                style,
                self.renderer.clone(),
                vp_buffer,
            )
            .await,
        )
    }

    pub async fn draw(
        &self,
        render_buffer: &wgpu::TextureView,
        elapsed: f64,
        camera: &Camera,
        mut drawables: Vec<Arc<Drawable>>,
    ) -> Vec<wgpu::CommandBuffer> {
        // Update camera first (if needed)
        let mut commands = vec![];
        if let Some(cmds) = camera.update(elapsed).await {
            commands.push(cmds);
        }

        drawables.sort_unstable_by(|d1, d2| {
            use std::cmp::Ordering;

            match d1.get_z_index().cmp(&d2.get_z_index()) {
                Ordering::Greater => Ordering::Greater,
                Ordering::Less => Ordering::Less,
                Ordering::Equal => {
                    // Use drawable ID as tie-breaker
                    d1.get_identifier().cmp(&d2.get_identifier())
                }
            }
        });

        // Also sort by idx so overlapping nodes always overlap the same way
        for drawable in drawables.iter() {
            let cmds = drawable.draw(render_buffer).await;
            commands.push(cmds);
        }

        commands
    }
}
