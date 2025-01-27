use crate::graphics::{
    BoundingBox, Drawable, DrawableId, MAX_Z_INDEX, Material, Program, Renderer, Vertex,
};

use std::sync::Arc;

use glam::{Vec2, Vec3, Vec4};

use wgpu::util::DeviceExt;
use wgpu::{BindingType, BufferBindingType, ShaderStages};

#[derive(Copy, Clone, Debug, Default, bytemuck::Zeroable, bytemuck::Pod)]
#[repr(C)]
pub struct CircleStyle {
    pub fill_color: Vec4,
    pub border_color: Vec4,
    pub radius: f32,
    pub border_width: f32,
    pub _unused: f64,
}

impl CircleStyle {
    fn get_total_radius(&self) -> f32 {
        self.radius + self.border_width
    }
}

pub(super) async fn create_material(renderer: &Renderer) -> Material {
    let device = renderer.get_device();
    let program = renderer.get_program("circle").await;

    let uniform_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    count: None,
                    visibility: ShaderStages::VERTEX,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    count: None,
                    visibility: ShaderStages::VERTEX,
                    ty: BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    count: None,
                    visibility: ShaderStages::FRAGMENT | ShaderStages::VERTEX,
                    ty: BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                },
            ],
        });

    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Circle Pipeline Layout"),
        bind_group_layouts: &[&uniform_bind_group_layout],
        push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Circle Render Pipeline"),
        layout: Some(&render_pipeline_layout),
        vertex: wgpu::VertexState {
            module: program.get_shader(),
            entry_point: Some("main_vs"),
            buffers: &[Vertex::desc()],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: program.get_shader(),
            entry_point: Some("main_fs"),
            targets: &[Some(wgpu::ColorTargetState {
                format: renderer.get_texture_format(),
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::One,
                        operation: wgpu::BlendOperation::Max,
                    },
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Cw,
            cull_mode: Some(wgpu::Face::Back),
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
        cache: None,
    });

    let vertices = vec![
        Vertex {
            pos: Vec3::new(-0.5, -0.5, 0.0),
            tex_coord: Vec2::new(-1.0, -1.0),
        },
        Vertex {
            pos: Vec3::new(-0.5, 0.5, 0.0),
            tex_coord: Vec2::new(-1.0, 1.0),
        },
        Vertex {
            pos: Vec3::new(0.5, -0.5, 0.0),
            tex_coord: Vec2::new(1.0, -1.0),
        },
        Vertex {
            pos: Vec3::new(0.5, 0.5, 0.0),
            tex_coord: Vec2::new(1.0, 1.0),
        },
    ];

    let indices: Vec<u16> = vec![0, 1, 2, 2, 1, 3];

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
        label: Some("Circle vertex buffer"),
    });

    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
        label: Some("Circle index buffer"),
    });

    Material {
        uniform_bind_group_layout,
        pipeline: render_pipeline,
        vertex_buffer,
        index_buffer,
    }
}

fn compute_bounding_box(position: &Vec2, style: &CircleStyle) -> BoundingBox {
    let radius = style.get_total_radius();
    let start = *position - Vec2::new(radius, radius);
    let end = *position + Vec2::new(radius, radius);

    BoundingBox::new(start, end)
}

pub(super) async fn new_drawable(
    identifier: DrawableId,
    position: Vec2,
    z_index: u16,
    style: CircleStyle,
    renderer: Arc<Renderer>,
    vp_buffer: Arc<wgpu::Buffer>,
) -> Drawable {
    if z_index >= MAX_Z_INDEX {
        panic!("invalid z index");
    }

    let bounding_box = compute_bounding_box(&position, &style);

    let material = renderer.get_material("circle").await;

    let device = renderer.get_device();
    let position3 = position.extend(z_index as f32);
    let translation = glam::Mat4::from_translation(position3);

    let position_buffer = Arc::new(
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::cast_slice(&translation.to_cols_array()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            label: Some("circle position"),
        }),
    );

    let style_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        contents: bytemuck::bytes_of(&style),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        label: Some("circle style"),
    });

    let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &material.uniform_bind_group_layout,
        label: None,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: vp_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: position_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: style_buffer.as_entire_binding(),
            },
        ],
    });

    Drawable {
        identifier,
        uniform_bind_group,
        style_buffer,
        renderer,
        material,
        position,
        z_index,
        bounding_box,
        style_bytes: Default::default(),
    }
}

pub(super) fn create_program(device: &wgpu::Device) -> Program {
    let shader = wgpu::ShaderModuleDescriptor {
        label: Some("Circle Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/circle.wgsl").into()),
    };
    Program::new(device, shader)
}
