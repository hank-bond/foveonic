// Linear pre-curve grading. Every selection is measured before any adjustment.
struct Params {
    shape: vec4<u32>, values: vec4<f32>,
    bands: array<vec4<f32>, 3>, luma: vec4<f32>,
    to_srgb: array<vec4<f32>, 3>, from_srgb: array<vec4<f32>, 3>,
    protection: vec4<f32>,
}
@group(0) @binding(0) var<storage, read> src: array<f32>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage, read_write> dst: array<f32>;
fn transform(v: vec3<f32>, rows: array<vec4<f32>, 3>) -> vec3<f32> {
    return vec3(dot(v, rows[0].xyz), dot(v, rows[1].xyz), dot(v, rows[2].xyz));
}
@compute @workgroup_size(8, 8)
fn perceptual_grade(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= p.shape.xy) { return; }
    let i = id.y * p.shape.x + id.x; let n = i * 3u;
    let input = vec3(src[n], src[n+1u], src[n+2u]);
    let y = dot(input, p.luma.xyz) / p.values.x;
    let ev = log2(max(y, 0.000001)) - p.values.y;
    let shadow = 1. - smoothstep(-3., -0.5, ev);
    let highlight = smoothstep(0.5, 3., ev);
    let weights = vec3(shadow, 1. - shadow - highlight, highlight);
    if p.shape.z > 0u { dst[i] = weights[p.shape.z-1u]; return; }
    var result = input;
    if p.shape.w == 0u {
        let original = to_lab(transform(input / p.values.x, p.to_srgb));
        let saturation = p.values.w + dot(weights, vec3(p.bands[0].y, p.bands[1].y, p.bands[2].y));
        if saturation != 0. || p.values.z != 0. {
            let normalized=input/p.values.x;
            let graded=perceptual_saturate(normalized, original, saturation, p.values.z, p.protection.x != 0., p.from_srgb);
            if any(graded!=normalized) { result=graded*p.values.x; }
        }
    }
    dst[n] = result.x; dst[n+1u] = result.y; dst[n+2u] = result.z;
}
