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

let TEXTURE_WIDTH: f32 = 2048.0;

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
    // [[builtin(instance_index)]] cell_index: u32;
    [[location(0)]] base_cell_index: u32;
    [[location(1)]] offset: vec2<f32>;
    [[location(2)]] tex_size: vec2<f32>;
    [[location(3)]] color: vec3<f32>;
    [[location(4)]] glyph_id: u32;
};

struct TextOutput {
    [[builtin(position)]] position: vec4<f32>;
    [[location(0)]] tex_position: vec2<f32>;
    [[location(1)]] color: vec3<f32>;
    [[location(2)]] layer: i32;
};

struct Rect {
    begin: vec2<f32>;
    size: vec2<f32>;
};

struct TexRect {
    rect: Rect;
    layer: i32;
};

fn calculate_cell_rect(cell_index: u32, offset: vec2<f32>) -> Rect {
    let row: u32 = cell_index / window_size.column;
    let column: u32 = cell_index % window_size.column;

    let begin = (vec2<f32>(f32(column), f32(row)) * window_size.cell_size + offset) * 2.0 / window_size.size;
    let begin = vec2<f32>(begin.x - 1.0, 1.0 - begin.y);
    var size = window_size.cell_size * 2.0 / window_size.size;
    size.y = -size.y;

    return Rect(begin, size);
}

fn calculate_tex_rect(glyph_id: u32, tex_size: vec2<f32>) -> TexRect {
    let glyph_per_layer = window_size.texture_count.x * window_size.texture_count.y;
    let layer = i32(glyph_id / glyph_per_layer);
    let left = glyph_id % glyph_per_layer;

    let row = left / window_size.texture_count.x;
    let column = left % window_size.texture_count.x;

    let begin = vec2<f32>(f32(column), f32(row)) * window_size.cell_size / TEXTURE_WIDTH;
    let size = tex_size / TEXTURE_WIDTH;

    return TexRect(Rect(begin, size), layer);
}

fn get_rect_position(rect: Rect, vertex_index: u32) -> vec2<f32> {
    var ret = rect.begin;

    switch (vertex_index) {
        case 0: {
        }
        case 1: {
            ret.x = ret.x + rect.size.x;
        }
        case 2: {
            ret.y = ret.y + rect.size.y;
        }
        default: {
            ret = ret + rect.size;
        }
    }

    return ret;
}

[[stage(vertex)]]
fn cell_vs(
    model: CellInput,
) -> CellOutput {
    let rect = calculate_cell_rect(model.cell_index, vec2<f32>(0.0));
    return CellOutput(vec4<f32>(get_rect_position(rect, model.vertex_index), 1.0, 1.0), model.color);
}

[[stage(fragment)]]
fn cell_fs(in: CellOutput) -> [[location(0)]] vec4<f32> {
    return in.color;
}

fn colorful_color(vertex_index: u32) -> vec3<f32> {
    switch (vertex_index) {
        case 0: { return vec3<f32>(1.0, 0.0, 0.0); }
        case 1: { return vec3<f32>(0.0, 1.0, 0.0); }
        case 2: { return vec3<f32>(0.0, 0.0, 1.0); }
        default: { return vec3<f32>(1.0, 1.0, 1.0); }
    }
}

[[stage(vertex)]]
fn text_vs(
    model: TextInput,
) -> TextOutput {
    var rect = calculate_cell_rect(model.base_cell_index, model.offset);
    rect.size = model.tex_size / window_size.size;
    rect.size.y = -rect.size.y;

    let tex_rect = calculate_tex_rect(model.glyph_id, model.tex_size);
    let pos = get_rect_position(rect, model.vertex_index);
    let tex_pos = get_rect_position(tex_rect.rect, model.vertex_index);
    let color = model.color;
    // let color = colorful_color(model.vertex_index);
    return TextOutput(vec4<f32>(pos, 1.0, 1.0), tex_pos, color, tex_rect.layer);
}

[[stage(fragment)]]
fn text_fs(in: TextOutput) -> [[location(0)]] vec4<f32> {
    // return vec4<f32>(in.color, 1.0);
    let alpha = textureSample(font_texture, font_sampler, in.tex_position, in.layer).r;
    // if (alpha < 0.02) {
    //     discard;
    // }
    let color = vec4<f32>(in.color, alpha);
    return color;
}
