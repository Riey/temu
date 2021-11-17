[[block]]
struct WindowSizeUniform {
    size: vec2<f32>;
    cell_size: vec2<f32>;
    column: u32;
};

[[group(0), binding(0)]] var<uniform> window_size: WindowSizeUniform;
[[group(0), binding(1)]] var font_texture: texture_2d_array<f32>;
[[group(0), binding(2)]] var font_sampler: sampler;

let TEXTURE_WIDTH: f32 = 1024.0;

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
    [[location(0)]] position: vec2<f32>;
    [[location(1)]] tex_position: vec2<f32>;
    [[location(2)]] tex_size: vec2<f32>;
    [[location(3)]] color: vec3<f32>;
    [[location(4)]] layer: i32;
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

fn pixel_to_ndc(px: vec2<f32>) -> vec2<f32> {
    let norm = px * 2.0 / window_size.size;
    return vec2<f32>(norm.x - 1.0, 1.0 - norm.y);
}

fn pixel_size_to_ndc(size: vec2<f32>) -> vec2<f32> {
    let size = size * 2.0 / window_size.size;
    return vec2<f32>(size.x, -size.y);
}

fn calculate_cell_rect(cell_index: u32) -> Rect {
    let row: u32 = cell_index / window_size.column;
    let column: u32 = cell_index % window_size.column;

    let begin = (vec2<f32>(f32(column), f32(row)) * window_size.cell_size);

    return Rect(pixel_to_ndc(begin), pixel_size_to_ndc(window_size.cell_size));
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

fn colorful_color(vertex_index: u32) -> vec4<f32> {
    switch (vertex_index) {
        case 0: { return vec4<f32>(1.0, 0.0, 0.0, 1.0); }
        case 1: { return vec4<f32>(0.0, 1.0, 0.0, 1.0); }
        case 2: { return vec4<f32>(0.0, 0.0, 1.0, 1.0); }
        default: { return vec4<f32>(1.0, 1.0, 1.0, 1.0); }
    }
}

[[stage(vertex)]]
fn cell_vs(
    model: CellInput,
) -> CellOutput {
    let rect = calculate_cell_rect(model.cell_index);
    // let color = colorful_color(model.vertex_index);
    let color = model.color;
    return CellOutput(vec4<f32>(get_rect_position(rect, model.vertex_index), 1.0, 1.0), color);
}

[[stage(fragment)]]
fn cell_fs(in: CellOutput) -> [[location(0)]] vec4<f32> {
    return in.color;
}

[[stage(vertex)]]
fn text_vs(
    model: TextInput,
) -> TextOutput {
    let rect = Rect(pixel_to_ndc(model.position), pixel_size_to_ndc(model.tex_size));

    let tex_rect = Rect(model.tex_position / TEXTURE_WIDTH, model.tex_size / TEXTURE_WIDTH);
    let pos = get_rect_position(rect, model.vertex_index);
    let tex_pos = get_rect_position(tex_rect, model.vertex_index);
    let color = model.color;
    // let color = colorful_color(model.vertex_index);
    return TextOutput(vec4<f32>(pos, 1.0, 1.0), tex_pos, color, model.layer);
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
