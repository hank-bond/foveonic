struct Params { shape: vec4<u32>, controls: vec4<f32>, dark: vec4<f32> }
@group(0) @binding(0) var<storage, read> rgb: array<f32>;
@group(0) @binding(1) var<storage, read> yuv: array<f32>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read> model: array<vec2<f32>>;
@group(0) @binding(4) var<storage, read> intensity: array<f32>;
@group(0) @binding(5) var<storage, read_write> destination: array<f32>;
@group(0) @binding(6) var<storage, read_write> guide: array<i32>;
@group(0) @binding(7) var<storage, read_write> mask: array<u32>;

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
fn nonnegative(value: vec2<f32>) -> bool {
    return value.x > 0.0 || (value.x == 0.0 && value.y >= 0.0);
}
fn neutral_ratio(value: f32, blue: f32) -> bool {
    // Double 0.95 and 1.05 represented by high/low Float32 words.
    let low = add(vec2<f32>(value, 0.0), -multiply(vec2<f32>(0.95, 1.1920929132713809e-8), blue));
    let high = add(multiply(vec2<f32>(1.05, 4.7683716530855236e-8), blue), vec2<f32>(-value, 0.0));
    return nonnegative(low) && nonnegative(high);
}
@compute @workgroup_size(8, 8)
fn uv_classify(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = params.shape.x;
    let height = params.shape.y;
    if id.x >= width || id.y >= height { return; }
    let pixel = id.y * width + id.x;
    let i = pixel * 3u;
    for (var ch = 0u; ch < 3u; ch++) { destination[i + ch] = yuv[i + ch]; }
    guide[pixel] = 0;
    mask[pixel] = 0u;
    if id.x == 0u || id.y == 0u || id.x + 1u == width || id.y + 1u == height { return; }
    var variance = vec3<f32>(0.0);
    for (var ch = 0u; ch < 3u; ch++) {
        let z = max(rgb[i + ch], 0.0);
        let polynomial = add(multiply(add(multiply(model[ch + 6u], z), model[ch + 3u]), z), model[ch]);
        let index = 14382u - u32(floor(clamp(z, 1.0, 14382.0)));
        variance[ch] = fma(fma(1.0, polynomial.x, polynomial.y), intensity[index], 0.0);
    }
    var rgb_mean = vec3<f32>(0.0);
    var yuv_mean = vec3<f32>(0.0);
    var counts = vec3<u32>(0u);
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let neighbor = (u32(i32(id.y) + dy) * width + u32(i32(id.x) + dx)) * 3u;
            for (var ch = 0u; ch < 3u; ch++) {
                rgb_mean[ch] = fma(1.0, fma(rgb[neighbor + ch], 0.1111111111111111, 0.0), rgb_mean[ch]);
                yuv_mean[ch] = fma(1.0, fma(yuv[neighbor + ch], 0.1111111111111111, 0.0), yuv_mean[ch]);
                let difference = fma(-1.0, rgb[i + ch], rgb[neighbor + ch]);
                if fma(difference, difference, 0.0) > variance[ch] { counts[ch]++; }
            }
        }
    }
    let excessive = u32(counts.x > 2u) + u32(counts.y > 2u) + u32(counts.z > 2u);
    let rejected = excessive >= 2u || any(counts > vec3<u32>(6u));
    let neutral = all(rgb_mean >= vec3<f32>(1.0)) && neutral_ratio(rgb_mean.x, rgb_mean.z) && neutral_ratio(rgb_mean.y, rgb_mean.z);
    let dark = nonnegative(add(params.dark.xy, vec2<f32>(-yuv_mean.x, 0.0)));
    var level = yuv[i];
    if dark || neutral || !rejected {
        destination[i + 1u] = yuv_mean.y;
        destination[i + 2u] = yuv_mean.z;
        level = yuv_mean.x;
        mask[pixel] = select(u32(level <= params.controls.x), 2u, neutral && !dark);
    }
    guide[pixel] = i32(u32(i32(level)) << 16u) >> 16;
}
