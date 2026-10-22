struct Params { shape: vec4<u32>, controls: vec4<f32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage, read> thresholds: array<f32>;
@group(0) @binding(3) var<storage, read_write> output: array<f32>;

fn code(pixel: i32, channel: u32) -> u32 {
    // Preserve packed-row addressing at the right edge. The final owned row
    // supplies the following halo instead of reading outside storage.
    let limit = i32(p.shape.x * p.shape.y) - 1;
    return u32(source[u32(clamp(pixel, 0, limit)) * 3u + channel]);
}

@compute @workgroup_size(8, 8)
fn color_sharpen(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= p.shape.x || id.y >= p.shape.y { return; }
    let pixel = id.y * p.shape.x + id.x;
    for (var c = 0u; c < 3u; c++) {
        let index = pixel * 3u + c;
        let original = source[index];
        if id.y == 0u { output[index] = original; continue; }
        if id.x == 0u { output[index] = 0.; continue; }
        var total = 0u;
        for (var dx = -1; dx <= 1; dx++) {
            var column = 0u;
            for (var dy = -1; dy <= 1; dy++) {
                column += code(i32(pixel) + dy * i32(p.shape.x) + dx, c);
            }
            total += column & 65535u;
        }
        // Refine Float32 division before narrowing the native double mean.
        let quotient = f32(total) / 9.;
        let mean = fma(1., quotient, fma(-quotient, 9., f32(total)) / 9.);
        let delta = fma(-1., mean, original);
        let signal = clamp(original * p.controls.x, 0., 16384.);
        let base = floor(signal);
        let table_index = u32(base + f32(signal - base >= 0.5));
        let excess = max(0., abs(delta) - thresholds[table_index]);
        let bounded = clamp(sign(delta) * excess, -1200., 1200.);
        let correction = clamp(bounded, original * -0.1, original * 0.1);
        let change = fma(correction, p.controls.y, 0.);
        let value = clamp(fma(1., change, original), 0., 65534.);
        output[index] = floor(fma(1., value, 0.5));
    }
}
