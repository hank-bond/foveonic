// Subject-hue brightness adjustment guided by independent source colors.
@group(0) @binding(3) var<storage, read> guide: array<f32>;
const MONO_SENSITIVITY_STOPS: f32 = 2.0;
const HUE_COUNT: u32 = 6u;
const CHROMA_PROTECTION_START: f32 = 0.02;
const CHROMA_PROTECTION_END: f32 = 0.15;
const SHADOW_PROTECTION_START: f32 = 0.0001;
const SHADOW_PROTECTION_END: f32 = 0.002;

@compute @workgroup_size(8, 8)
fn mono_sensitivity(@builtin(global_invocation_id) id: vec3<u32>) {
    if !valid(id) { return; }
    let i = id.y * p.shape.x + id.x;
    let n = i * 3u;
    let original = vec3(guide[n], guide[n + 1u], guide[n + 2u]);
    let color = max(vec3(
        dot(p.guide_matrix[0].xyz, original),
        dot(p.guide_matrix[1].xyz, original),
        dot(p.guide_matrix[2].xyz, original)
    ), vec3(0.));
    let hi = max(color.x, max(color.y, color.z));
    let lo = min(color.x, min(color.y, color.z));
    let span = hi - lo;
    var hue = 0.;
    if span > EPSILON {
        // RGB hue occupies six unit-width sectors, beginning at red.
        if hi == color.x { hue = (color.y - color.z) / span; }
        else if hi == color.y { hue = 2. + (color.z - color.x) / span; }
        else { hue = 4. + (color.x - color.y) / span; }
        hue = hue - 6. * floor(hue / 6.);
    }
    var sum = 0.;
    var total = 0.;
    for (var h = 0u; h < HUE_COUNT; h++) {
        let d = abs(hue - f32(h));
        let distance = min(d, 6. - d);
        let weight = 1. - smoothstep(0., 1., distance);
        sum += weight * p.hues[h / 4u][h % 4u];
        total += weight;
    }
    // Protect gray subjects and unstable near-black chroma.
    let chroma = smoothstep(CHROMA_PROTECTION_START, CHROMA_PROTECTION_END, span / max(hi, EPSILON));
    let shadow = smoothstep(SHADOW_PROTECTION_START, SHADOW_PROTECTION_END, hi);
    let gain = exp2(MONO_SENSITIVITY_STOPS * sum / max(total, EPSILON) * chroma * shadow);
    store(i, rgb(i) * gain);
}
