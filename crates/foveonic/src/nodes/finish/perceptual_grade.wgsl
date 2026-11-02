// Linear pre-curve grading. Every selection is measured before any adjustment.
struct Params {
    shape: vec4<u32>, values: vec4<f32>,
    bands: array<vec4<f32>, 3>, luma: vec4<f32>,
    to_srgb: array<vec4<f32>, 3>, from_srgb: array<vec4<f32>, 3>,
}
@group(0) @binding(0) var<storage, read> src: array<f32>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage, read_write> dst: array<f32>;
fn transform(v: vec3<f32>, rows: array<vec4<f32>, 3>) -> vec3<f32> {
    return vec3(dot(v, rows[0].xyz), dot(v, rows[1].xyz), dot(v, rows[2].xyz));
}
fn working_rgb(lab: vec3<f32>) -> vec3<f32> {
    return transform(from_lab(lab), p.from_srgb);
}
fn polynomial(c: vec4<f32>, x: f32) -> f32 {
    return ((c.w * x + c.z) * x + c.y) * x + c.x;
}
fn colorfulness_contrast(x: f32, amount: f32) -> f32 {
    return x + amount * x * (1. - x) * (2. * x - 1.);
}
// Each working RGB component is cubic along an Oklab hue ray. Split at its
// derivative roots so narrow out-of-gamut regions near blue cannot be skipped.
fn channel_boundary(c: vec4<f32>, lower: f32, upper: f32, limit: f32) -> f32 {
    var first = limit;
    var second = limit;
    if abs(c.w) > 0.0000001 {
        let discriminant = c.z * c.z - 3. * c.w * c.y;
        if discriminant > 0. {
            let root = sqrt(discriminant);
            // This quadratic form avoids cancellation at the nearer root.
            let q = -c.z - select(-root, root, c.z >= 0.);
            let a = q / (3. * c.w);
            let b = c.y / q;
            first = clamp(min(a, b), 0., limit);
            second = clamp(max(a, b), 0., limit);
        }
    } else if abs(c.z) > 0.0000001 {
        first = clamp(-c.y / (2. * c.z), 0., limit);
    }
    let endpoints = array<f32, 3>(first, second, limit);
    var low = 0.;
    for (var segment = 0u; segment < 3u; segment++) {
        var high = endpoints[segment];
        let value = polynomial(c, high);
        if value < lower || value > upper {
            for (var k = 0u; k < 20u; k++) {
                let middle = (low + high) * 0.5;
                let sample = polynomial(c, middle);
                if sample >= lower && sample <= upper { low = middle; }
                else { high = middle; }
            }
            return low;
        }
        low = high;
    }
    return limit;
}
fn saturate(input: vec3<f32>, lab: vec3<f32>, amount: f32) -> vec3<f32> {
    let chroma = length(lab.yz);
    let minimum = min(min(input.x, input.y), input.z);
    let maximum = max(max(input.x, input.y), input.z);
    // Avoid a conversion round trip and amplification of matrix noise on grays.
    if (amount == 0. && p.values.z == 0.) || maximum - minimum <= 0.000001 * max(abs(maximum), 0.000001)
        || chroma <= 0.000001 * max(abs(lab.x), 0.000001) { return input; }
    if amount <= -1. { return working_rgb(vec3(lab.x, 0., 0.)); }

    let neutral = working_rgb(vec3(lab.x, 0., 0.));
    // Retain existing signed values and headroom. The boundary limits new
    // excursions rather than clipping the input to nominal black and white.
    let lower = min(0., min(minimum, min(min(neutral.x, neutral.y), neutral.z)));
    let upper = max(1., max(maximum, max(max(neutral.x, neutral.y), neutral.z)));
    let direction = lab.yz / chroma;
    let slopes = lab_roots(vec3(0., direction));
    let linear = transform(from_lms(3. * lab.x * lab.x * slopes), p.from_srgb);
    let quadratic = transform(from_lms(3. * lab.x * slopes * slopes), p.from_srgb);
    let cubic = transform(from_lms(slopes * slopes * slopes), p.from_srgb);
    let limit = max(2. * chroma, 4. * abs(lab.x));
    var boundary = limit;
    for (var channel = 0u; channel < 3u; channel++) {
        boundary = min(boundary, channel_boundary(
            vec4(neutral[channel], linear[channel], quadratic[channel], cubic[channel]),
            lower, upper, limit));
    }
    // Existing signed or over-range colors define full occupancy. Preserve
    // their baseline, but still allow deliberate desaturation.
    let available = max(chroma, boundary);
    let occupancy = chroma / available;
    let contrast = p.values.z;
    let shaped = colorfulness_contrast(occupancy, contrast);
    var gain = shaped / occupancy;
    if amount > 0. {
        gain *= (1. + amount) / (1. + amount * shaped);
    } else if amount < 0. {
        // Vivid colors lose a larger fraction. Approach monochrome continuously
        // without disproportionately draining muted colors at modest amounts.
        gain *= (1. + amount) / (1. + amount - amount * shaped);
    }
    if gain == 1. { return input; }
    var output = working_rgb(vec3(lab.x, lab.yz * gain));
    // A wide-gamut hue ray can briefly leave RGB bounds while desaturating.
    // If the target lands in that gap, use the first reachable chroma boundary
    // rather than letting the following camera curve clip channels separately.
    let tolerance = 0.000002 * max(1., max(abs(lower), abs(upper)));
    if any(output < vec3(lower - tolerance)) || any(output > vec3(upper + tolerance)) {
        output = working_rgb(vec3(lab.x, direction * min(chroma * gain, boundary)));
    }
    return output;
}
@compute @workgroup_size(8, 8)
fn perceptual_grade(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= p.shape.xy) { return; }
    let i = id.y * p.shape.x + id.x; let n = i * 3u;
    let input = vec3(src[n], src[n+1u], src[n+2u]);
    let y = dot(input, p.luma.xyz) / p.values.x;
    let ev = log2(max(y, 0.000001)) - p.values.y;
    let shadow = 1. - smoothstep(-3., -0.5, ev);
    let highlight = smoothstep(0.5, 3., ev);
    let weights = vec3(shadow, 1. - shadow - highlight, highlight);
    if p.shape.z > 0u { dst[i] = weights[p.shape.z-1u]; return; }
    var result = input;
    if p.shape.w == 0u {
        let original = to_lab(transform(input / p.values.x, p.to_srgb));
        let saturation = p.values.w + dot(weights, vec3(p.bands[0].y, p.bands[1].y, p.bands[2].y));
        if saturation != 0. || p.values.z != 0. {
            result = saturate(input / p.values.x, original, saturation) * p.values.x;
        }
    }
    dst[n] = result.x; dst[n+1u] = result.y; dst[n+2u] = result.z;
}
