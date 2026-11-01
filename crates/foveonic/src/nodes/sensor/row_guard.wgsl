// Fine row evidence is measured before joint layer filtering changes its covariance.
struct Params {
    source: vec4<u32>, area: vec4<u32>,
    noise: array<vec4<f32>, 3>, inverse_covariance: array<vec4<f32>, 3>, mean_pass: u32,
    reserved0: u32,
    reserved1: u32,
    reserved2: u32,
}
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> original: array<f32>;
@group(0) @binding(2) var<storage, read> shield_corrected: array<f32>;
@group(0) @binding(3) var<storage, read> black: array<f32>;
@group(0) @binding(4) var<storage, read> weights: array<f32>;
@group(0) @binding(5) var<storage, read_write> spans: array<f32>;
@group(0) @binding(6) var<storage, read_write> stats: array<f32>;
@group(0) @binding(7) var<storage, read_write> field: array<f32>;
@group(0) @binding(8) var<storage, read_write> guarded: array<f32>;
@group(0) @binding(9) var<storage, read_write> means: array<f32>;
@group(0) @binding(10) var<storage, read_write> guide: array<f32>;
@group(0) @binding(11) var<storage, read_write> output_pixels: array<f32>;
const LAYERS: u32 = 3u;
const SPANS: u32 = 12u;
const SPAN_COMPONENTS: u32 = 3u * LAYERS;
const ROW_COMPONENTS: u32 = 3u * LAYERS;
const FINE_RADIUS: i32 = 5;
const SIGNAL_RADIUS: i32 = 12;
const RELIABILITY_RADIUS: i32 = 32;
const GUIDE_RADIUS: i32 = 16;
const FINE_KERNEL: u32 = 0u;
const SIGNAL_KERNEL: u32 = u32(2 * FINE_RADIUS + 1);
const RELIABILITY_KERNEL: u32 = SIGNAL_KERNEL + u32(2 * SIGNAL_RADIUS + 1);
const GUIDE_KERNEL: u32 = RELIABILITY_KERNEL + u32(2 * RELIABILITY_RADIUS + 1);
const VARIANCE_FLOOR: f32 = 1e-5;
const ERROR_FLOOR: f32 = 1e-6;
const MINIMUM_SCATTER: f32 = 0.05;
const MAD_SIGMA: f32 = 1.4826;
const HUBER_PASSES: u32 = 2u;
const HUBER_SIGMAS: f32 = 1.5;
const RESIDUAL_SIGMAS: f32 = 2.5;
const VALID_MINIMUM: f32 = -50.;
const VALID_MAXIMUM: f32 = 3800.;
const ORIGINAL_CLIP: f32 = 3900.;
const INNOVATION_VARIANCE: f32 = 4.;
const AGREEMENT_VARIANCE: f32 = 4.;
const AGREEMENT_SPANS: u32 = 9u;
const SNR_FULL: f32 = 4.;
const SNR_ZERO: f32 = 12.;
// Half-sample reflection matches the row estimator at the active boundary.
fn reflect(value: i32, extent: u32) -> u32 {
    let period = 2 * i32(extent);
    let folded = ((value % period) + period) % period;
    return u32(min(folded, period - 1 - folded));
}
fn span_value(offset: u32) -> vec3<f32> {
    return vec3(spans[offset], spans[offset + 1u], spans[offset + 2u]);
}
fn row_stat(offset: u32) -> vec3<f32> {
    return vec3(stats[offset], stats[offset + 1u], stats[offset + 2u]);
}
fn field_value(offset: u32) -> vec3<f32> {
    return vec3(field[offset], field[offset + 1u], field[offset + 2u]);
}
fn field_mean(offset: u32) -> vec3<f32> {
    return vec3(means[offset], means[offset + 1u], means[offset + 2u]);
}
fn guide_value(offset: u32) -> vec3<f32> {
    return vec3(guide[offset], guide[offset + 1u], guide[offset + 2u]);
}
fn guarded_value(offset: u32) -> vec3<f32> {
    return vec3(guarded[offset], guarded[offset + 1u], guarded[offset + 2u]);
}
fn sample(x: u32, y: u32) -> vec3<f32> {
    let i = ((y + p.area.y) * p.source.x + x + p.area.x) * LAYERS;
    return vec3(original[i] - black[0], original[i + 1u] - black[1], original[i + 2u] - black[2]);
}
fn variance(signal: vec3<f32>) -> vec3<f32> {
    let v = max(signal, vec3(0.));
    return p.noise[0].xyz + p.noise[1].xyz * v + p.noise[2].xyz * v * v;
}
fn vertical(x: u32, y: u32, radius: i32, kernel: u32) -> vec3<f32> {
    var sum = vec3(0.);
    for (var d = -radius; d <= radius; d++) {
        sum += sample(x, reflect(i32(y) + d, p.source.w)) * weights[kernel + u32(d + radius)];
    }
    return sum;
}
fn sorted12(input: array<f32, SPANS>) -> array<f32, SPANS> {
    var a = input;
    for (var i = 1u; i < SPANS; i++) {
        let value = a[i];
        var j = i;
        loop {
            if j == 0u {
                break;
            }
            if a[j - 1u] <= value {
                break;
            }
            a[j] = a[j - 1u];
            j--;
        }
        a[j] = value;
    }
    return a;
}
fn median12(input: array<f32, SPANS>) -> f32 {
    let a = sorted12(input);
    return (a[SPANS / 2u - 1u] + a[SPANS / 2u]) * 0.5;
}
fn gaussian_field(y: u32, radius: i32, kernel: u32) -> vec3<f32> {
    var value = vec3(0.);
    for (var d = -radius; d <= radius; d++) {
        let i = reflect(i32(y) + d, p.source.w) * LAYERS;
        value += field_value(i) * weights[kernel + u32(d + radius)];
    }
    return value;
}
@compute @workgroup_size(64)
fn row_guard_profiles(@builtin(global_invocation_id) id: vec3<u32>) {
    let y = id.x;
    let span = id.y;
    if y >= p.source.w {
        return;
    }
    let base_width = p.source.z / SPANS;
    let remainder = p.source.z % SPANS;
    let start = span * base_width + min(span, remainder);
    let end = start + base_width + u32(span < remainder);
    var residual = vec3(0.);
    var residual_error = vec3(0.);
    var variances = vec3(0.);
    var variance_error = vec3(0.);
    var reference = vec3(0.);
    var reference_error = vec3(0.);
    var count = 0.;
    for (var x = start; x < end; x++) {
        let z = sample(x, y);
        let i = ((y + p.area.y) * p.source.x + x + p.area.x) * LAYERS;
        let difference = vec3(original[i] - shield_corrected[i], original[i + 1u] - shield_corrected[i + 1u], original[i + 2u] - shield_corrected[i + 2u]);
        let corrected_reference = difference - reference_error;
        let next_reference = reference + corrected_reference;
        reference_error = (next_reference - reference) - corrected_reference;
        reference = next_reference;
        if !all(z < vec3(VALID_MAXIMUM)) || !all(z > vec3(VALID_MINIMUM)) {
            continue;
        }
        let v = max(variance(vertical(x, y, SIGNAL_RADIUS, SIGNAL_KERNEL)), vec3(VARIANCE_FLOOR));
        let limit = RESIDUAL_SIGMAS * sqrt(v);
        let r = clamp(z - vertical(x, y, FINE_RADIUS, FINE_KERNEL), -limit, limit);
        let corrected_residual = r - residual_error;
        let next_residual = residual + corrected_residual;
        residual_error = (next_residual - residual) - corrected_residual;
        residual = next_residual;
        let corrected_variance = v - variance_error;
        let next_variance = variances + corrected_variance;
        variance_error = (next_variance - variances) - corrected_variance;
        variances = next_variance;
        count += 1.;
    }
    let n = max(count, 1.);
    let out = (span * p.source.w + y) * SPAN_COMPONENTS;
    for (var c = 0u; c < LAYERS; c++) {
        spans[out + c] = residual[c] / n;
        spans[out + LAYERS + c] = variances[c] / (n * n);
        // Span contributions add to the full-width mean even for unequal spans.
        spans[out + 2u * LAYERS + c] = reference[c] / f32(p.source.z);
    }
}
@compute @workgroup_size(64)
fn row_guard_robust(@builtin(global_invocation_id) id: vec3<u32>) {
    let y = id.x;
    if y >= p.source.w {
        return;
    }
    for (var c = 0u; c < LAYERS; c++) {
        var values: array<f32, SPANS>;
        var deviations: array<f32, SPANS>;
        var reference = 0.;
        for (var s = 0u; s < SPANS; s++) {
            let i = (s * p.source.w + y) * SPAN_COMPONENTS;
            values[s] = spans[i + c];
            reference += spans[i + 2u * LAYERS + c];
        }
        let median = median12(values);
        for (var s = 0u; s < SPANS; s++) {
            deviations[s] = abs(values[s] - median);
        }
        let scatter = MAD_SIGMA * median12(deviations);
        var center = median;
        for (var iteration = 0u; iteration < HUBER_PASSES; iteration++) {
            var sum = 0.;
            var normalization = 0.;
            for (var s = 0u; s < SPANS; s++) {
                let weight = min(1., HUBER_SIGMAS * max(scatter, MINIMUM_SCATTER) / max(abs(values[s] - center), ERROR_FLOOR));
                sum += values[s] * weight;
                normalization += weight;
            }
            center = sum / normalization;
        }
        stats[y * ROW_COMPONENTS + c] = center;
        stats[y * ROW_COMPONENTS + LAYERS + c] = max(scatter * scatter / f32(SPANS), ERROR_FLOOR);
        stats[y * ROW_COMPONENTS + 2u * LAYERS + c] = reference;
    }
}
@compute @workgroup_size(64)
fn row_guard_reliability(@builtin(global_invocation_id) id: vec3<u32>) {
    let y = id.x;
    if y >= p.source.w {
        return;
    }
    var power = vec3(0.);
    var uncertainty = vec3(0.);
    for (var d = -RELIABILITY_RADIUS; d <= RELIABILITY_RADIUS; d++) {
        let i = reflect(i32(y) + d, p.source.w) * ROW_COMPONENTS;
        let value = row_stat(i);
        let w = weights[RELIABILITY_KERNEL + u32(d + RELIABILITY_RADIUS)];
        power += value * value * w;
        uncertainty += row_stat(i + LAYERS) * w;
    }
    let reliability = clamp(vec3(1.) - uncertainty / max(power, vec3(ERROR_FLOOR)), vec3(0.), vec3(1.));
    let value = row_stat(y * ROW_COMPONENTS) * reliability;
    for (var c = 0u; c < LAYERS; c++) {
        field[y * LAYERS + c] = value[c];
    }
}
@compute @workgroup_size(1)
fn row_guard_mean(@builtin(global_invocation_id) id: vec3<u32>) {
    let c = id.x;
    var sum = 0.;
    var error = 0.;
    for (var y = 0u; y < p.source.w; y++) {
        var value = field[y * LAYERS + c];
        if p.mean_pass == 0u {
            value -= gaussian_field(y, RELIABILITY_RADIUS, RELIABILITY_KERNEL)[c];
        }
        let corrected = value - error;
        let next = sum + corrected;
        error = (next - sum) - corrected;
        sum = next;
    }
    means[p.mean_pass * LAYERS + c] = sum / f32(p.source.w);
}
@compute @workgroup_size(64)
fn row_guard_confidence(@builtin(global_invocation_id) id: vec3<u32>) {
    let y = id.x;
    if y >= p.source.w {
        return;
    }
    let row = y * ROW_COMPONENTS;
    var reference = row_stat(row + 2u * LAYERS);
    for (var d = -FINE_RADIUS; d <= FINE_RADIUS; d++) {
        let i = reflect(i32(y) + d, p.source.w) * ROW_COMPONENTS + 2u * LAYERS;
        reference -= row_stat(i) * weights[FINE_KERNEL + u32(d + FINE_RADIUS)];
    }
    let estimate = field_value(y * LAYERS) - gaussian_field(y, RELIABILITY_RADIUS, RELIABILITY_KERNEL) - field_mean(0u) - reference;
    let projected = vec3(dot(p.inverse_covariance[0].xyz, estimate), dot(p.inverse_covariance[1].xyz, estimate), dot(p.inverse_covariance[2].xyz, estimate));
    let mahalanobis = max(dot(estimate, projected), 0.) / INNOVATION_VARIANCE;
    var distances: array<f32, SPANS>;
    let center = row_stat(row);
    for (var s = 0u; s < SPANS; s++) {
        let i = (s * p.source.w + y) * SPAN_COMPONENTS;
        let difference = span_value(i) - center;
        distances[s] = dot(difference * difference / max(span_value(i + LAYERS), vec3(ERROR_FLOOR)), vec3(1.));
    }
    let ordered = sorted12(distances);
    var agreement = 0.;
    for (var s = 0u; s < AGREEMENT_SPANS; s++) {
        agreement += ordered[s];
    }
    let heterogeneity = max(agreement / f32(AGREEMENT_SPANS) / AGREEMENT_VARIANCE - 1., 0.);
    let confidence = 1. / ((1. + mahalanobis * mahalanobis) * (1. + heterogeneity * heterogeneity));
    let value = estimate * confidence;
    for (var c = 0u; c < LAYERS; c++) {
        guarded[y * LAYERS + c] = value[c];
    }
}
@compute @workgroup_size(8, 8)
fn row_guard_horizontal(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= p.source.z || id.y >= p.source.w {
        return;
    }
    var value = vec3(0.);
    for (var d = -GUIDE_RADIUS; d <= GUIDE_RADIUS; d++) {
        value += sample(reflect(i32(id.x) + d, p.source.z), id.y) * weights[GUIDE_KERNEL + u32(d + GUIDE_RADIUS)];
    }
    let i = (id.y * p.source.z + id.x) * LAYERS;
    for (var c = 0u; c < LAYERS; c++) {
        guide[i + c] = value[c];
    }
}
@compute @workgroup_size(8, 8)
fn row_guard_apply(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= p.source.xy) {
        return;
    }
    let i = (id.y * p.source.x + id.x) * LAYERS;
    let in_area = all(id.xy >= p.area.xy) && all(id.xy < p.area.zw);
    let clipped = any(vec3(original[i], original[i + 1u], original[i + 2u]) >= vec3(ORIGINAL_CLIP));
    if !in_area || clipped {
        for (var c = 0u; c < LAYERS; c++) {
            output_pixels[i + c] = original[i + c];
        }
        return;
    }
    let point = id.xy - p.area.xy;
    var signal = vec3(0.);
    for (var d = -GUIDE_RADIUS; d <= GUIDE_RADIUS; d++) {
        let offset = (reflect(i32(point.y) + d, p.source.w) * p.source.z + point.x) * LAYERS;
        signal += guide_value(offset) * weights[GUIDE_KERNEL + u32(d + GUIDE_RADIUS)];
    }
    let snr = max(dot(signal, vec3(1.)), 0.) / sqrt(max(dot(variance(signal), vec3(1.)), ERROR_FLOOR));
    let t = clamp((SNR_ZERO - snr) / (SNR_ZERO - SNR_FULL), 0., 1.);
    let gate = t * t * (3. - 2. * t);
    let correction = guarded_value(point.y * LAYERS) - field_mean(LAYERS);
    for (var c = 0u; c < LAYERS; c++) {
        output_pixels[i + c] = shield_corrected[i + c] - correction[c] * gate;
    }
}
