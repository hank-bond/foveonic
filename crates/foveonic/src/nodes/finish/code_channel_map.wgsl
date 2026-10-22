struct Params { shape: vec4<u32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage, read> weights: array<vec2<f32>>;
@group(0) @binding(3) var<storage, read_write> output: array<f32>;

fn product(a: f32, b: f32) -> vec2<f32> {
    let high = fma(a, b, 0.);
    return vec2(high, fma(a, b, -high));
}
fn add(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let sum = fma(1., a.x, b.x);
    let vb = fma(-1., a.x, sum);
    let ra = fma(-1., fma(-1., vb, sum), a.x);
    let rb = fma(-1., vb, b.x);
    let error = fma(1., fma(1., ra, rb), fma(1., a.y, b.y));
    let high = fma(1., sum, error);
    return vec2(high, fma(-1., fma(-1., sum, high), error));
}
fn weighted(value: f32, coefficient: vec2<f32>) -> vec2<f32> {
    return add(product(value, coefficient.x), product(value, coefficient.y));
}
fn mix_codes(rgb: vec3<f32>, row: u32) -> f32 {
    let green = weighted(rgb.y, weights[row * 3u + 1u]);
    let red = add(green, weighted(rgb.x, weights[row * 3u]));
    let result = add(red, weighted(rgb.z, weights[row * 3u + 2u]));
    var integral = floor(result.x);
    if integral == result.x && result.y < 0. { integral -= 1.; }
    return clamp(integral, 0., 65535.);
}
@compute @workgroup_size(8, 8)
fn code_channel_map(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= p.shape.x || id.y >= p.shape.y { return; }
    let i = (id.y * p.shape.x + id.x) * 3u;
    let rgb = vec3(source[i], source[i + 1u], source[i + 2u]);
    var first: f32;
    if p.shape.w != 0u {
        first = f32((u32(rgb.x) + u32(rgb.y) + u32(rgb.z)) / 3u);
    } else {
        first = mix_codes(rgb, 0u);
    }
    output[i] = first;
    if p.shape.z != 0u { output[i + 1u] = first; output[i + 2u] = first; }
    else { output[i + 1u] = mix_codes(rgb, 1u); output[i + 2u] = mix_codes(rgb, 2u); }
}
