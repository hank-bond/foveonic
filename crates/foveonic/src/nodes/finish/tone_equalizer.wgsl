// Coordinated HDR response and multiscale edge-preserving brightness base.
// Response equations and residual-limited slopes were studied in the reference
// implementation version 16.6. Our filter uses floating EV moments and working-space luminance.
struct Guide { shape: vec4<u32>, sampling: vec4<u32>, smoothing: vec4<f32>, luma: vec4<f32> };
@group(0) @binding(0) var<storage, read> rgb: array<f32>;
@group(0) @binding(1) var<uniform> guide: Guide;
@group(0) @binding(2) var<storage, read> moments: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> filtered: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read> brightness_mask: array<f32>;
@group(0) @binding(5) var<storage, read_write> output_mask: array<f32>;
struct Adjustment { shape: vec4<u32>, amounts: vec4<f32>, luma: vec4<f32>, pivot: vec4<f32> };
@group(0) @binding(6) var<uniform> adjustment: Adjustment;
@group(0) @binding(7) var<storage, read_write> output_rgb: array<f32>;
@group(0) @binding(8) var<storage, read> original: array<vec4<f32>>;
@group(0) @binding(9) var<storage, read> previous_base: array<vec4<f32>>;
@group(0) @binding(10) var<storage, read_write> output_gain: array<f32>;
@group(0) @binding(11) var<storage, read> gains: array<f32>;

const NOMINAL_WHITE: f32 = 4095.0;
const MIN_LUMINANCE: f32 = 0.0000152587890625; // 2^-16, stabilizes the guide only.
const MASK_FLOOR_EV: f32 = -8.0;
const MASK_SPAN_EV: f32 = 8.0;
const RANGE_SLOPE: f32 = 1.8;
const SHADOW_LIMIT_EV: f32 = 2.1;
const HIGHLIGHT_LIMIT_EV: f32 = 3.0;
const BLACK_LIMIT_EV: f32 = 4.2;
const LIFT_DETAIL: f32 = 0.3;
const LOWER_DETAIL: f32 = 0.5;
const SHADOW_DETAIL: f32 = 0.0247487556;
const BLACK_OFFSET_EV: f32 = 1.666666746;
const WHITE_OFFSET_EV: f32 = -1.111111164;
const LOWER_EXTREME_GAIN: f32 = 4.0;
const HIGHLIGHT_AMOUNT_SCALE: f32 = -0.6;
const RESIDUAL_VARIANCE_LIMIT: f32 = 8.0;
const EDGE_SLOPE_BOOST: f32 = 1.125;
// A one-cell radius has no smaller neighborhood. The native operation uses
// its largest residual code here, so a zero residual cannot erase every edge.
const MAX_RESIDUAL_EV: f32 = 65535.0 / 4096.0;

fn cell_exposure(cell: vec2<u32>, sample: u32, extent: vec2<u32>, luma: vec3<f32>) -> f32 {
    var brightness = 0.0;
    for (var y = 0u; y < sample; y++) {
        for (var x = 0u; x < sample; x++) {
            let p = min(cell * sample + vec2<u32>(x, y), extent - 1u);
            let i = (p.y * extent.x + p.x) * 3u;
            brightness += dot(vec3<f32>(rgb[i], rgb[i + 1u], rgb[i + 2u]), luma);
        }
    }
    return log2(max(brightness / (f32(sample * sample) * NOMINAL_WHITE), MIN_LUMINANCE));
}
fn encode_mask(ev: f32) -> f32 { return (ev - MASK_FLOOR_EV) / MASK_SPAN_EV; }

@compute @workgroup_size(8, 8)
fn reduce_guide(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= guide.sampling.xy) { return; }
    let ev = cell_exposure(id.xy, guide.sampling.z, guide.shape.xy, guide.luma.xyz);
    filtered[id.y * guide.sampling.x + id.x] = vec4<f32>(ev, ev * ev, 0.0, 0.0);
}
fn at(point: vec2<i32>) -> vec4<f32> {
    let p = vec2<u32>(clamp(point, vec2<i32>(0), vec2<i32>(guide.sampling.xy) - 1));
    return moments[p.y * guide.sampling.x + p.x];
}
@compute @workgroup_size(8, 8)
fn mean_horizontal(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= guide.sampling.xy) { return; }
    let radius = i32(guide.smoothing.x);
    let half_radius = i32(guide.smoothing.z);
    let phase = guide.sampling.w;
    var sum = vec2<f32>(0.0);
    var smaller = 0.0;
    for (var d = -radius; d <= radius; d++) {
        let value = at(vec2<i32>(id.xy) + vec2<i32>(d, 0));
        if phase != 1u { sum += value.xy; }
        if abs(d) <= half_radius {
            smaller += select(value.x, value.z, phase == 1u);
        }
    }
    var mean = sum / f32(2 * radius + 1);
    if phase == 1u { mean = at(vec2<i32>(id.xy)).xy; }
    filtered[id.y * guide.sampling.x + id.x] = vec4<f32>(mean, smaller / f32(2 * half_radius + 1), 0.0);
}
@compute @workgroup_size(8, 8)
fn statistics(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= guide.sampling.xy) { return; }
    let radius = i32(guide.smoothing.x);
    let half_radius = i32(guide.smoothing.z);
    var sum = vec2<f32>(0.0);
    var smaller = 0.0;
    for (var d = -radius; d <= radius; d++) {
        let value = at(vec2<i32>(id.xy) + vec2<i32>(0, d));
        sum += value.xy;
        if abs(d) <= half_radius { smaller += value.z; }
    }
    let i = id.y * guide.sampling.x + id.x;
    let residual = select(abs(original[i].x - smaller / f32(2 * half_radius + 1)), MAX_RESIDUAL_EV, half_radius == 0);
    filtered[i] = vec4<f32>(sum / f32(2 * radius + 1), residual, 0.0);
}
@compute @workgroup_size(8, 8)
fn coefficients(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= guide.sampling.xy) { return; }
    let half_radius = i32(guide.smoothing.z);
    var residual = 0.0;
    for (var d = -half_radius; d <= half_radius; d++) {
        residual += at(vec2<i32>(id.xy) + vec2<i32>(0, d)).z;
    }
    residual /= f32(2 * half_radius + 1);
    let mean = at(vec2<i32>(id.xy)).xy;
    let variance = min(max(mean.y - mean.x * mean.x, 0.0), RESIDUAL_VARIANCE_LIMIT * residual);
    let a = min(EDGE_SLOPE_BOOST * variance / (variance + guide.smoothing.y), 1.0);
    filtered[id.y * guide.sampling.x + id.x] = vec4<f32>(a, (1.0 - a) * mean.x, 0.0, 0.0);
}
@compute @workgroup_size(8, 8)
fn mean_vertical(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= guide.sampling.xy) { return; }
    let radius = i32(guide.smoothing.x);
    var sum = vec2<f32>(0.0);
    for (var d = -radius; d <= radius; d++) { sum += at(vec2<i32>(id.xy) + vec2<i32>(0, d)).xy; }
    filtered[id.y * guide.sampling.x + id.x] = vec4<f32>(sum / f32(2 * radius + 1), 0.0, 0.0);
}
fn interpolated_coefficients(point: vec2<f32>) -> vec2<f32> {
    let p = vec2<i32>(floor(point));
    let t = fract(point);
    return mix(mix(at(p).xy, at(p + vec2<i32>(1, 0)).xy, t.x), mix(at(p + vec2<i32>(0, 1)).xy, at(p + vec2<i32>(1, 1)).xy, t.x), t.y);
}
@compute @workgroup_size(8, 8)
fn update_base(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= guide.shape.zw) { return; }
    let ratio = f32(guide.sampling.z) / 2.0;
    let coefficients = interpolated_coefficients((vec2<f32>(id.xy) + 0.5) / ratio - 0.5);
    let i = id.y * guide.shape.z + id.x;
    filtered[i] = vec4<f32>(coefficients.x * previous_base[i].x + coefficients.y, 0.0, 0.0, 0.0);
}
@compute @workgroup_size(8, 8)
fn finish_mask(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= guide.shape.xy) { return; }
    let p = id.xy / 2u;
    output_mask[id.y * guide.shape.x + id.x] = encode_mask(previous_base[p.y * guide.shape.z + p.x].x);
}

// The soft knee and saturating response replace independent, fixed exposure
// bands. Slider amounts blend linear gains, not a fixed number of stops.
fn knee(z: f32) -> f32 {
    let t = 0.5 + 0.5 * z * (1.0 + abs(z)) / (1.0 + abs(z) + z * z);
    return 0.5 * max(t, z + 1.0 - t);
}
fn limited(q: f32, limit: f32) -> f32 {
    let u = q / limit;
    let u2 = u * u;
    let p = (u2 * u2 + u) * (1.0 + u + u2);
    return limit * p / (1.0 + p);
}
fn coordinate(base: f32, detail: f32, amount: f32) -> f32 {
    return base + select(-LIFT_DETAIL * detail, LOWER_DETAIL * detail, amount < 0.0);
}
fn blend_gain(amount: f32, ev: f32) -> f32 { return mix(1.0, exp2(ev), abs(max(amount, -1.0))); }
fn hdr_gain(light: f32, base: f32) -> f32 {
    let detail = light - base;
    let amounts = adjustment.amounts;
    let s = amounts.y;
    let h = HIGHLIGHT_AMOUNT_SCALE * amounts.z;
    let b = amounts.x;
    let w = -amounts.w;
    let xs = coordinate(base, detail, s);
    let shadow = limited(knee(-RANGE_SLOPE * xs), SHADOW_LIMIT_EV);
    let shadow_ev = select(shadow + max(0.0, -SHADOW_DETAIL * xs) * detail, -shadow, s < 0.0);
    let gs = blend_gain(s, shadow_ev);
    let highlight = limited(knee(RANGE_SLOPE * coordinate(base, detail, h)), HIGHLIGHT_LIMIT_EV);
    let gh = blend_gain(h, select(highlight, -highlight, h < 0.0));
    let xb = coordinate(base, detail, b);
    let qb = knee(-RANGE_SLOPE * xb);
    let a = limited(qb, BLACK_LIMIT_EV) - limited(qb, SHADOW_LIMIT_EV);
    let black_ev = limited(knee(-RANGE_SLOPE * (xb + BLACK_OFFSET_EV + log2(gs) + a)), BLACK_LIMIT_EV - SHADOW_LIMIT_EV) + a;
    let gb = blend_gain(b, black_ev * select(1.0, -LOWER_EXTREME_GAIN, b < 0.0));
    let white_ev = knee(RANGE_SLOPE * (coordinate(base, detail, w) + WHITE_OFFSET_EV - log2(gh)));
    let gw = blend_gain(w, white_ev * select(1.0, -LOWER_EXTREME_GAIN, w < 0.0));
    return gb * gs / (gw * gh);
}
@compute @workgroup_size(8, 8)
fn calculate_gain(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= adjustment.shape.zw) { return; }
    let sample = u32(adjustment.pivot.y);
    let p = min(id.xy * sample, adjustment.shape.xy - 1u);
    let light = cell_exposure(id.xy, sample, adjustment.shape.xy, adjustment.luma.xyz) - adjustment.pivot.x;
    let base = brightness_mask[p.y * adjustment.shape.x + p.x] * MASK_SPAN_EV + MASK_FLOOR_EV - adjustment.pivot.x;
    output_gain[id.y * adjustment.shape.z + id.x] = hdr_gain(light, base);
}
fn gain_at(p: vec2<i32>) -> f32 {
    let q = vec2<u32>(clamp(p, vec2<i32>(0), vec2<i32>(adjustment.shape.zw) - 1));
    return gains[q.y * adjustment.shape.z + q.x];
}
@compute @workgroup_size(8, 8)
fn apply_ranges(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= adjustment.shape.xy) { return; }
    let point = (vec2<f32>(id.xy) + 0.5) / adjustment.pivot.y - 0.5;
    let p = vec2<i32>(floor(point));
    let t = fract(point);
    let gain = mix(mix(gain_at(p), gain_at(p + vec2<i32>(1, 0)), t.x), mix(gain_at(p + vec2<i32>(0, 1)), gain_at(p + vec2<i32>(1, 1)), t.x), t.y);
    let i = (id.y * adjustment.shape.x + id.x) * 3u;
    for (var c = 0u; c < 3u; c++) { output_rgb[i + c] = rgb[i + c] * gain; }
}
