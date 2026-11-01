// Joint guided regression in native layer codes, applied from coarse to fine.
struct Params {
    // Full source extent, followed by reflected active-area padded extent.
    source: vec4<u32>, area: vec4<u32>, stage: vec4<u32>,
    // Strength, inverse scale squared, variance floor and update phase.
    controls: vec4<f32>, noise: array<vec4<f32>, 3>, neutral: vec4<f32>,
}
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> source: array<f32>;
@group(0) @binding(2) var<storage, read> auxiliary_a: array<f32>;
@group(0) @binding(3) var<storage, read> auxiliary_b: array<f32>;
@group(0) @binding(4) var<storage, read> auxiliary_c: array<f32>;
@group(0) @binding(5) var<storage, read_write> output_a: array<f32>;
@group(0) @binding(6) var<storage, read_write> output_b: array<f32>;
@group(0) @binding(7) var<storage, read_write> output_c: array<f32>;
@group(0) @binding(8) var<storage, read> black: array<f32>;
@group(0) @binding(9) var<storage, read> original: array<f32>;

const LAYERS: u32 = 3u;
const CLIPPED_CODE: f32 = 3900.;
const MAXIMUM_SLOPE: f32 = 8.;
const GUIDE_PRIOR_VARIANCE: f32 = 1.;
const UPDATE_PHASE: f32 = 1.;
const HORIZONTAL_WORKGROUP_WIDTH: u32 = 32u;
const HORIZONTAL_WORKGROUP_HEIGHT: u32 = 4u;
const MAXIMUM_RADIUS: u32 = 4u;
const HORIZONTAL_TILE_WIDTH: u32 = HORIZONTAL_WORKGROUP_WIDTH + 2u * MAXIMUM_RADIUS;
const HORIZONTAL_TILE_PIXELS: u32 = HORIZONTAL_TILE_WIDTH * HORIZONTAL_WORKGROUP_HEIGHT;
// Planar shared storage keeps neighboring lanes on neighboring scalars.
var<workgroup> horizontal_a: array<f32, HORIZONTAL_TILE_PIXELS * LAYERS>;
var<workgroup> horizontal_b: array<f32, HORIZONTAL_TILE_PIXELS * LAYERS>;
var<workgroup> horizontal_c: array<f32, HORIZONTAL_TILE_PIXELS * LAYERS>;

// Separate accessors avoid unsupported workgroup-pointer arguments.
fn store_horizontal_a(index: u32, value: vec3<f32>) {
    horizontal_a[index] = value.x;
    horizontal_a[HORIZONTAL_TILE_PIXELS + index] = value.y;
    horizontal_a[2u * HORIZONTAL_TILE_PIXELS + index] = value.z;
}
fn load_horizontal_a(index: u32) -> vec3<f32> {
    return vec3(horizontal_a[index], horizontal_a[HORIZONTAL_TILE_PIXELS + index],
        horizontal_a[2u * HORIZONTAL_TILE_PIXELS + index]);
}
fn store_horizontal_b(index: u32, value: vec3<f32>) {
    horizontal_b[index] = value.x;
    horizontal_b[HORIZONTAL_TILE_PIXELS + index] = value.y;
    horizontal_b[2u * HORIZONTAL_TILE_PIXELS + index] = value.z;
}
fn load_horizontal_b(index: u32) -> vec3<f32> {
    return vec3(horizontal_b[index], horizontal_b[HORIZONTAL_TILE_PIXELS + index],
        horizontal_b[2u * HORIZONTAL_TILE_PIXELS + index]);
}
fn store_horizontal_c(index: u32, value: vec3<f32>) {
    horizontal_c[index] = value.x;
    horizontal_c[HORIZONTAL_TILE_PIXELS + index] = value.y;
    horizontal_c[2u * HORIZONTAL_TILE_PIXELS + index] = value.z;
}
fn load_horizontal_c(index: u32) -> vec3<f32> {
    return vec3(horizontal_c[index], horizontal_c[HORIZONTAL_TILE_PIXELS + index],
        horizontal_c[2u * HORIZONTAL_TILE_PIXELS + index]);
}

fn reflect(coordinate: i32, extent: u32) -> u32 {
    let period = 2 * (i32(extent) - 1);
    let folded = ((coordinate % period) + period) % period;
    return u32(min(folded, period - folded));
}
fn load_source(index: u32) -> vec3<f32> {
    return vec3(source[index * LAYERS], source[index * LAYERS + 1u], source[index * LAYERS + 2u]);
}
fn load_a(index: u32) -> vec3<f32> {
    return vec3(auxiliary_a[index * LAYERS], auxiliary_a[index * LAYERS + 1u], auxiliary_a[index * LAYERS + 2u]);
}
fn load_b(index: u32) -> vec3<f32> {
    return vec3(auxiliary_b[index * LAYERS], auxiliary_b[index * LAYERS + 1u], auxiliary_b[index * LAYERS + 2u]);
}
fn load_c(index: u32) -> vec3<f32> {
    return vec3(auxiliary_c[index * LAYERS], auxiliary_c[index * LAYERS + 1u], auxiliary_c[index * LAYERS + 2u]);
}
fn store_a(index: u32, value: vec3<f32>) {
    for (var c = 0u; c < LAYERS; c++) { output_a[index * LAYERS + c] = value[c]; }
}

@compute @workgroup_size(8, 8)
fn multiscale_prepare(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= p.source.zw) { return; }
    let point = vec2(reflect(i32(id.x), p.area.z - p.area.x), reflect(i32(id.y), p.area.w - p.area.y)) + p.area.xy;
    let value = load_source(point.y * p.source.x + point.x) - vec3(black[0], black[1], black[2]);
    store_a(id.y * p.source.z + id.x, value);
}

@compute @workgroup_size(8, 8)
fn multiscale_downsample(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= p.stage.xy) { return; }
    var value = vec3(0.);
    for (var y = 0u; y < p.stage.z; y++) {
        for (var x = 0u; x < p.stage.z; x++) {
            value += load_source((id.y * p.stage.z + y) * p.source.z + id.x * p.stage.z + x);
        }
    }
    store_a(id.y * p.stage.x + id.x, value * p.controls.y);
}

@compute @workgroup_size(HORIZONTAL_WORKGROUP_WIDTH, HORIZONTAL_WORKGROUP_HEIGHT)
fn multiscale_moments(@builtin(global_invocation_id) id: vec3<u32>,
    @builtin(local_invocation_index) lane: u32, @builtin(workgroup_id) group: vec3<u32>) {
    let radius = i32(p.stage.w);
    for (var index = lane; index < HORIZONTAL_TILE_PIXELS; index += HORIZONTAL_WORKGROUP_WIDTH * HORIZONTAL_WORKGROUP_HEIGHT) {
        let x = reflect(i32(group.x * HORIZONTAL_WORKGROUP_WIDTH + index % HORIZONTAL_TILE_WIDTH) - radius, p.stage.x);
        let y = min(group.y * HORIZONTAL_WORKGROUP_HEIGHT + index / HORIZONTAL_TILE_WIDTH, p.stage.y - 1u);
        store_horizontal_a(index, load_source(y * p.stage.x + x));
    }
    // Partial groups must finish their shared loads before any lane leaves.
    workgroupBarrier();
    if any(id.xy >= p.stage.xy) { return; }
    let first = (id.y % HORIZONTAL_WORKGROUP_HEIGHT) * HORIZONTAL_TILE_WIDTH + id.x % HORIZONTAL_WORKGROUP_WIDTH;
    var mean = vec3(0.); var square = vec3(0.); var cross = vec3(0.); var guide_square = 0.;
    for (var tap = 0u; tap < 2u * p.stage.w + 1u; tap++) {
        let value = load_horizontal_a(first + tap);
        let guide = (value.x + value.y + value.z) / f32(LAYERS);
        mean += value; square += value * value; cross += value * guide; guide_square += guide * guide;
    }
    let reciprocal = 1. / f32(2 * radius + 1);
    let pixel = id.y * p.stage.x + id.x;
    for (var c = 0u; c < LAYERS; c++) {
        output_a[pixel * 4u + c] = mean[c] * reciprocal;
        output_b[pixel * LAYERS + c] = square[c] * reciprocal;
        output_c[pixel * LAYERS + c] = cross[c] * reciprocal;
    }
    output_a[pixel * 4u + 3u] = guide_square * reciprocal;
}

@compute @workgroup_size(8, 8)
fn multiscale_coefficients(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= p.stage.xy) { return; }
    var mean = vec3(0.); var square = vec3(0.); var cross = vec3(0.); var guide_square = 0.;
    let radius = i32(p.stage.w);
    for (var dy = -radius; dy <= radius; dy++) {
        let pixel = reflect(i32(id.y) + dy, p.stage.y) * p.stage.x + id.x;
        mean += vec3(auxiliary_a[pixel * 4u], auxiliary_a[pixel * 4u + 1u], auxiliary_a[pixel * 4u + 2u]);
        square += load_b(pixel); cross += load_c(pixel); guide_square += auxiliary_a[pixel * 4u + 3u];
    }
    let reciprocal = 1. / f32(2 * radius + 1);
    mean *= reciprocal; square *= reciprocal; cross *= reciprocal; guide_square *= reciprocal;
    let guide_mean = (mean.x + mean.y + mean.z) / f32(LAYERS);
    let guide_variance = max(guide_square - guide_mean * guide_mean, p.controls.z);
    let channel_variance = max(square - mean * mean, vec3(p.controls.z));
    let covariance = cross - mean * guide_mean;
    let signal = max(mean, vec3(0.));
    let variance = max((p.noise[0].xyz + p.noise[1].xyz * signal + p.noise[2].xyz * signal * signal) * p.controls.y, vec3(p.controls.z));
    let guide_noise = (variance.x + variance.y + variance.z) / f32(LAYERS * LAYERS);
    let channel_noise = variance * p.controls.x;
    let residual = max(channel_variance - covariance * covariance / (guide_variance + guide_noise), vec3(0.));
    var weight = residual / (residual + channel_noise);
    let prior = max(covariance / (guide_variance + GUIDE_PRIOR_VARIANCE), vec3(0.));
    var a = ((vec3(1.) - weight) * covariance + prior * guide_noise) / (guide_variance + guide_noise);
    var b = (vec3(1.) - weight) * mean - a * guide_mean;
    let fallback = abs(a) > vec3(MAXIMUM_SLOPE);
    a = select(a, vec3(0.), fallback); b = select(b, vec3(0.), fallback); weight = select(weight, vec3(1.), fallback);
    let pixel = id.y * p.stage.x + id.x;
    store_a(pixel, a);
    for (var c = 0u; c < LAYERS; c++) { output_b[pixel * LAYERS + c] = b[c]; output_c[pixel * LAYERS + c] = weight[c]; }
}

@compute @workgroup_size(HORIZONTAL_WORKGROUP_WIDTH, HORIZONTAL_WORKGROUP_HEIGHT)
fn multiscale_average(@builtin(global_invocation_id) id: vec3<u32>,
    @builtin(local_invocation_index) lane: u32, @builtin(workgroup_id) group: vec3<u32>) {
    let radius = i32(p.stage.w);
    for (var index = lane; index < HORIZONTAL_TILE_PIXELS; index += HORIZONTAL_WORKGROUP_WIDTH * HORIZONTAL_WORKGROUP_HEIGHT) {
        let x = reflect(i32(group.x * HORIZONTAL_WORKGROUP_WIDTH + index % HORIZONTAL_TILE_WIDTH) - radius, p.stage.x);
        let y = min(group.y * HORIZONTAL_WORKGROUP_HEIGHT + index / HORIZONTAL_TILE_WIDTH, p.stage.y - 1u);
        let pixel = y * p.stage.x + x;
        store_horizontal_a(index, load_a(pixel));
        store_horizontal_b(index, load_b(pixel));
        store_horizontal_c(index, load_c(pixel));
    }
    // Partial groups must finish their shared loads before any lane leaves.
    workgroupBarrier();
    if any(id.xy >= p.stage.xy) { return; }
    let first = (id.y % HORIZONTAL_WORKGROUP_HEIGHT) * HORIZONTAL_TILE_WIDTH + id.x % HORIZONTAL_WORKGROUP_WIDTH;
    var a = vec3(0.); var b = vec3(0.); var weight = vec3(0.);
    for (var tap = 0u; tap < 2u * p.stage.w + 1u; tap++) {
        a += load_horizontal_a(first + tap);
        b += load_horizontal_b(first + tap);
        weight += load_horizontal_c(first + tap);
    }
    let reciprocal = 1. / f32(2 * radius + 1);
    let pixel = id.y * p.stage.x + id.x;
    store_a(pixel, a * reciprocal);
    for (var c = 0u; c < LAYERS; c++) {
        output_b[pixel * LAYERS + c] = b[c] * reciprocal;
        output_c[pixel * LAYERS + c] = weight[c] * reciprocal;
    }
}

fn interpolated_delta(point: vec2<u32>) -> vec3<f32> {
    // Pixel centers follow bilinear resize with align_corners=false.
    let coordinate = (vec2<f32>(point) + vec2(0.5)) / f32(p.stage.z) - vec2(0.5);
    let lower = vec2<i32>(floor(coordinate));
    let blend = fract(coordinate);
    let low = vec2<u32>(clamp(lower, vec2(0), vec2<i32>(p.stage.xy) - vec2(1)));
    let high = vec2<u32>(clamp(lower + vec2(1), vec2(0), vec2<i32>(p.stage.xy) - vec2(1)));
    let a = load_a(low.y * p.stage.x + low.x); let b = load_a(low.y * p.stage.x + high.x);
    let c = load_a(high.y * p.stage.x + low.x); let d = load_a(high.y * p.stage.x + high.x);
    return mix(mix(a, b, blend.x), mix(c, d, blend.x), blend.y);
}

@compute @workgroup_size(8, 8)
fn multiscale_apply(@builtin(global_invocation_id) id: vec3<u32>) {
    if p.controls.w == UPDATE_PHASE {
        if any(id.xy >= p.source.zw) { return; }
        var delta = interpolated_delta(id.xy);
        delta -= p.neutral.xyz * ((delta.x + delta.y + delta.z) / p.neutral.w);
        let pixel = id.y * p.source.z + id.x;
        store_a(pixel, load_source(pixel) + delta);
        return;
    }
    if any(id.xy >= p.stage.xy) { return; }
    var a = vec3(0.); var b = vec3(0.); var weight = vec3(0.);
    let radius = i32(p.stage.w);
    for (var dy = -radius; dy <= radius; dy++) {
        let pixel = reflect(i32(id.y) + dy, p.stage.y) * p.stage.x + id.x;
        a += load_a(pixel); b += load_b(pixel); weight += load_c(pixel);
    }
    let reciprocal = 1. / f32(2 * radius + 1);
    let pixel = id.y * p.stage.x + id.x;
    let value = load_source(pixel); let guide = (value.x + value.y + value.z) / f32(LAYERS);
    store_a(pixel, a * reciprocal * guide + b * reciprocal + weight * reciprocal * value - value);
}

@compute @workgroup_size(8, 8)
fn multiscale_finish(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= p.source.xy) { return; }
    let pixel = id.y * p.source.x + id.x;
    let original_value = vec3(original[pixel * LAYERS], original[pixel * LAYERS + 1u], original[pixel * LAYERS + 2u]);
    var value = original_value;
    if all(id.xy >= p.area.xy) && all(id.xy < p.area.zw) && !any(original_value >= vec3(CLIPPED_CODE)) {
        let point = id.xy - p.area.xy;
        value = load_source(point.y * p.source.z + point.x) + vec3(black[0], black[1], black[2]);
    }
    store_a(pixel, value);
}
