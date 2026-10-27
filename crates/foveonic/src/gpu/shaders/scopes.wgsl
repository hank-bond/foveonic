// Sample a region of the current floating buffer in its oriented view.
@group(0) @binding(4) var<uniform> signal_white: vec4<f32>;
@group(0) @binding(3) var<uniform> region: vec4<f32>;

fn sample_region(id: vec2<u32>) -> vec3<f32> {
    let size = params.crop.zw - params.crop.xy;
    let oriented_size = select(size, size.yx, params.source.z == 90u || params.source.z == 270u);
    let start = region.xy * vec2<f32>(oriented_size);
    let end = region.zw * vec2<f32>(oriented_size);
    let point = start + (vec2<f32>(id) + vec2(0.5)) * (end - start) / vec2<f32>(params.output_size.xy) - vec2(0.5);
    return sample_oriented(clamp(point, vec2(0.), vec2<f32>(oriented_size) - vec2(1.)));
}
@group(0) @binding(2) var<storage, read_write> counts: array<atomic<u32>>;
const DENSITY: u32 = 1024u;
const OUTSIDE: u32 = 263168u;
// Split each sample's 256 weight units without changing their total.
const DENSITY_SCALE: u32 = 256u;

fn add_density(channel: u32, point: vec2<u32>, weight: u32) {
    if weight != 0u {
        atomicAdd(&counts[DENSITY + channel * 65536u + point.y * 256u + point.x], weight);
    }
}

fn spread_density(channel: u32, point: vec2<f32>) {
    let lower = vec2<u32>(floor(point));
    let upper = min(lower + vec2(1u), vec2(255u));
    let fraction = fract(point);
    let right = u32(round(fraction.x * f32(DENSITY_SCALE)));
    let left = DENSITY_SCALE - right;
    let bottom_left = u32(round(fraction.y * f32(left)));
    let bottom_right = u32(round(fraction.y * f32(right)));
    add_density(channel, lower, left - bottom_left);
    add_density(channel, vec2(upper.x, lower.y), right - bottom_right);
    add_density(channel, vec2(lower.x, upper.y), bottom_left);
    add_density(channel, upper, bottom_right);
}
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= params.output_size.xy) { return; }
    var rgb = sample_region(id.xy);
    if params.output_size.z == 1u { rgb /= signal_white.x; }
    if params.output_size.z == 4u { rgb /= 16383.; }
    if params.output_size.z == 5u { rgb /= 65535.; }
    // Shift before scaling so the signed range's endpoints remain exact.
    if params.output_size.z == 2u { rgb = (rgb + 1000.) / 2000.; }
    if params.output_size.z == 3u { rgb /= 2.; }
    var weights = select(vec3(0.2126, 0.7152, 0.0722), vec3(0.297344975, 0.627363566, 0.075291459), params.output_size.w == 1u);
    if params.output_size.w == 2u { weights = vec3(0.288055420, 0.711868286, 0.000076294); }
    let values = vec4(rgb, dot(rgb, weights));
    let x = clamp((f32(id.x) + 0.5) * 256. / f32(params.output_size.x) - 0.5, 0., 255.);
    for (var c = 0u; c < 4u; c++) {
        let value = values[c];
        if value != value { atomicAdd(&counts[OUTSIDE + 8u + c], 1u); continue; }
        if value < 0. { atomicAdd(&counts[OUTSIDE + c], 1u); }
        if value > 1. { atomicAdd(&counts[OUTSIDE + 4u + c], 1u); }
        let level = u32(round(clamp(value, 0., 1.) * 255.));
        atomicAdd(&counts[c * 256u + level], 1u);
        spread_density(c, vec2(x, (1. - clamp(value, 0., 1.)) * 255.));
    }
}
