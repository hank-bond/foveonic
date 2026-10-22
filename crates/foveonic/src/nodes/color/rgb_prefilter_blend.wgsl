struct Params { shape: vec4<u32>, controls: vec4<f32>, gains: vec4<f32> }
@group(0) @binding(0) var<storage, read> original: array<f32>;
@group(0) @binding(1) var<storage, read> filtered: array<f32>;
@group(0) @binding(2) var<storage, read> guide: array<i32>;
@group(0) @binding(3) var<storage, read> mask: array<u32>;
@group(0) @binding(4) var<uniform> params: Params;
@group(0) @binding(5) var<storage, read_write> destination: array<f32>;
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
fn rgb_prefilter_blend(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.shape.x || id.y >= params.shape.y { return; }
    let pixel = id.y * params.shape.x + id.x;
    var factor = 0.0;
    if mask[pixel] == 2u { factor = 1.0; }
    else if mask[pixel] == 1u {
        let level = f32(guide[pixel]);
        let upper = fma(1.0, params.controls.y, params.controls.z);
        if level <= params.controls.y { factor = 1.0; }
        else if level < upper {
            let numerator = fma(-1.0, level, upper);
            let quotient = numerator / params.controls.z;
            factor = fma(1.0, quotient, fma(-quotient, params.controls.z, numerator) / params.controls.z);
        }
    }
    let complement = add(vec2<f32>(1.0, 0.0), vec2<f32>(-factor, 0.0));
    for (var ch = 0u; ch < 3u; ch++) {
        let i = pixel * 3u + ch;
        let term = multiply(complement, filtered[i]);
        let narrowed = fma(1.0, term.x, term.y);
        destination[i] = fma(1.0, fma(original[i], factor, 0.0), narrowed);
    }
}
