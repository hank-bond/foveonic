struct Params { shape: vec4<u32>, controls: vec4<f32> }
struct Axis { index: vec2<u32>, weight: vec2<f32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read> gain_map: array<f32>;
@group(0) @binding(3) var<storage, read> axes: array<Axis>;
@group(0) @binding(4) var<storage, read_write> destination: array<f32>;

// Two Float32 words retain the product residuals that ordinary Float32 resizing
// loses. The native operation accumulates both separable passes in Double.
// Explicit fma additions retain each rounding boundary. Ordinary additions let
// Metal contract away the residual terms, changing the resized gains.
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
fn multiply(value: vec2<f32>, weight: f32) -> vec2<f32> {
    return add(product(value.x, weight), product(value.y, weight));
}
fn sample_map(x: u32, y: u32, phase: vec2<u32>) -> f32 {
    return gain_map[(y * 2u + phase.y) * params.shape.z + x * 2u + phase.x];
}
@compute @workgroup_size(8, 8)
fn phase_gain(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = params.shape.x;
    let height = params.shape.y;
    if id.x >= width || id.y >= height { return; }
    let position = id.y * width + id.x;
    let value = source[position];
    if value >= params.controls.x || id.x >= (width / 2u) * 2u || id.y >= (height / 2u) * 2u {
        destination[position] = value;
        return;
    }
    let horizontal = axes[id.x / 2u];
    let vertical = axes[width / 2u + id.y / 2u];
    let phase = id.xy % vec2<u32>(2u);
    let left = add(product(sample_map(horizontal.index.x, vertical.index.x, phase), vertical.weight.x), product(sample_map(horizontal.index.x, vertical.index.y, phase), vertical.weight.y));
    let right = add(product(sample_map(horizontal.index.y, vertical.index.x, phase), vertical.weight.x), product(sample_map(horizontal.index.y, vertical.index.y, phase), vertical.weight.y));
    let result = add(multiply(left, horizontal.weight.x), multiply(right, horizontal.weight.y));
    let gain = fma(1.0, result.x, result.y);
    destination[position] = value * gain;
}
