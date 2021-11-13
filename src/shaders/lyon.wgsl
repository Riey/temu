struct VertexInput {
    [[builtin(vertex_index)]] vertex_index: u32;
    [[location(0)]] position: vec2<f32>;
};

struct VertexOutput {
    [[builtin(position)]] position: vec4<f32>;
    [[location(0)]] color: vec4<f32>;
};

[[stage(vertex)]]
fn lyon_vs(model: VertexInput) -> VertexOutput {
    return VertexOutput(vec4<f32>(model.position, 1.0, 1.0), vec4<f32>(1.0));
}

[[stage(fragment)]]
fn lyon_fs(in: VertexOutput) -> [[location(0)]] vec4<f32> {
    return in.color;
}
