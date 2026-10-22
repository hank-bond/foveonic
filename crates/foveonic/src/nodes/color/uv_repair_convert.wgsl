struct Params { shape: vec4<u32>, controls: vec4<f32>, dark: vec4<f32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read> matrix: array<vec2<f32>>;
@group(0) @binding(3) var<storage, read_write> destination: array<f32>;

// High/low words retain Double coefficient and accumulation residuals until
// the native callback's final Float32 rounding. Explicit fma blocks contraction.
fn product(a: f32, b: f32) -> vec2<f32> {
    let high = fma(a, b, 0.0);
    return vec2<f32>(high, fma(a, b, -high));
}
fn add(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let sum = fma(1.0, a.x, b.x);
    let virtual_b = fma(-1.0, a.x, sum);
    let remainder_a = fma(-1.0, fma(-1.0, virtual_b, sum), a.x);
    let remainder_b = fma(-1.0, virtual_b, b.x);
    let error = fma(1.0, fma(1.0, remainder_a, remainder_b), fma(1.0, a.y, b.y));
    let high = fma(1.0, sum, error);
    return vec2<f32>(high, fma(-1.0, fma(-1.0, sum, high), error));
}
fn multiply(value: vec2<f32>, factor: f32) -> vec2<f32> {
    return add(product(value.x, factor), product(value.y, factor));
}
@compute @workgroup_size(8, 8)
fn uv_convert(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.shape.x || id.y >= params.shape.y { return; }
    let i = (id.y * params.shape.x + id.x) * 3u;
    for (var channel = 0u; channel < 3u; channel++) {
        let row = channel * 3u;
        let first = add(multiply(matrix[row + 1u], source[i + 1u]), multiply(matrix[row], source[i]));
        let result = add(first, multiply(matrix[row + 2u], source[i + 2u]));
        destination[i + channel] = fma(1.0, result.x, result.y);
    }
}
