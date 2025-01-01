use glam::{Vec2, Vec3, Vec4};

#[derive(Copy, Clone, Debug, bytemuck::Zeroable, bytemuck::Pod)]
#[repr(C)]
pub struct Vertex {
    pub pos: Vec3,
    pub tex_coord: Vec2,
}

#[derive(Copy, Clone, Debug, bytemuck::Zeroable, bytemuck::Pod)]
#[repr(C)]
pub struct Color {
    // TODO replace with glam as soon as Vec4::new is const
    // inner: Vec4,
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

#[derive(Debug, Clone)]
pub struct BoundingBox {
    pub start: Vec2,
    pub end: Vec2,
}

impl BoundingBox {
    pub fn new(start: Vec2, end: Vec2) -> Self {
        Self { start, end }
    }

    /// Does the bounding box contain a specific point?
    pub fn contains(&self, pos: &Vec2) -> bool {
        self.start.x <= pos.x && self.start.y <= pos.y && self.end.x >= pos.x && self.end.y >= pos.y
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        !(other.start.x > self.end.x
            || other.start.y > self.end.y
            || self.start.x > other.end.x
            || self.start.y > other.end.y)
    }
}

impl Color {
    pub const BLACK: Self = Self::from_rgba(0, 0, 0, 255);
    pub const WHITE: Self = Self::from_rgba(0, 0, 0, 255);

    pub const fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: (r as f32) / 255.0,
            g: (g as f32) / 255.0,
            b: (b as f32) / 255.0,
            a: (a as f32) / 255.0,
        }
    }

    pub fn into_wgpu(self) -> wgpu::Color {
        wgpu::Color {
            r: self.r as f64,
            g: self.g as f64,
            b: self.b as f64,
            a: self.a as f64,
        }
    }

    pub fn into_vec4(self) -> Vec4 {
        Vec4::new(self.r, self.g, self.b, self.a)
    }
}

impl Vertex {
    pub fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<Vec3>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}
