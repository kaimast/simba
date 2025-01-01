use std::sync::Arc;

use parking_lot::Mutex;

use glam::Vec2;

use wgpu::util::DeviceExt;
use wgpu::{
    BindGroup, Buffer, CommandBuffer, LoadOp, RenderPassColorAttachment, RenderPassDescriptor,
    StoreOp, TextureView,
};

use crate::graphics::{BoundingBox, DrawableId, Material, Renderer};

//TODO refactor this
pub struct Drawable {
    pub(super) identifier: DrawableId,
    pub(super) material: Arc<Material>,
    pub(super) renderer: Arc<Renderer>,
    pub(super) style_buffer: Buffer,
    pub(super) uniform_bind_group: BindGroup,
    pub(super) position: Vec2,
    pub(super) z_index: u16,
    pub(super) bounding_box: BoundingBox,
    pub(super) style_bytes: Mutex<Option<Vec<u8>>>,
}

impl Drawable {
    pub(super) async fn draw(&self, render_buffer: &TextureView) -> CommandBuffer {
        let mut encoder = self.renderer.make_command_encoder();

        if let Some(style_bytes) = self.style_bytes.lock().take() {
            let device = self.renderer.get_device();
            let staging_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                contents: &style_bytes,
                usage: wgpu::BufferUsages::COPY_SRC,
                label: None,
            });

            encoder.copy_buffer_to_buffer(
                &staging_buffer,
                0,
                &self.style_buffer,
                0,
                style_bytes.len() as u64,
            );
        }

        let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("sprite render pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: render_buffer,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });

        render_pass.set_pipeline(&self.material.pipeline);
        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.material.vertex_buffer.slice(0..));
        render_pass.set_index_buffer(
            self.material.index_buffer.slice(0..),
            wgpu::IndexFormat::Uint16,
        );

        render_pass.draw_indexed(0..6, 0, 0..1);

        drop(render_pass);

        encoder.finish()
    }

    pub fn set_style<T: bytemuck::Zeroable + bytemuck::Pod>(&self, style: T) {
        *self.style_bytes.lock() = Some(bytemuck::bytes_of(&style).to_vec());
    }

    pub fn get_z_index(&self) -> u16 {
        self.z_index
    }

    pub fn get_position(&self) -> glam::Vec2 {
        self.position
    }

    pub fn get_identifier(&self) -> DrawableId {
        self.identifier
    }

    pub fn get_bbox(&self) -> BoundingBox {
        self.bounding_box.clone()
    }
}
