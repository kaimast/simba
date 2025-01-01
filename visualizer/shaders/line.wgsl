@group(0) @binding(0) var<uniform> view_proj: array<mat4x4f, 2>;
@group(0) @binding(1) var<uniform> model: mat4x4f;
@group(0) @binding(2) var<uniform> style: LineStyle;
@group(0) @binding(3) var<uniform> config: LineConfig;

struct LineStyle {
    fill_color: vec4f,
    border_color: vec4f,
    line_width: f32,
    border_width: f32,
}

struct LineConfig {
    length: f32,
    _unused: f32,
}

fn total_width(style: LineStyle) -> f32 {
    return style.border_width * 2.0 + style.line_width;
}

@fragment
fn main_fs(
    @location(1) normal: vec2f,
) -> @location(0) vec4f {
    let rel_inner_width = max(style.line_width / total_width(style), 0.0);

    if length(normal) > rel_inner_width {
        return style.border_color;
    } else {
        return style.fill_color;
    };
}

struct VertexOutput {
    @builtin(position) pos: vec4f,
    @location(1) normal: vec2f,
}

@vertex
fn main_vs(
    @location(0) position_in: vec3f,
    @location(1) normal: vec2f,
) -> VertexOutput {
    let model_view = view_proj[0] * model;

    var out_position = vec4(position_in, 1.0);
    out_position.x *= config.length;
    out_position.y *= total_width(style);

    out_position = view_proj[1] * model_view * out_position;

    return VertexOutput(out_position, normal);
}
