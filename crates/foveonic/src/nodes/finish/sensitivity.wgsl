// One scalar exposure correction preserves RGB ratios and the chosen reconstruction.
struct Params {
    shape: vec4<u32>,
    values: vec4<f32>,
    hues: array<vec4<f32>, 2>,
    luma: vec4<f32>,
    guide_matrix: array<vec4<f32>, 3>,
}
@group(0) @binding(0) var<storage, read> src: array<f32>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage, read_write> dst: array<f32>;
@group(0) @binding(3) var<storage, read> aux: array<f32>;
const SIGNAL_FLOOR: f32 = 0.00001;
const MAX_TRANSFER_STOPS: f32 = 6.;
const SHADOW_FADE: f32 = 4.;
// Half-stop range sigma limits transfer across strong brightness edges.
const RANGE_SIGMA_STOPS: f32 = 0.5;
const SPATIAL_SIGMA_FRACTION: f32 = 0.5;

fn valid(id: vec3<u32>) -> bool {
    return id.x < p.shape.x && id.y < p.shape.y;
}
fn rgb(i: u32) -> vec3<f32> {
    let n = i * 3u;
    return vec3(src[n], src[n + 1u], src[n + 2u]);
}
fn guide_y(i: u32) -> f32 {
    let n = i * 3u;
    return dot(vec3(aux[n], aux[n + 1u], aux[n + 2u]), p.luma.xyz);
}

@compute @workgroup_size(8, 8)
fn color_sensitivity_filter(@builtin(global_invocation_id) id: vec3<u32>) {
    if !valid(id) { return; }
    let i = id.y * p.shape.x + id.x;
    let floor_value = p.values.x * SIGNAL_FLOOR;
    let center = log2(max(guide_y(i), floor_value));
    let radius = i32(p.shape.z);
    var sum = 0.;
    var total = 0.;
    for (var k = -radius; k <= radius; k++) {
        var q = vec2<i32>(id.xy);
        if p.shape.w == 0u { q.x += k; } else { q.y += k; }
        q = clamp(q, vec2(0), vec2<i32>(p.shape.xy) - 1);
        let n = u32(q.y) * p.shape.x + u32(q.x);
        let difference = (log2(max(guide_y(n), floor_value)) - center) / RANGE_SIGMA_STOPS;
        let distance = f32(k) / max(f32(radius) * SPATIAL_SIGMA_FRACTION, 1.);
        let weight = exp(-0.5 * (distance * distance + difference * difference));
        sum += src[n] * weight;
        total += weight;
    }
    dst[i] = sum / total;
}

@compute @workgroup_size(8, 8)
fn color_sensitivity_apply(@builtin(global_invocation_id) id: vec3<u32>) {
    if !valid(id) { return; }
    let i = id.y * p.shape.x + id.x;
    let n = i * 3u;
    let result = rgb(i) * exp2(aux[i]);
    dst[n] = result.x;
    dst[n + 1u] = result.y;
    dst[n + 2u] = result.z;
}
