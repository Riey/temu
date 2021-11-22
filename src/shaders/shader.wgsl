[[block]]
struct WindowSizeUniform {
    size: vec2<f32>;
    cell_size: vec2<f32>;
    column: u32;
};

[[block]]
struct UiUniform {
    cursor_color: vec4<f32>;
    cursor_pos: vec2<f32>;
    // px
    scrollbar_width: f32;
    // ndc
    scrollbar_height: f32;
    scrollbar_fg: vec4<f32>;
    scrollbar_bg: vec4<f32>;
    // ndc
    scrollbar_top: f32;
    pad: vec3<f32>;
};

[[group(0), binding(0)]] var<uniform> window_size: WindowSizeUniform;
[[group(0), binding(1)]] var<uniform> ui: UiUniform;
[[group(0), binding(5)]] var font_texture: texture_2d_array<f32>;
[[group(0), binding(6)]] var font_sampler: sampler;

let TEXTURE_WIDTH: f32 = 1024.0;

struct CellInput {
    [[builtin(vertex_index)]] vertex_index: u32;
    [[location(0)]] color: vec4<f32>;
    [[location(1)]] cell_pos: vec2<f32>;
    [[location(2)]] pad: vec2<f32>;
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

fn pixel_x_to_ndc(x: f32) -> f32 {
    let norm = x * 2.0 / window_size.size.x;
    return norm - 1.0;
}

fn pixel_size_to_ndc(size: vec2<f32>) -> vec2<f32> {
    let size = size * 2.0 / window_size.size;
    return vec2<f32>(size.x, -size.y);
}

fn pixel_width_to_ndc(width: f32) -> f32 {
    let width = width * 2.0 / window_size.size.x;
    return width;
}

fn calculate_cell_rect(cell_pos: vec2<f32>) -> Rect {
    let begin = (cell_pos * window_size.cell_size);

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
    let rect = calculate_cell_rect(model.cell_pos);
    // let color = colorful_color(model.vertex_index);
    let color = model.color;
    return CellOutput(vec4<f32>(get_rect_position(rect, model.vertex_index), 1.0, 1.0), color);
}

[[stage(fragment)]]
fn cell_fs(in: CellOutput) -> [[location(0)]] vec4<f32> {
    return in.color;
}

fn calculate_text_pos(line_no: f32, position: vec2<f32>) -> vec2<f32> {
    let pixel_pos = vec2<f32>(0.0, (line_no + 1.0) * window_size.cell_size.y) + vec2<f32>(position.x, -position.y);
    return pixel_to_ndc(pixel_pos);
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
    // let color = colorful_color(model.vertex_index).rgb;
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

[[stage(vertex)]]
fn ui_vs(
    [[builtin(vertex_index)]] vertex_index: u32,
    [[builtin(instance_index)]] ui_index: u32,
) -> CellOutput {
    switch (ui_index) {
        // cursor
        case 0: {
            let rect = Rect(pixel_to_ndc(ui.cursor_pos * window_size.cell_size), pixel_size_to_ndc(window_size.cell_size));
            let pos = get_rect_position(rect, vertex_index);

            return CellOutput(vec4<f32>(pos, 1.0, 1.0), ui.cursor_color);
        }
        // scrollbar outer
        case 1: {
            let rect = Rect(
                pixel_to_ndc(vec2<f32>(window_size.size.x - ui.scrollbar_width, 0.0)),
                pixel_size_to_ndc(vec2<f32>(ui.scrollbar_width, window_size.size.y))
            );
            let pos = get_rect_position(rect, vertex_index);

            return CellOutput(vec4<f32>(pos, 1.0, 1.0), ui.scrollbar_bg);
            // return CellOutput(vec4<f32>(pos, 1.0, 1.0), vec4<f32>(1.0));
        }
        // scrollbar inner
        case 2: {
            let rect = Rect(
                vec2<f32>(pixel_x_to_ndc(window_size.size.x - ui.scrollbar_width), ui.scrollbar_top),
                vec2<f32>(pixel_width_to_ndc(ui.scrollbar_width), ui.scrollbar_height)
            );
            let pos = get_rect_position(rect, vertex_index);

            return CellOutput(vec4<f32>(pos, 1.0, 1.0), ui.scrollbar_fg);
            // return CellOutput(vec4<f32>(pos, 1.0, 1.0), vec4<f32>(1.0, 0.0, 0.0, 1.0));
        }
        default: {
            // Unknown
            return CellOutput(vec4<f32>(0.0), vec4<f32>(0.0));
        }
    }
}
