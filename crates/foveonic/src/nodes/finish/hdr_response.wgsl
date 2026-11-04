// Shared existing HDR response. Camera interpretation and spatial filtering remain with callers.
// Response equations follow the inspected coordinated spatial HDR operation.
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
fn hdr_gain(light: f32, base: f32, amounts: vec4<f32>) -> f32 {
    let detail = light - base;
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
