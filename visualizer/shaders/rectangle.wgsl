@group(0) @binding(0) var<uniform> view_proj: array<mat4x4f, 2>;
@group(0) @binding(1) var<uniform> model: mat4x4f;
@group(0) @binding(2) var<uniform> style: RectangleStyle;

struct RectangleStyle {
    fill_color: vec4f,
    border_color: vec4f,
    width: f32,
    height: f32,
    border_width: f32,
}

@fragment
fn main_fs(
    @location(1) tex_coord: vec2f,
) -> @location(0) vec4f {
    let normalized_w_border = max(0.0, style.border_width / (style.width*0.5 + style.border_width));
    let normalized_h_border = max(0.0, style.border_width / (style.height*0.5 + style.border_width));

    if tex_coord.x > (1.0 - normalized_w_border) || tex_coord.x < (-1.0 + normalized_w_border)
            || tex_coord.y  > (1.0 - normalized_h_border) || tex_coord.y < (-1.0 + normalized_h_border) {
        return style.border_color;
    } else {
        return style.fill_color;
    }
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
    var position_out = vec4f(
        position_in.x * (0.5 * style.width + style.border_width),
        position_in.y * (0.5 * style.height + style.border_width),
        position_in.z,
        1.0,
    );

    position_out = view_proj[1] * model_view * position_out;

    return VertexOutput(position_out, tex_coord);
}
