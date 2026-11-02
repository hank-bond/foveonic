// Shared contrast after camera appearance.
// Negative contrast is reduced to preserve a monotone curve over the UI range.
const NEGATIVE_CONTRAST_SCALE: f32 = 0.3333333333;

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
    if y > EPSILON && y < 1. && p.values.x != 0. {
        let root = sqrt(y);
        let mapped = contrast(root, p.values.x);
        v *= mapped * mapped / y;
    }
    store(i, v);
}

// Anchor black and nominal white in square-root brightness. Use the ratio of
// roots directly so near-black samples need no division by measured luminance.
// One scalar preserves RGB ratios, signed components and out-of-range headroom.
@compute @workgroup_size(8, 8)
fn photographic_brightness(@builtin(global_invocation_id) id: vec3<u32>) {
    if !valid(id) { return; }
    let i = id.y * p.shape.x + id.x;
    var v = rgb(i);
    let y = dot(v, p.luma.xyz);
    if y > 0. && y < 1. && p.values.x != 1. {
        let g = p.values.x / (1. + sqrt(y) * (p.values.x - 1.));
        v *= g * g;
    }
    store(i, v);
}
