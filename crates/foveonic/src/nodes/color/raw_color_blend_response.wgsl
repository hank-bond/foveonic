struct Params { shape: vec4<u32>, neutral: vec4<f32>, gains: vec4<f32>, appearance: vec4<f32>, response: vec4<f32>, hue_first: vec4<f32>, hue_last: vec4<f32>, knees: vec4<f32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read> matrix: array<vec2<f32>>;
@group(0) @binding(3) var<storage, read> tables: array<f32>;
@group(0) @binding(4) var<storage, read_write> destination: array<f32>;
const RESPONSE_BASE: u32 = 901u;
const ADAPTIVE_BASE: u32 = 3973u;
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

fn divide(a: f32, b: f32) -> f32 { let q = a / b; return fma(1.0, q, fma(-q, b, a) / b); }
fn pair_divide(value: vec2<f32>, denominator: f32) -> vec2<f32> {
    let q = value.x / denominator;
    let remainder = add(value, -product(q, denominator));
    return add(vec2<f32>(q, 0.0), vec2<f32>(divide(fma(1.0, remainder.x, remainder.y), denominator), 0.0));
}
fn rounded(value: vec2<f32>) -> i32 {
    // The tiny positive native tie bias rounds exact half values upward.
    let shifted = add(value, vec2<f32>(0.5, 1.5e-8));
    var base = floor(shifted.x);
    if shifted.x == base && shifted.y < 0.0 { base -= 1.0; }
    return i32(base);
}
fn power(base: f32, exponent: f32) -> f32 {
    if exponent == 0.0 || base == 1.0 { return 1.0; }
    if base == 0.0 { return 0.0; }
    return pow(base, exponent);
}
fn signed_response(channel: u32, value: i32) -> f32 {
    let index = clamp(value, -1023, 1023);
    let sample = tables[RESPONSE_BASE + channel * 1024u + u32(abs(index))];
    return select(sample, -sample, index < 0);
}
fn hue_strength(index: u32) -> f32 {
    if index < 4u { return params.hue_first[index]; }
    return params.hue_last[index - 4u];
}
@compute @workgroup_size(8, 8)
fn raw_color_response(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.shape.x || id.y >= params.shape.y { return; }
    let i = (id.y * params.shape.x + id.x) * 3u;
    let pixel_rgb = vec3<f32>(source[i], source[i + 1u], source[i + 2u]);
    if params.response.y != 0.0 {
        for (var channel = 0u; channel < 3u; channel++) {
            destination[i + channel] = divide(fma(pixel_rgb[channel], params.appearance.y, 0.0), params.appearance.x);
        }
        return;
    }
    var normalized = vec3<f32>(0.0);
    for (var ch = 0u; ch < 3u; ch++) { normalized[ch] = fma(divide(pixel_rgb[ch], 14335.0), 4095.0, 0.0); }
    let average = fma(fma(1.0, normalized.z, fma(1.0, normalized.x, normalized.y)), 0.33, 0.0);
    let adaptive_index = u32(clamp(rounded(vec2<f32>(average, 0.0)), 0, 4095));
    let alpha = tables[ADAPTIVE_BASE + adaptive_index];
    let maximum = max(pixel_rgb.x, max(pixel_rgb.y, pixel_rgb.z));
    let brightness_position = add(pair_divide(product(maximum, params.appearance.z), params.response.x), vec2<f32>(-params.appearance.w, 0.0));
    let brightness_index = u32(clamp(rounded(brightness_position), 1, 900));
    let color = abs(pixel_rgb);
    let maximum_abs = max(color.x, max(color.y, color.z));
    let delta = fma(-1.0, min(color.x, min(color.y, color.z)), maximum_abs);
    var hue = 0.0;
    var saturation = 0.0;
    if delta != 0.0 {
        if color.x == maximum_abs { hue = divide(fma(-1.0, color.z, color.y), delta); if hue < 0.0 { hue = fma(1.0, hue, 6.0); } }
        else if color.y == maximum_abs { hue = fma(1.0, divide(fma(-1.0, color.x, color.z), delta), 2.0); }
        else { hue = fma(1.0, divide(fma(-1.0, color.y, color.x), delta), 4.0); }
        hue = fma(hue, 0.16666666666666666, 0.0);
        saturation = divide(delta, maximum_abs);
    }
    let centers = array<f32, 7>(0.0, 0.16666666666666666, 0.3333333333333333, 0.5, 0.6666666666666666, 0.8333333333333334, 1.0);
    var total = 0.0;
    var hue_mean = 0.0;
    for (var h = 0u; h < 7u; h++) {
        let center = centers[h];
        let distance = abs(fma(-1.0, hue, center));
        let weight = max(0.0, fma(-1.0, fma(distance, 6.0, 0.0), 1.0));
        total = fma(1.0, total, weight);
        hue_mean = fma(weight, hue_strength(h), hue_mean);
    }
    var amount = 0.0;
    if total > 0.0 {
        let brightness = fma(maximum, params.appearance.z, 0.0);
        let ratio = divide(min(divide(brightness, params.response.x), params.appearance.w), params.appearance.w);
        let squared = fma(ratio, ratio, 0.0);
        let first = power(divide(hue_mean, total), squared);
        let attenuation = fma(tables[brightness_index], fma(-saturation, squared, 1.0), 0.0);
        let value = fma(attenuation, power(first, saturation), 0.0);
        amount = power(clamp(value, 0.0, 1.0), alpha);
    }
    var transformed: array<vec2<f32>, 3>;
    var residual = vec3<f32>(0.0);
    for (var ch = 0u; ch < 3u; ch++) {
        let row = ch * 3u;
        transformed[ch] = add(add(multiply(matrix[row + 1u], color.y), multiply(matrix[row], color.x)), multiply(matrix[row + 2u], color.z));
        let scaled = multiply(transformed[ch], params.response.w);
        let bounded = fma(1.0, scaled.x, scaled.y);
        residual[ch] = fma(-1.0, fma(signed_response(ch, rounded(scaled)), params.response.w, 0.0), bounded);
    }
    let gray_sum = fma(1.0, fma(residual.y, 2.0, residual.x), residual.z);
    let gray = rounded(vec2<f32>(gray_sum, 0.0)) >> 2;
    let second = power(amount, divide(params.response.z, alpha));
    for (var ch = 0u; ch < 3u; ch++) {
        let correction = fma(-1.0, signed_response(ch, rounded(vec2<f32>(residual[ch], 0.0)) - gray), divide(residual[ch], params.response.w));
        let trans = fma(1.0, transformed[ch].x, transformed[ch].y);
        let a = fma(fma(-1.0, second, 1.0), trans, 0.0);
        let b = fma(second, correction, 0.0);
        let inner = fma(1.0, a, b);
        let mixed = fma(1.0, fma(amount, color[ch], 0.0), fma(fma(-1.0, amount, 1.0), inner, 0.0));
        destination[i + ch] = divide(fma(mixed, params.appearance.y, 0.0), params.appearance.x);
    }
}
