// Shield profiles predict only additive native green-magenta errors.
struct Params {
    // Full sensor extent, profile stride and optional column-mode flag.
    source: vec4<u32>, area: vec4<u32>, bottom: vec4<u32>, right: vec4<u32>, direction: vec4<f32>,
}
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> input_pixels: array<f32>;
@group(0) @binding(2) var<storage, read> original: array<f32>;
@group(0) @binding(3) var<storage, read> black: array<f32>;
@group(0) @binding(4) var<storage, read> model: array<f32>;
@group(0) @binding(5) var<storage, read_write> histogram: array<atomic<u32>>;
@group(0) @binding(6) var<storage, read_write> guards: array<f32>;
@group(0) @binding(7) var<storage, read_write> profiles: array<f32>;
@group(0) @binding(8) var<storage, read_write> means: array<f32>;
@group(0) @binding(9) var<storage, read_write> predictions: array<f32>;
@group(0) @binding(10) var<storage, read_write> prediction_means: array<f32>;
@group(0) @binding(11) var<storage, read_write> output_pixels: array<f32>;

const LAYERS: u32 = 3u;
const HISTOGRAM_BINS: u32 = 4096u;
const MINIMUM_GUARD: f32 = 8.;
const MAD_SIGMA_FACTOR: f32 = 1.4826;
const GUARD_SIGMAS: f32 = 8.;
const PEDESTAL_STATE_THRESHOLD: f32 = 24.;
const SCALE_COUNT: u32 = 5u;
const WEIGHTS_PER_MODEL: u32 = SCALE_COUNT * LAYERS;
const GAUSSIAN_OFFSET: u32 = WEIGHTS_PER_MODEL * 5u;
const COLUMN_REFERENCE_LENGTH: u32 = 4704u;
const RADII = array<u32, 4>(8u, 32u, 128u, 512u);
const KERNEL_COUNTS = array<u32, 4>(2u * RADII[0] + 1u, 2u * RADII[1] + 1u, 2u * RADII[2] + 1u, 2u * RADII[3] + 1u);
const KERNEL_OFFSETS = array<u32, 4>(0u, KERNEL_COUNTS[0], KERNEL_COUNTS[0] + KERNEL_COUNTS[1], KERNEL_COUNTS[0] + KERNEL_COUNTS[1] + KERNEL_COUNTS[2]);
const REFERENCE_OFFSET: u32 = GAUSSIAN_OFFSET + KERNEL_COUNTS[0] + KERNEL_COUNTS[1] + KERNEL_COUNTS[2] + KERNEL_COUNTS[3];

fn rectangle(axis: u32) -> vec4<u32> { return select(p.right, p.bottom, axis == 1u); }
fn length(axis: u32) -> u32 { return select(p.area.w - p.area.y, p.area.z - p.area.x, axis == 1u); }
fn reflect(coordinate: i32, extent: u32) -> u32 {
    let period = 2 * (i32(extent) - 1);
    let folded = ((coordinate % period) + period) % period;
    return u32(min(folded, period - folded));
}
fn profile(axis: u32, position: u32) -> vec3<f32> {
    let base = (axis * p.source.z + position) * LAYERS;
    return vec3(profiles[base], profiles[base + 1u], profiles[base + 2u]);
}
fn axis_mean(axis: u32) -> vec3<f32> { return vec3(means[axis * LAYERS], means[axis * LAYERS + 1u], means[axis * LAYERS + 2u]); }
fn compensated_add(pair: vec2<f32>, value: f32) -> vec2<f32> {
    let corrected = fma(-1., pair.y, value);
    let total = fma(1., pair.x, corrected);
    let error = fma(-1., pair.x, total) - corrected;
    return vec2(total, error);
}

@compute @workgroup_size(64)
fn shield_histogram(@builtin(global_invocation_id) id: vec3<u32>) {
    let axis = id.y; let r = rectangle(axis); let width = r.z - r.x;
    if id.x >= width * (r.w - r.y) { return; }
    let point = r.xy + vec2(id.x % width, id.x / width);
    let base = (point.y * p.source.x + point.x) * LAYERS;
    for (var c = 0u; c < LAYERS; c++) {
        let code = u32(clamp(original[base + c], 0., f32(HISTOGRAM_BINS - 1u)));
        atomicAdd(&histogram[(axis * LAYERS + c) * HISTOGRAM_BINS + code], 1u);
    }
}

@compute @workgroup_size(1)
fn shield_guard(@builtin(global_invocation_id) id: vec3<u32>) {
    let axis = id.y; let channel = id.x;
    let r = rectangle(axis); let count = (r.z - r.x) * (r.w - r.y);
    let low_rank = (count - 1u) / 2u; let high_rank = count / 2u;
    let base = (axis * LAYERS + channel) * HISTOGRAM_BINS;
    var seen = 0u; var low = 0u; var high = 0u; var found_low = false;
    for (var code = 0u; code < HISTOGRAM_BINS; code++) {
        let next = seen + atomicLoad(&histogram[base + code]);
        if !found_low && next > low_rank { low = code; found_low = true; }
        if next > high_rank { high = code; break; }
        seen = next;
    }
    let median_twice = low + high;
    var left = i32(median_twice / 2u); var right = left + 1;
    seen = 0u; found_low = false;
    var low_distance = 0u; var high_distance = 0u;
    // Walk histogram distances from the median. Half-code medians retain the
    // exact even-sample median and MAD used to prepare the trained profiles.
    loop {
        var left_distance = 0xffffffffu; var right_distance = 0xffffffffu;
        if left >= 0 { left_distance = median_twice - u32(left) * 2u; }
        if right < i32(HISTOGRAM_BINS) { right_distance = u32(right) * 2u - median_twice; }
        let distance = min(left_distance, right_distance);
        if distance == 0xffffffffu { break; }
        var n = 0u;
        if left_distance == distance { n += atomicLoad(&histogram[base + u32(left)]); left--; }
        if right_distance == distance { n += atomicLoad(&histogram[base + u32(right)]); right++; }
        let next = seen + n;
        if !found_low && next > low_rank { low_distance = distance; found_low = true; }
        if next > high_rank { high_distance = distance; break; }
        seen = next;
    }
    let mad = f32(low_distance + high_distance) * 0.25;
    let destination = (axis * LAYERS + channel) * 2u;
    guards[destination] = f32(median_twice) * 0.5;
    guards[destination + 1u] = max(MINIMUM_GUARD, GUARD_SIGMAS * MAD_SIGMA_FACTOR * mad);
}

@compute @workgroup_size(64)
fn shield_profile(@builtin(global_invocation_id) id: vec3<u32>) {
    let axis = id.y; if id.x >= length(axis) { return; }
    let r = rectangle(axis); let count = select(r.z - r.x, r.w - r.y, axis == 1u);
    var value = vec3(0.);
    for (var i = 0u; i < count; i++) {
        let point = r.xy + select(vec2(i, id.x), vec2(id.x, i), axis == 1u);
        let base = (point.y * p.source.x + point.x) * LAYERS;
        for (var c = 0u; c < LAYERS; c++) {
            let guard = (axis * LAYERS + c) * 2u;
            value[c] += clamp(original[base + c], guards[guard] - guards[guard + 1u], guards[guard] + guards[guard + 1u]);
        }
    }
    let base = (axis * p.source.z + id.x) * LAYERS;
    for (var c = 0u; c < LAYERS; c++) { profiles[base + c] = value[c] / f32(count); }
}

@compute @workgroup_size(1)
fn shield_center(@builtin(global_invocation_id) id: vec3<u32>) {
    let axis = id.y; let channel = id.x; var sum = vec2(0.);
    for (var i = 0u; i < length(axis); i++) {
        sum = compensated_add(sum, profiles[(axis * p.source.z + i) * LAYERS + channel]);
    }
    means[axis * LAYERS + channel] = sum.x / f32(length(axis));
}

@compute @workgroup_size(64)
fn shield_predict(@builtin(global_invocation_id) id: vec3<u32>) {
    let axis = id.y; let n = length(axis); if id.x >= n { return; }
    let black_median = max(min(black[0], black[1]), min(max(black[0], black[1]), black[2]));
    let state = u32(black_median >= PEDESTAL_STATE_THRESHOLD);
    var model_index = 0u;
    if p.source.w != 0u { model_index = select(1u + state, 3u + state, axis == 1u); }
    let weights = model_index * WEIGHTS_PER_MODEL;
    let center = axis_mean(axis);
    let value = profile(axis, id.x) - center;
    var predicted = dot(value, vec3(model[weights], model[weights + 1u], model[weights + 2u]));
    for (var scale = 0u; scale < SCALE_COUNT - 1u; scale++) {
        let radius = i32(RADII[scale]); var filtered = vec3(0.);
        let kernel = GAUSSIAN_OFFSET + KERNEL_OFFSETS[scale];
        for (var offset = -radius; offset <= radius; offset++) {
            filtered += (profile(axis, reflect(i32(id.x) + offset, n)) - center) * model[kernel + u32(offset + radius)];
        }
        let coefficient = weights + (scale + 1u) * LAYERS;
        predicted += dot(filtered, vec3(model[coefficient], model[coefficient + 1u], model[coefficient + 2u]));
    }
    if axis == 1u { predicted -= model[REFERENCE_OFFSET + state * COLUMN_REFERENCE_LENGTH + id.x]; }
    predictions[axis * p.source.z + id.x] = predicted;
}

@compute @workgroup_size(1)
fn shield_reduce(@builtin(global_invocation_id) id: vec3<u32>) {
    let axis = id.y; var sum = vec2(0.);
    for (var i = 0u; i < length(axis); i++) { sum = compensated_add(sum, predictions[axis * p.source.z + i]); }
    prediction_means[axis] = sum.x / f32(length(axis));
}

@compute @workgroup_size(8, 8)
fn shield_apply(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= p.source.xy) { return; }
    let pixel = (id.y * p.source.x + id.x) * LAYERS;
    if all(id.xy >= p.area.xy) && all(id.xy < p.area.zw) {
        let point = id.xy - p.area.xy;
        var error = predictions[point.y] - prediction_means[0];
        if p.source.w != 0u { error += predictions[p.source.z + point.x] - prediction_means[1]; }
        for (var c = 0u; c < LAYERS; c++) { output_pixels[pixel + c] = input_pixels[pixel + c] - error * p.direction[c]; }
    } else {
        for (var c = 0u; c < LAYERS; c++) { output_pixels[pixel + c] = original[pixel + c]; }
    }
}
