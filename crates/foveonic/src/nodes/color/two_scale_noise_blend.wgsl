struct Params { shape: vec4<u32>, gains: vec4<f32> }
fn divide(numerator: f32, denominator: f32) -> f32 {
    let quotient = numerator / denominator;
    let residual = fma(-quotient, denominator, numerator);
    return fma(1.0, quotient, residual / denominator);
}
@group(0) @binding(0) var<storage, read> current: array<f32>;
@group(0) @binding(1) var<storage, read> candidate: array<f32>;
@group(0) @binding(2) var<storage, read> original: array<f32>;
@group(0) @binding(3) var<storage, read> weights: array<f32>;
@group(0) @binding(4) var<uniform> params: Params;
@group(0) @binding(5) var<storage, read> brightness: array<f32>;
@group(0) @binding(6) var<storage, read_write> destination: array<f32>;
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

fn mix_code(a: f32, b: f32, amount: f32) -> f32 {
    let complement = add(vec2<f32>(1.0, 0.0), vec2<f32>(-amount, 0.0));
    let other = multiply(complement, b);
    return fma(1.0, fma(a, amount, 0.0), fma(1.0, other.x, other.y));
}
fn gray(red: f32, green: f32, blue: f32) -> f32 { return divide(fma(1.0, fma(1.0, red, green), blue), 3.0); }
@compute @workgroup_size(8, 8)
fn noise_protection_blend(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = params.shape.x;
    let height = params.shape.y;
    if id.x >= width || id.y >= height { return; }
    let i = (id.y * width + id.x) * 3u;
    for (var ch = 0u; ch < 3u; ch++) { destination[i + ch] = current[i + ch]; }
    if id.x == 0u || id.x + 1u == width || id.y == 0u { return; }
    var kernel = array<f32, 9>(0.25, 0.5, 0.25, 0.5, 1.0, 0.5, 0.25, 0.5, 0.25);
    var cells: array<f32, 9>;
    for (var dy = 0u; dy < 3u; dy++) {
        for (var dx = 0u; dx < 3u; dx++) { cells[dy * 3u + dx] = weights[min(id.y + dy - 1u, height - 1u) * width + id.x + dx - 1u]; }
    }
    var smoothed_weight = fma(cells[1], kernel[1], 0.0);
    for (var j = 0u; j < 9u; j++) { if j != 1u { smoothed_weight = fma(cells[j], kernel[j], smoothed_weight); } }
    let level = gray(current[i], current[i + 1u], current[i + 2u]);
    let index = u32(floor(fma(1.0, clamp(level, 1.0, 14343.0), 0.5)));
    var amount = min(clamp(smoothed_weight, 0.0, 1.0), clamp(fma(-1.0, brightness[index], 1.0), 0.0, 1.0));
    let original_gray = gray(original[i], original[i + 1u], original[i + 2u]);
    let highlight = clamp(divide(fma(-1.0, 9000.0, original_gray), 2000.0), 0.0, 1.0);
    amount = clamp(fma(1.0, amount, highlight), 0.0, 1.0);
    var result = vec3<f32>(0.0);
    var maximum = -3.4028234663852886e38;
    for (var ch = 0u; ch < 3u; ch++) {
        result[ch] = mix_code(current[i + ch], candidate[i + ch], amount);
        maximum = max(maximum, max(fma(original[i + ch], params.gains[ch], 0.0), fma(result[ch], params.gains[ch], 0.0)));
    }
    let hold = clamp(divide(fma(-1.0, 11000.0, maximum), 3000.0), 0.0, 1.0);
    for (var ch = 0u; ch < 3u; ch++) { destination[i + ch] = mix_code(original[i + ch], result[ch], hold); }
}
