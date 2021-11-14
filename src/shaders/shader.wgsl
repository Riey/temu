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

struct CellInput {
    [[builtin(vertex_index)]] vertex_index: u32;
    [[builtin(instance_index)]] cell_index: u32;
    [[location(0)]] color: vec4<f32>;
};

struct CellOutput {
    [[builtin(position)]] position: vec4<f32>;
    [[location(0)]] color: vec4<f32>;
};

struct TextInput {
    [[builtin(vertex_index)]] vertex_index: u32;
    [[builtin(instance_index)]] cell_index: u32;
    [[location(0)]] offset: vec2<f32>;
    [[location(1)]] tex_size: vec2<f32>;
    [[location(2)]] color: vec3<f32>;
    [[location(3)]] glyph_id: u32;
};

struct TextOutput {
    [[builtin(position)]] position: vec4<f32>;
    [[location(0)]] tex_position: vec2<f32>;
    [[location(1)]] color: vec3<f32>;
    [[location(2)]] layer: i32;
};

struct CellRect {
    left: f32;
    right: f32;
    top: f32;
    bottom: f32;
};

struct TexRect {
    rect: CellRect;
    layer: i32;
};

fn calculate_cell_rect(cell_index: u32) -> CellRect {
    let row: u32 = cell_index / window_size.column;
    let column: u32 = cell_index % window_size.column;

    let left = f32(column) * window_size.cell_size.x;
    let right = left + window_size.cell_size.x;
    let top = f32(row) * window_size.cell_size.y;
    let bottom = top + window_size.cell_size.y;

    let left = left / window_size.size.x * 2.0 - 1.0;
    let right = right / window_size.size.x * 2.0 - 1.0;
    let top = 1.0 - top / window_size.size.y * 2.0;
    let bottom = 1.0 - bottom / window_size.size.y * 2.0;

    return CellRect(left, right, top, bottom);
}

fn calculate_tex_rect(glyph_id: u32, tex_size: vec2<f32>) -> TexRect {
    let glyph_per_layer = window_size.texture_count.x * window_size.texture_count.y;
    let layer = i32(glyph_id / glyph_per_layer);
    let left = glyph_id % glyph_per_layer;

    let tex_row = left / window_size.texture_count.x;
    let tex_column = left % window_size.texture_count.x;

    let tex_top = f32(tex_row) * window_size.cell_size.y / 1024.0;
    let tex_left = f32(tex_column) * window_size.cell_size.x / 1024.0;

    let tex_size = tex_size / 1024.0;

    let tex_bottom = tex_top + tex_size.y;
    let tex_right = tex_left + tex_size.x;

    return TexRect(CellRect(tex_left, tex_right, tex_top, tex_bottom), layer);
}

fn get_rect_position(rect: CellRect, vertex_index: u32) -> vec2<f32> {
    switch (vertex_index) {
        case 0: {
            return vec2<f32>(rect.left, rect.top);
        }
        case 1: {
            return vec2<f32>(rect.right, rect.top);
        }
        case 2: {
            return vec2<f32>(rect.left, rect.bottom);
        }
        default: {
            return vec2<f32>(rect.right, rect.bottom);
        }
    }
}

[[stage(vertex)]]
fn cell_vs(
    model: CellInput,
) -> CellOutput {
    let rect = calculate_cell_rect(model.cell_index);
    return CellOutput(vec4<f32>(get_rect_position(rect, model.vertex_index), 1.0, 1.0), model.color);
}

[[stage(fragment)]]
fn cell_fs(in: CellOutput) -> [[location(0)]] vec4<f32> {
    return in.color;
}

[[stage(vertex)]]
fn text_vs(
    model: TextInput,
) -> TextOutput {
    let rect = calculate_cell_rect(model.cell_index);
    let tex_rect = calculate_tex_rect(model.glyph_id, model.tex_size);
    let pos = get_rect_position(rect, model.vertex_index) + model.offset / window_size.size;
    let tex_pos = get_rect_position(tex_rect.rect, model.vertex_index);
    return TextOutput(vec4<f32>(pos, 1.0, 1.0), tex_pos, model.color, 0);
}

[[stage(fragment)]]
fn text_fs(in: TextOutput) -> [[location(0)]] vec4<f32> {
    // return vec4<f32>(in.color, 1.0);
    let alpha = textureSample(font_texture, font_sampler, in.tex_position, in.layer).r;
    if (alpha < 0.02) {
        discard;
    }
    let color = vec4<f32>(in.color * alpha, alpha);
    return color;
}
