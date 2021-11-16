[[block]]
struct WindowSizeUniform {
    size: vec2<f32>;
    cell_size: vec2<f32>;
    column: u32;
};

[[group(0), binding(0)]] var<uniform> window_size: WindowSizeUniform;

struct VertexInput {
    [[builtin(vertex_index)]] vertex_index: u32;
    [[builtin(instance_index)]] cell_index: u32;
    [[location(0)]] color: vec4<f32>;
    [[location(1)]] bg_color: vec4<f32>;
};

struct VertexOutput {
    [[builtin(position)]] position: vec4<f32>;
    [[location(0)]] color: vec4<f32>;
};

struct Rect {
    begin: vec2<f32>;
    size: vec2<f32>;
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

fn colorful_color(vertex_index: u32) -> vec3<f32> {
    switch (vertex_index) {
        case 0: { return vec3<f32>(1.0, 0.0, 0.0); }
        case 1: { return vec3<f32>(0.0, 1.0, 0.0); }
        case 2: { return vec3<f32>(0.0, 0.0, 1.0); }
        default: { return vec3<f32>(1.0, 1.0, 1.0); }
    }
}

[[stage(vertex)]]
fn cell_vs(
    model: VertexInput,
) -> VertexOutput {
    let rect = calculate_cell_rect(model.cell_index, vec2<f32>(0.0));
    let color = vec4<f32>(colorful_color(model.vertex_index), 1.0);
    return VertexOutput(vec4<f32>(get_rect_position(rect, model.vertex_index), 1.0, 1.0), color);
}

[[stage(fragment)]]
fn simple_fs(in: VertexOutput) -> [[location(0)]] vec4<f32> {
    return in.color;
}

struct LyonInput {
    [[builtin(vertex_index)]] vertex_index: u32;
    [[location(0)]] position: vec2<f32>;
};

struct LyonOutput {
    [[builtin(position)]] position: vec4<f32>;
    [[location(0)]] color: vec4<f32>;
};

[[stage(vertex)]]
fn lyon_vs(model: LyonInput) -> VertexOutput {
    let scale = window_size.cell_size.yy * 2.0 / window_size.size;
    // let position = model.position;
    var position = model.position * scale + vec2<f32>(-1.0, 1.0 - scale.y);
    // position.x = position.x * 2.0 - 1.0;
    // position.y = position.y * 2.0;
    return VertexOutput(vec4<f32>(position, 1.0, 1.0), vec4<f32>(1.0));
}
