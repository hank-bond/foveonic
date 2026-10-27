// Full-frame scalar brightness texture, independent of color reconstruction.
struct Params { shape: vec4<u32>, values: vec4<f32> }
@group(0) @binding(0) var<storage, read> src: array<f32>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage, read_write> dst: array<f32>;
@group(0) @binding(3) var<storage, read> log_signal: array<f32>;
@group(0) @binding(4) var<storage, read> fine_base: array<f32>;
@group(0) @binding(5) var<storage, read> coarse_base: array<f32>;
// Log-domain protection for zero and signed camera samples.
const SIGNAL_FLOOR: f32 = 0.00001;
// Scalar log-brightness bands share the color texture node's binomial scales.
const BINOMIAL: array<f32, 5> = array(0.0625, 0.25, 0.375, 0.25, 0.0625);

fn valid(id: vec3<u32>) -> bool {
    return id.x < p.shape.x && id.y < p.shape.y;
}

@compute @workgroup_size(8, 8)
fn mono_texture_extract(@builtin(global_invocation_id) id: vec3<u32>) {
    if !valid(id) { return; }
    let i = id.y * p.shape.x + id.x;
    dst[i] = log2(max(src[i], SIGNAL_FLOOR));
}

@compute @workgroup_size(8, 8)
fn mono_texture_blur(@builtin(global_invocation_id) id: vec3<u32>) {
    if !valid(id) { return; }
    let i = id.y * p.shape.x + id.x;
    var sum = 0.;
    for (var k = -2; k <= 2; k++) {
        let offset = f32(k) * p.values.z;
        let lo = i32(floor(offset));
        var a = vec2<i32>(id.xy);
        var b = a;
        if p.shape.w == 0u { a.x += lo; b.x += lo + 1; }
        else { a.y += lo; b.y += lo + 1; }
        a = clamp(a, vec2(0), vec2<i32>(p.shape.xy) - 1);
        b = clamp(b, vec2(0), vec2<i32>(p.shape.xy) - 1);
        sum += mix(src[u32(a.y) * p.shape.x + u32(a.x)],
                   src[u32(b.y) * p.shape.x + u32(b.x)], fract(offset)) * BINOMIAL[u32(k + 2)];
    }
    dst[i] = sum;
}

@compute @workgroup_size(8, 8)
fn mono_texture_apply(@builtin(global_invocation_id) id: vec3<u32>) {
    if !valid(id) { return; }
    let i = id.y * p.shape.x + id.x;
    let fine = log_signal[i] - fine_base[i];
    let coarse = fine_base[i] - coarse_base[i];
    let gain = exp2((p.values.x - 1.) * fine + (p.values.y - 1.) * coarse);
    dst[i] = src[i] * gain;
}
