// Soften new saturation along the operation's native neutral-to-color direction.
// Existing signed samples and highlight headroom define the baseline bounds.
fn guard_saturation(input: vec3<f32>, candidate: vec3<f32>, neutral: f32, white: f32) -> vec3<f32> {
    let ray = input - vec3(neutral);
    let magnitude = dot(ray, ray);
    if magnitude <= 0.000000000001 * white * white { return candidate; }
    let strength = dot(candidate - input, ray) / magnitude;
    if strength <= 0. { return candidate; }
    let lower = min(0., min(neutral, min(min(input.x, input.y), input.z)));
    let upper = max(white, max(neutral, max(max(input.x, input.y), input.z)));
    var occupancy = 0.;
    for (var c = 0u; c < 3u; c++) {
        if ray[c] > 0. { occupancy = max(occupancy, ray[c] / max(upper - neutral, 0.000000000001 * white)); }
        else if ray[c] < 0. { occupancy = max(occupancy, -ray[c] / max(neutral - lower, 0.000000000001 * white)); }
    }
    occupancy = clamp(occupancy, 0., 1.);
    if occupancy == 1. { return input; }
    let attenuation = (1. - occupancy) / (1. + strength * occupancy);
    return input + (candidate - input) * attenuation;
}
