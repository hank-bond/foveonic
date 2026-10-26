// Positive multiplicative residuals are separated in log space so bright and
// dark texture have symmetric controls and constant illumination stays neutral.
struct TextureControls { strength: vec4<f32>, steps: vec4<f32> }
@group(0) @binding(0) var<storage, read> input_values: array<f32>;
@group(0) @binding(1) var<uniform> params: NoiseParams;
@group(0) @binding(2) var<storage, read_write> output_values: array<f32>;
@group(0) @binding(3) var<storage, read> base: array<f32>;
@group(0) @binding(4) var<storage, read> ratios: array<f32>;
@group(0) @binding(5) var<storage, read> fine_low: array<f32>;
@group(0) @binding(6) var<storage, read> coarse_low: array<f32>;
@group(0) @binding(7) var<uniform> controls: TextureControls;

const BINOMIAL: array<f32, 5> = array(0.0625, 0.25, 0.375, 0.25, 0.0625);
// Samples and restoration stay in the calibrated sensor basis until color conversion.
// Positive camera-code floor for logarithms and near-black normalization.
const MIN_LUMINANCE: f32 = 0.0001;
fn base_rgb(p: u32) -> vec3<f32> { let q=p*3u; return vec3(base[q],base[q+1u],base[q+2u]); }
fn raw_rgb(p: u32) -> vec3<f32> { let q=p*3u; return vec3(input_values[q],input_values[q+1u],input_values[q+2u]); }
fn mono_log(p: u32) -> f32 {
    let a = dot(base_rgb(p), params.weights.xyz);
    let d = dot(raw_rgb(p), params.weights.xyz);
    if controls.strength.z == 0. {
        return log2(max(d, MIN_LUMINANCE) / max(a, MIN_LUMINANCE));
    }
    // Undo the stabilizing epsilon used by the reference ratio. A neutral
    // cleanup ratio reconstructs the filtered luminance, without an extra gain.
    let cleaned = max(ratios[p] * (max(a, 0.) + params.edge.z) - params.edge.z, 0.);
    let selected = mix(d, cleaned, controls.strength.z);
    return log2(max(selected, MIN_LUMINANCE) / max(a, MIN_LUMINANCE));
}
@compute @workgroup_size(8,8)
fn extract(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * tile_count(params) { return; }
    let p = id.y * params.tile.x + id.x;
    output_values[p] = mono_log(p);
}
@compute @workgroup_size(8,8)
fn blur(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * tile_count(params) { return; }
    let p = atlas_position(id.xy, params);
    var sum = 0.;
    for (var k = -2; k <= 2; k++) {
        let offset = f32(k) * controls.steps.x;
        let low = i32(floor(offset));
        var a = vec2<i32>(p.xy);
        var b = a;
        if controls.steps.y == 0. { a.x += low; b.x += low + 1; }
        else { a.y += low; b.y += low + 1; }
        a = clamp(a, vec2(0), vec2<i32>(params.tile.xy) - 1);
        b = clamp(b, vec2(0), vec2<i32>(params.tile.xy) - 1);
        let va = input_values[tile_pixel(vec3(vec2<u32>(a), p.z), params)];
        let vb = input_values[tile_pixel(vec3(vec2<u32>(b), p.z), params)];
        sum += mix(va, vb, fract(offset)) * BINOMIAL[u32(k+2)];
    }
    output_values[tile_pixel(p, params)] = sum;
}
@compute @workgroup_size(8,8)
fn mix_texture(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * tile_count(params) { return; }
    let p = id.y * params.tile.x + id.x;
    let detail = mono_log(p);
    var restored = detail;
    if controls.strength.x != 1. || controls.strength.y != 1. {
        let fine = detail - fine_low[p];
        let coarse = fine_low[p] - coarse_low[p];
        restored = coarse_low[p] + controls.strength.x * fine + controls.strength.y * coarse;
    }
    let a = base_rgb(p); let d = raw_rgb(p);
    let base_luma = max(dot(a, params.weights.xyz), MIN_LUMINANCE);
    let raw_luma = max(dot(d, params.weights.xyz), MIN_LUMINANCE);
    // Blend color proportions at a common luminance. Unit texture amounts
    // preserve the selected brightness evidence instead of boosting its contrast.
    let chroma = mix(d / raw_luma, a / base_luma, controls.strength.w);
    let value = chroma * (base_luma * exp2(restored));
    let q = p * 3u;
    output_values[q] = value.x; output_values[q+1u] = value.y; output_values[q+2u] = value.z;
}

// Multiplicative brightness change relative to neutral detail. This is the
// same log-band sum used for restoration, excluding independent noise cleanup.
@compute @workgroup_size(8,8)
fn inspect_texture(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * tile_count(params) { return; }
    let p = id.y * params.tile.x + id.x;
    var change = 0.;
    if controls.strength.x != 1. || controls.strength.y != 1. {
        let fine = mono_log(p) - fine_low[p];
        let coarse = fine_low[p] - coarse_low[p];
        change = (controls.strength.x - 1.) * fine + (controls.strength.y - 1.) * coarse;
    }
    output_values[p] = exp2(change);
}
