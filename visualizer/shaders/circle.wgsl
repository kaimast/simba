@group(0) @binding(0) var<uniform> view_proj: array<mat4x4f, 2>;
@group(0) @binding(1) var<uniform> model: mat4x4f;
@group(0) @binding(2) var<uniform> style: CircleStyle;

struct CircleStyle {
    fill_color: vec4f,
    border_color: vec4f,
    radius: f32,
    border_width: f32,
}

fn total_radius(style: CircleStyle) -> f32 {
    return max(0.0, style.radius + style.border_width);
}

@fragment
fn main_fs(
    @location(1) tex_coord: vec2f,
) -> @location(0) vec4f {
    // How far away is this pixel form the center?
    let dist = length(tex_coord);

    if dist <= 1.0 {
        var normalized_border = max(0.0, (style.border_width / total_radius(style)));

        if dist > (1.0 - normalized_border) {
            return style.border_color;
        } else {
            return style.fill_color;
        }
    } else {
        return vec4f(0.0,0.0,0.0,0.0);
    };
}

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(1) tex_coord: vec2f,
}

@vertex
fn main_vs(
    @location(0) position_in: vec3f,
    @location(1) tex_coord: vec2f,
) -> VertexOutput {
    let model_view = view_proj[0] * model;
    let position_out = view_proj[1]
        * model_view
        * vec4(position_in * total_radius(style), 1.0);

    return VertexOutput(position_out, tex_coord);
}
