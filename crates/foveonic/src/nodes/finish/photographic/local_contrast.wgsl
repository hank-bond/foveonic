// Six-scale log-brightness local contrast, independent of reconstruction.
// Each pixel carries its current base and accumulated exposure correction.
@group(0) @binding(6) var<storage, read> wave: array<vec2<f32>>;
@group(0) @binding(7) var<storage, read_write> wave_output: array<vec2<f32>>;
@group(0) @binding(8) var<storage, read> previous_wave: array<vec2<f32>>;
const LOCAL_BAND_LIMIT_EV: f32 = 0.5;
const LOCAL_TOTAL_LIMIT_EV: f32 = 1.0;

@compute @workgroup_size(8, 8)
fn local_extract(@builtin(global_invocation_id) id: vec3<u32>) {
    if !valid(id) { return; }
    let i = id.y * p.shape.x + id.x;
    wave_output[i] = vec2(log2(max(light(i), EPSILON)), 0.);
}

@compute @workgroup_size(8, 8)
fn local_blur(@builtin(global_invocation_id) id: vec3<u32>) {
    if !valid(id) { return; }
    let i = id.y * p.shape.x + id.x;
    // Separable five-tap binomial kernel. Strides double at successive levels.
    let weights = array<f32, 5>(0.0625, 0.25, 0.375, 0.25, 0.0625);
    var mean = 0.;
    for (var d = -2; d <= 2; d++) {
        var offset = vec2<i32>(d * i32(p.shape.z), 0);
        if p.shape.w != 0u { offset = vec2<i32>(0, d * i32(p.shape.z)); }
        mean += weights[u32(d + 2)] * wave[index(vec2<i32>(id.xy) + offset)].x;
    }
    // Keep the unblurred base alive across both axes instead of copying it
    // into a separate scalar buffer at every level.
    var detail = wave[i].y;
    if p.shape.w != 0u {
        detail += clamp(previous_wave[i].x - mean, -LOCAL_BAND_LIMIT_EV, LOCAL_BAND_LIMIT_EV) * p.values.x;
    }
    wave_output[i] = vec2(mean, detail);
}

@compute @workgroup_size(8, 8)
fn local_apply(@builtin(global_invocation_id) id: vec3<u32>) {
    if !valid(id) { return; }
    let i = id.y * p.shape.x + id.x;
    store(i, rgb(i) * exp2(clamp(wave[i].y, -LOCAL_TOTAL_LIMIT_EV, LOCAL_TOTAL_LIMIT_EV)));
}
