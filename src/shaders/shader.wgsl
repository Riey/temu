[[block]]
struct WindowSizeUniform {
    size: vec2<f32>;
    cell_size: vec2<f32>;
    texture_count: vec2<u32>;
    column: u32;
};

[[group(0), binding(0)]] var<uniform> window_size: WindowSizeUniform;
[[group(0), binding(1)]] var font_texture: texture_2d_array<f32>;
[[group(0), binding(2)]] var font_sampler: sampler;

struct VertexInput {
    [[builtin(vertex_index)]] vertex_index: u32;
    [[builtin(instance_index)]] cell_index: u32;
    [[location(0)]] color: vec4<f32>;
    [[location(1)]] bg_color: vec4<f32>;
    [[location(2)]] glyph_id: u32;
};

struct VertexOutput {
    [[builtin(position)]] position: vec4<f32>;
    [[location(0)]] tex_position: vec2<f32>;
    [[location(1)]] color: vec3<f32>;
    [[location(2)]] bg_color: vec4<f32>;
    [[location(3)]] layer: i32;
};

[[stage(vertex)]]
fn cell_vs(
    model: VertexInput,
) -> VertexOutput {
    let glyph_per_layer = window_size.texture_count.x * window_size.texture_count.y;
    let layer = i32(model.glyph_id / glyph_per_layer);
    let left = model.glyph_id % glyph_per_layer;

    let tex_row = left / window_size.texture_count.x;
    let tex_column = left % window_size.texture_count.x;

    let tex_bottom = f32(tex_row) * window_size.cell_size.y;
    let tex_top = tex_bottom + window_size.cell_size.y;
    let tex_left = f32(tex_column) * window_size.cell_size.x;
    let tex_right = tex_left + window_size.cell_size.x;

    let row: u32 = model.cell_index / window_size.column;
    let column: u32 = model.cell_index % window_size.column;

    let left = f32(column) * window_size.cell_size.x;
    let right = left + window_size.cell_size.x;
    let top = f32(row) * window_size.cell_size.y;
    let bottom = top + window_size.cell_size.y;

    let left = left / window_size.size.x * 2.0 - 1.0;
    let right = right / window_size.size.x * 2.0 - 1.0;
    let top = 1.0 - top / window_size.size.y * 2.0;
    let bottom = 1.0 - bottom / window_size.size.y * 2.0;

    var pos: vec2<f32>;
    var tex_pos: vec2<f32>;
    var color: vec3<f32> = vec3<f32>(1.0);

    switch (model.vertex_index) {
        case 0: {
            pos = vec2<f32>(left, top);
            tex_pos = vec2<f32>(tex_left, tex_top);
            color = vec3<f32>(1.0, 0.0, 0.0);
        }
        case 1: {
            pos = vec2<f32>(right, top);
            tex_pos = vec2<f32>(tex_right, tex_top);
            color = vec3<f32>(0.0, 1.0, 0.0);
        }
        case 2: {
            pos = vec2<f32>(left, bottom);
            tex_pos = vec2<f32>(tex_left, tex_bottom);
            color = vec3<f32>(0.0, 0.0, 1.0);
        }
        default: {
            pos = vec2<f32>(right, bottom);
            tex_pos = vec2<f32>(tex_right, tex_bottom);
            color = vec3<f32>(1.0, 1.0, 1.0);
        }
    }

    tex_pos = tex_pos / 1024.0;

    return VertexOutput(vec4<f32>(pos, 1.0, 1.0), tex_pos, color, model.bg_color, layer);
}

[[stage(fragment)]]
fn simple_fs(in: VertexOutput) -> [[location(0)]] vec4<f32> {
    let alpha = textureSample(font_texture, font_sampler, in.tex_position, in.layer).r;
    let color = vec4<f32>(alpha);
    return color;
}
