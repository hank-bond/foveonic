// Shared working-gamut shoulder for global, tonal and selected-range saturation.
fn saturation_transform(v: vec3<f32>, rows: array<vec4<f32>, 3>) -> vec3<f32> {
    return vec3(dot(v, rows[0].xyz), dot(v, rows[1].xyz), dot(v, rows[2].xyz));
}
fn saturation_rgb(lab: vec3<f32>, rows: array<vec4<f32>, 3>) -> vec3<f32> {
    return saturation_transform(from_lab(lab), rows);
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
fn perceptual_saturate(input: vec3<f32>, lab: vec3<f32>, amount: f32, contrast: f32, prevent_clipping: bool, from_srgb: array<vec4<f32>, 3>) -> vec3<f32> {
    let chroma = length(lab.yz);
    let minimum = min(min(input.x, input.y), input.z);
    let maximum = max(max(input.x, input.y), input.z);
    // Avoid a conversion round trip and amplification of matrix noise on grays.
    if (amount == 0. && contrast == 0.) || maximum - minimum <= 0.000001 * max(abs(maximum), 0.000001)
        || chroma <= 0.000001 * max(abs(lab.x), 0.000001) { return input; }
    if amount <= -1. { return saturation_rgb(vec3(lab.x, 0., 0.), from_srgb); }

    let neutral = saturation_rgb(vec3(lab.x, 0., 0.), from_srgb);
    // Retain existing signed values and headroom. The boundary limits new
    // excursions rather than clipping the input to nominal black and white.
    let lower = min(0., min(minimum, min(min(neutral.x, neutral.y), neutral.z)));
    let upper = max(1., max(maximum, max(max(neutral.x, neutral.y), neutral.z)));
    let direction = lab.yz / chroma;
    let slopes = lab_roots(vec3(0., direction));
    let linear = saturation_transform(from_lms(3. * lab.x * lab.x * slopes), from_srgb);
    let quadratic = saturation_transform(from_lms(3. * lab.x * slopes * slopes), from_srgb);
    let cubic = saturation_transform(from_lms(slopes * slopes * slopes), from_srgb);
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
    let shaped = colorfulness_contrast(occupancy, contrast);
    var gain = shaped / occupancy;
    if amount > 0. {
        // Unlocked saturation retains the shoulder but admits movement at the old boundary.
        let colorfulness = select(shaped / (1. + shaped), shaped, prevent_clipping);
        gain *= (1. + amount) / (1. + amount * colorfulness);
    } else if amount < 0. {
        // Vivid colors lose a larger fraction. Approach monochrome continuously
        // without disproportionately draining muted colors at modest amounts.
        gain *= (1. + amount) / (1. + amount - amount * shaped);
    }
    if gain == 1. { return input; }
    var output = saturation_rgb(vec3(lab.x, lab.yz * gain), from_srgb);
    // A wide-gamut hue ray can briefly leave RGB bounds while desaturating.
    // If the target lands in that gap, use the first reachable chroma boundary
    // rather than letting the following camera curve clip channels separately.
    let tolerance = 0.000002 * max(1., max(abs(lower), abs(upper)));
    if (prevent_clipping || amount <= 0.) && (any(output < vec3(lower - tolerance)) || any(output > vec3(upper + tolerance))) {
        output = saturation_rgb(vec3(lab.x, direction * min(chroma * gain, boundary)), from_srgb);
    }
    return output;
}
