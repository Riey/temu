[[block]]
struct WindowSizeUniform {
    size: vec2<f32>;
};

[[group(0), binding(0)]]
var<uniform> window_size: WindowSizeUniform;

let RADIUS: f32 = 30.0;

struct VertexInput {
    [[location(0)]] position: vec2<f32>;
    [[location(1)]] color: vec3<f32>;
};

struct VertexOutput {
    [[builtin(position)]] clip_position: vec4<f32>;
    [[location(0)]] color: vec3<f32>;
};

// Vertex shader

[[stage(vertex)]]
fn rect_vs(
    model: VertexInput,
) -> VertexOutput {
    return VertexOutput(vec4<f32>(model.position, 1.0, 1.0), model.color);
}

// Fragment shader

[[stage(fragment)]]
fn rect_fs(in: VertexOutput) -> [[location(0)]] vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}

[[stage(fragment)]]
fn rect_round_fs(in: VertexOutput) -> [[location(0)]] vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
 
