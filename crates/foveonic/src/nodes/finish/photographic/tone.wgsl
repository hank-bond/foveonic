// Shared contrast and saturation after camera appearance.
// Negative contrast is reduced to preserve a monotone curve over the UI range.
const NEGATIVE_CONTRAST_SCALE: f32 = 0.3333333333;
const SATURATION_RESPONSE_GAIN: f32 = 1.6;

// Piecewise cubic with fixed endpoints and no change at the middle pivot.
// Signed values and highlight headroom pass through unchanged.
fn contrast(x: f32, a: f32) -> f32 {
    if x <= 0. || x >= 1. { return x; }
    var d = 0.75 * x * (4. * x * x - 1.);
    if x >= 0.5 {
        let y = 1. - x;
        d = 0.75 * y * (1. - 4. * y * y);
    }
    let amount = select(a, a * NEGATIVE_CONTRAST_SCALE, a < 0.);
    return x + amount * d;
}

@compute @workgroup_size(8, 8)
fn photographic_tone(@builtin(global_invocation_id) id: vec3<u32>) {
    if !valid(id) { return; }
    let i = id.y * p.shape.x + id.x;
    var v = rgb(i);
    let y = dot(v, p.luma.xyz);
    if y > EPSILON && p.values.x != 0. {
        let root = sqrt(y);
        let mapped = contrast(root, p.values.x);
        v *= mapped * mapped / y;
    }
    let l = dot(v, p.luma.xyz);
    var amount = p.values.y;
    if amount > 1. {
        let increase = SATURATION_RESPONSE_GAIN * (amount - 1.);
        let colorful = clamp(1. - min(v.x, min(v.y, v.z)) / max(l, EPSILON), 0., 1.);
        amount = (1. + increase) / (1. + increase * colorful);
    }
    if p.values.y != 1. { v = vec3(l) + (v - vec3(l)) * amount; }
    store(i, v);
}
