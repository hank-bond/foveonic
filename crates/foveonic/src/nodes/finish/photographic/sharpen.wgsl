// Square-root brightness sharpening with threshold and halo restraint.
@group(0) @binding(4) var<storage, read> scalar: array<f32>;
@group(0) @binding(5) var<storage, read_write> scalar_output: array<f32>;
@compute @workgroup_size(8, 8)
fn sharp_extract(@builtin(global_invocation_id) id: vec3<u32>) {
    if !valid(id) { return; }
    let i = id.y * p.shape.x + id.x;
    scalar_output[i] = sqrt(light(i));
}

@compute @workgroup_size(8, 8)
fn sharp_blur(@builtin(global_invocation_id) id: vec3<u32>) {
    if !valid(id) { return; }
    let radius = i32(p.shape.z);
    let sigma = p.values.x;
    var sum = 0.;
    var weight = 0.;
    for (var d = -radius; d <= radius; d++) {
        let w = exp(-0.5 * f32(d * d) / (sigma * sigma));
        var offset = vec2<i32>(d, 0);
        if p.shape.w != 0u { offset = vec2<i32>(0, d); }
        sum += w * scalar[index(vec2<i32>(id.xy) + offset)];
        weight += w;
    }
    scalar_output[id.y * p.shape.x + id.x] = sum / weight;
}

@compute @workgroup_size(8, 8)
fn universal_sharpen(@builtin(global_invocation_id) id: vec3<u32>) {
    if !valid(id) { return; }
    let i = id.y * p.shape.x + id.x;
    let y = light(i);
    let center = sqrt(y);
    let residual = center - scalar[i];
    let edge = max(0., abs(residual) - p.values.y);
    var result = center + sign(residual) * edge * p.values.x;
    var low = center;
    var high = center;
    // The local 3x3 extrema constrain outlines at strong edges.
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let n = sqrt(light(index(vec2<i32>(id.xy) + vec2<i32>(dx, dy))));
            low = min(low, n);
            high = max(high, n);
        }
    }
    let bounded = clamp(result, low, high);
    result = mix(result, bounded, p.values.z);
    result = max(0., result);
    var gain = 1.;
    if y > EPSILON { gain = result * result / y; }
    store(i, rgb(i) * gain);
}
