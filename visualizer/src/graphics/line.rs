use super::{BoundingBox, Drawable, DrawableId, Material, Program, Renderer, Vertex, MAX_Z_INDEX};

use glam::{Mat4, Vec2, Vec3, Vec4};

use std::sync::Arc;

use wgpu::util::DeviceExt;
use wgpu::{
    BindingType, Buffer, BufferBindingType, BufferUsages, RenderPipelineDescriptor, ShaderStages,
};

#[derive(Default, Copy, Clone, Debug, bytemuck::Zeroable, bytemuck::Pod)]
#[repr(C)]
pub struct LineStyle {
    pub fill_color: Vec4,
    pub border_color: Vec4,
    pub line_width: f32,
    pub border_width: f32,
    pub _unused: f64,
}

#[derive(Default, Copy, Clone, Debug, bytemuck::Zeroable, bytemuck::Pod)]
#[repr(C)]
pub struct LineConfig {
    pub length: f32,
    pub _unused: f32,
}

pub(super) async fn create_material(renderer: &Renderer) -> Material {
    let device = renderer.get_device();
    let program = renderer.get_program("line").await;

    let uniform_bind_group_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    count: None,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    count: None,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    count: None,
                    visibility: ShaderStages::FRAGMENT | ShaderStages::VERTEX,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    count: None,
                    visibility: ShaderStages::VERTEX,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                },
            ],
        });

    let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Line Pipeline Layout"),
        bind_group_layouts: &[&uniform_bind_group_layout],
        push_constant_ranges: &[],
    });

    let render_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("Line Render Pipeline"),
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
                blend: None,
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
            tex_coord: Vec2::new(0.0, -1.0),
        },
        Vertex {
            pos: Vec3::new(-0.5, 0.5, 0.0),
            tex_coord: Vec2::new(0.0, 1.0),
        },
        Vertex {
            pos: Vec3::new(0.5, -0.5, 0.0),
            tex_coord: Vec2::new(0.0, -1.0),
        },
        Vertex {
            pos: Vec3::new(0.5, 0.5, 0.0),
            tex_coord: Vec2::new(0.0, 1.0),
        },
    ];

    let indices: Vec<u16> = vec![0, 1, 2, 2, 1, 3];

    let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        contents: bytemuck::cast_slice(&vertices),
        usage: wgpu::BufferUsages::VERTEX,
        label: Some("Line vertex buffer"),
    });

    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
        label: Some("Line index buffer"),
    });

    Material {
        uniform_bind_group_layout,
        pipeline: render_pipeline,
        vertex_buffer,
        index_buffer,
    }
}

fn compute_bounding_box(start: &Vec2, end: &Vec2) -> BoundingBox {
    let x1 = start.x.min(end.x);
    let y1 = start.y.min(end.y);
    let x2 = start.x.max(end.x);
    let y2 = start.y.max(end.y);

    BoundingBox::new(Vec2::new(x1, y1), Vec2::new(x2, y2))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn new_drawable(
    identifier: DrawableId,
    start: glam::Vec2,
    end: Vec2,
    z_index: u16,
    style: LineStyle,
    renderer: Arc<Renderer>,
    vp_buffer: Arc<Buffer>,
) -> Drawable {
    let bounding_box = compute_bounding_box(&start, &end);

    let direction = end - start;
    let position = start + direction * 0.5;

    let length = direction.length();
    let config = LineConfig {
        length,
        _unused: 0.0,
    };

    let rotation = direction.angle_to(Vec2::new(1.0, 0.0));

    if z_index >= MAX_Z_INDEX {
        panic!("invalid z index");
    }

    let material = renderer.get_material("line").await;

    let device = renderer.get_device();
    let position3 = position.extend(z_index as f32);
    let translation = Mat4::from_translation(position3) * Mat4::from_rotation_z(-rotation);

    let position_buffer = Arc::new(
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            contents: bytemuck::cast_slice(&translation.to_cols_array()),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            label: Some("Line position"),
        }),
    );

    let style_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        contents: bytemuck::bytes_of(&style),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        label: Some("Line style"),
    });

    let config_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        contents: bytemuck::bytes_of(&config),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        label: Some("Line config"),
    });
    let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        layout: &material.uniform_bind_group_layout,
        label: Some("Line bind group"),
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
            wgpu::BindGroupEntry {
                binding: 3,
                resource: config_buffer.as_entire_binding(),
            },
        ],
    });

    Drawable {
        identifier,
        uniform_bind_group,
        style_buffer,
        material,
        renderer,
        position,
        z_index,
        bounding_box,
        style_bytes: Default::default(),
    }
}

pub(super) fn create_program(device: &wgpu::Device) -> Program {
    let shader = wgpu::ShaderModuleDescriptor {
        label: Some("Line Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/line.wgsl").into()),
    };
    Program::new(device, shader)
}
