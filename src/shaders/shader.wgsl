[[block]]
struct WindowSizeUniform {
    size: vec2<f32>;
    cell_size: vec2<f32>;
    column: u32;
};

[[group(0), binding(0)]] var<uniform> window_size: WindowSizeUniform;

struct VertexInput {
    [[builtin(vertex_index)]] vertex_index: u32;
    [[location(0)]] cell_index: u32;
    [[location(1)]] color: vec4<f32>;
    [[location(2)]] bg_color: vec4<f32>;
};

struct VertexOutput {
    [[builtin(position)]] position: vec4<f32>;
    [[location(0)]] color: vec4<f32>;
};

[[stage(vertex)]]
fn cell_vs(
    model: VertexInput,
) -> VertexOutput {
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
    var color: vec4<f32>;

    switch (model.vertex_index) {
        case 0: {
            pos = vec2<f32>(left, top);
            color = vec4<f32>(1.0, 0.0, 0.0, 1.0);
        }
        case 1: {
            pos = vec2<f32>(right, top);
            color = vec4<f32>(0.0, 1.0, 0.0, 1.0);
        }
        case 2: {
            pos = vec2<f32>(left, bottom);
            color = vec4<f32>(0.0, 0.0, 1.0, 1.0);
        }
        default: {
            pos = vec2<f32>(right, bottom);
            color = vec4<f32>(1.0, 1.0, 1.0, 1.0);
        }
    }

    return VertexOutput(vec4<f32>(pos, 1.0, 1.0), color);
}

[[stage(fragment)]]
fn cell_fs(in: VertexOutput) -> [[location(0)]] vec4<f32> {
    return in.color;
}
