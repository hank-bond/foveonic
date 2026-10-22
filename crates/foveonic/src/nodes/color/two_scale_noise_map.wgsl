struct Params { shape: vec4<u32>, gains: vec4<f32> }
fn divide(numerator: f32, denominator: f32) -> f32 {
    let quotient = numerator / denominator;
    let residual = fma(-quotient, denominator, numerator);
    return fma(1.0, quotient, residual / denominator);
}
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read_write> destination: array<f32>;
fn gray(x: u32, y: u32) -> f32 {
    let i = (min(y, params.shape.y - 1u) * params.shape.x + x) * 3u;
    return divide(fma(1.0, fma(1.0, source[i], source[i + 1u]), source[i + 2u]), 3.0);
}
@compute @workgroup_size(8, 8)
fn noise_protection_map(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = params.shape.x;
    let height = params.shape.y;
    if id.x >= width || id.y >= height { return; }
    let i = id.y * width + id.x;
    destination[i] = 0.0;
    if id.x == 0u || id.x + 1u == width || id.y == 0u { return; }
    var means: array<f32, 9>;
    for (var dy = 0u; dy < 3u; dy++) {
        for (var dx = 0u; dx < 3u; dx++) { means[dy * 3u + dx] = gray(id.x + dx - 1u, id.y + dy - 1u); }
    }
    var total = fma(means[1], 0.125, 0.0);
    for (var j = 0u; j < 9u; j++) {
        if j != 1u { total = fma(means[j], select(0.125, 0.0, j == 4u), total); }
    }
    let center = means[4];
    if center != 0.0 && total != 0.0 {
        let amount = fma(-1.0, 1.0, max(divide(total, center), divide(center, total)));
        destination[i] = clamp(amount * 10.0, 0.0, 1.0);
    }
}
