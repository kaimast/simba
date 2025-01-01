use super::{BoundingBox, Renderer, MAX_Z_INDEX};

use std::sync::Arc;

use parking_lot::{Mutex, MutexGuard};

use glam::{Mat4, Vec2, Vec3};

use wgpu::util::DeviceExt;
use winit::dpi::{LogicalPosition, LogicalSize};

use enum_map::{Enum, EnumMap};

struct Configuration {
    dirty: bool,
    position: Vec3,
    /// The view size in scene coordinates (not screen pixels)
    view_size: Vec2,
    zoom: f32,
    max_pos: Vec2,
    min_pos: Vec2,
}

#[derive(Default)]
struct Movement {
    velocity: Vec2,
    key_states: EnumMap<InputDirection, bool>,
}

pub struct Camera {
    renderer: Arc<Renderer>,
    configuration: Mutex<Configuration>,
    vp_buffer: Arc<wgpu::Buffer>,
    movement: Mutex<Movement>,
}

#[derive(Enum, Clone, Copy, Debug)]
pub enum InputDirection {
    Up,
    Down,
    Left,
    Right,
}

impl Camera {
    const SPEED: f32 = 750.0;

    pub(super) async fn new(
        renderer: Arc<Renderer>,
        vp_buffer: Arc<wgpu::Buffer>,
        min_pos: Vec2,
        max_pos: Vec2,
    ) -> Self {
        let logical_size: LogicalSize<f32> = {
            let geometry = renderer.get_geometry();
            geometry.window_size.to_logical(geometry.scale_factor)
        };

        let zoom = 10.0;

        let position = Vec3::new(0.0, 0.0, 0.0);
        let view_size = Vec2::new(logical_size.width, logical_size.height) / zoom;

        let configuration = Configuration {
            dirty: true,
            position,
            view_size,
            zoom,
            max_pos,
            min_pos,
        };

        Self {
            renderer,
            configuration: Mutex::new(configuration),
            vp_buffer,
            movement: Mutex::new(Default::default()),
        }
    }

    pub fn get_vp_buffer(&self) -> Arc<wgpu::Buffer> {
        self.vp_buffer.clone()
    }

    pub fn look_at(&self, new_pos: Vec2) {
        let mut config = self.configuration.lock();
        config.position = Vec3::new(new_pos[0], new_pos[1], 0.0);
        config.dirty = true;
    }

    pub fn get_position_from_cursor(&self, cursor_pos: LogicalPosition<f64>) -> Vec2 {
        let config = self.configuration.lock();

        // Scale position by zoom
        let cursor_pos = Vec2::new(cursor_pos.x as f32, cursor_pos.y as f32) / config.zoom;

        // Offset from the center
        let offset = cursor_pos - 0.5 * config.view_size;

        // We need to flip the y axis here to convert from window to opengl coordinates
        let offset = Vec2::new(offset.x, -offset.y);

        Vec2::new(config.position.x, config.position.y) + offset
    }

    pub fn change_zoom_by(&self, delta: f32) {
        let geometry = self.renderer.get_geometry();
        let logical_size: LogicalSize<f32> = geometry.window_size.to_logical(geometry.scale_factor);

        let mut config = self.configuration.lock();
        let zoom = (config.zoom - delta).clamp(1.0, 50.0);

        let view_size = Vec2::new(logical_size.width, logical_size.height) / zoom;

        config.view_size = view_size;
        config.zoom = zoom;
        config.dirty = true;
    }

    //TODO remove
    pub fn set_zoom(&self, zoom: f32) {
        let geometry = self.renderer.get_geometry();
        let logical_size: LogicalSize<f32> = geometry.window_size.to_logical(geometry.scale_factor);

        let view_size = Vec2::new(logical_size.width, logical_size.height) / zoom;

        let mut config = self.configuration.lock();
        config.view_size = view_size;
        config.zoom = zoom;
        config.dirty = true;
    }

    pub fn notify_resize(&self) {
        let geometry = self.renderer.get_geometry();
        let logical_size: LogicalSize<f32> = geometry.window_size.to_logical(geometry.scale_factor);

        let mut config = self.configuration.lock();
        let view_size = Vec2::new(logical_size.width, logical_size.height) / config.zoom;

        log::debug!("New view size is {}", config.view_size);
        config.view_size = view_size;
        config.dirty = true;
    }

    fn update_vp_buffer(&self, config: MutexGuard<'_, Configuration>) -> wgpu::CommandBuffer {
        log::trace!("Updating ViewProjection Buffer");

        let start = -0.5 * config.view_size;
        let end = 0.5 * config.view_size;

        let view_matrix = Mat4::from_translation(-config.position);
        let proj_matrix =
            Mat4::orthographic_rh(start.x, end.x, start.y, end.y, 0.0, -(MAX_Z_INDEX as f32));

        let view_bytes = view_matrix.to_cols_array();
        let proj_bytes = proj_matrix.to_cols_array();

        let (staging_buffer, mut encoder) = {
            let device = self.renderer.get_device();
            let staging_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                contents: [
                    bytemuck::cast_slice(&view_bytes),
                    bytemuck::cast_slice(&proj_bytes),
                ]
                .concat()
                .as_slice(),
                usage: wgpu::BufferUsages::COPY_SRC,
                label: Some("Camera"),
            });

            let encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

            (staging_buffer, encoder)
        };

        let matrix_size = std::mem::size_of::<Mat4>() as wgpu::BufferAddress;
        encoder.copy_buffer_to_buffer(&staging_buffer, 0, &self.vp_buffer, 0, 2 * matrix_size);

        encoder.finish()
    }

    pub async fn update(&self, elapsed: f64) -> Option<wgpu::CommandBuffer> {
        let velocity = self.movement.lock().velocity;
        let mut config = self.configuration.lock();

        if velocity != Vec2::new(0.0, 0.0) {
            let max_pos = config.max_pos;
            let min_pos = config.min_pos;
            let pos = &mut config.position;

            let old_pos = *pos;

            *pos += (elapsed as f32) * velocity.extend(0.0);
            pos.x = pos.x.clamp(min_pos.x, max_pos.x);
            pos.y = pos.y.clamp(min_pos.y, max_pos.y);

            if *pos != old_pos {
                log::trace!("Camera moved to {}", config.position);
                config.dirty = true;
            }
        }

        if config.dirty {
            config.dirty = false;
            Some(self.update_vp_buffer(config))
        } else {
            None
        }
    }

    pub fn notify_button_pressed(&self, direction: InputDirection) {
        let mut movement = self.movement.lock();
        let zoom = self.get_zoom();

        if !movement.key_states[direction] {
            movement.key_states[direction] = true;
            movement.update(zoom);
        }
    }

    pub fn notify_button_released(&self, direction: InputDirection) {
        let mut movement = self.movement.lock();
        let zoom = self.get_zoom();

        if movement.key_states[direction] {
            movement.key_states[direction] = false;
            movement.update(zoom);
        }
    }

    pub fn get_zoom(&self) -> f32 {
        let config = self.configuration.lock();
        config.zoom
    }

    pub fn get_view_bbox(&self) -> BoundingBox {
        let config = self.configuration.lock();
        let pos = config.position.truncate();

        let start = pos - 0.5 * config.view_size;
        let end = pos + 0.5 * config.view_size;

        BoundingBox::new(start, end)
    }

    pub fn resume(&self) {
        let config = self.configuration.lock();
        self.update_vp_buffer(config);
    }

    pub fn suspend(&self) {
        let mut movement = self.movement.lock();
        movement.stop();
    }

    pub fn set_min_max_pos(&self, min_pos: Vec2, max_pos: Vec2) {
        let mut config = self.configuration.lock();
        config.min_pos = min_pos;
        config.max_pos = max_pos;
    }
}

impl Movement {
    fn stop(&mut self) {
        for (_, val) in self.key_states.iter_mut() {
            *val = false;
        }
    }

    fn update(&mut self, zoom: f32) {
        let key_states = self.key_states;

        let y = if key_states[InputDirection::Up] && key_states[InputDirection::Down] {
            0
        } else if key_states[InputDirection::Up] {
            1
        } else if key_states[InputDirection::Down] {
            -1
        } else {
            0
        };

        let x = if key_states[InputDirection::Left] && key_states[InputDirection::Right] {
            0
        } else if key_states[InputDirection::Left] {
            -1
        } else if key_states[InputDirection::Right] {
            1
        } else {
            0
        };

        let speed = Camera::SPEED / zoom;

        self.velocity = if x == 0 && y == 0 {
            Vec2::new(0.0, 0.0)
        } else {
            let diagonal = (1.0 / (2.0_f32).sqrt()) * speed;

            if x == 0 || y == 0 {
                Vec2::new((x as f32) * speed, (y as f32) * speed)
            } else if x == -1 && y == 1 {
                Vec2::new(-diagonal, diagonal)
            } else if x == 1 && y == 1 {
                Vec2::new(diagonal, diagonal)
            } else if x == -1 && y == -1 {
                Vec2::new(-diagonal, -diagonal)
            } else if x == 1 && y == -1 {
                Vec2::new(diagonal, -diagonal)
            } else {
                panic!("Invalid state");
            }
        };

        log::trace!("Camera velocity set to {}", self.velocity);
    }
}
