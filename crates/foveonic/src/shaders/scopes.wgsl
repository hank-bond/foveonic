// Sample the current floating buffer in its oriented view.
@group(0) @binding(2) var<storage, read_write> counts: array<atomic<u32>>;
const DENSITY: u32 = 1024u;
const OUTSIDE: u32 = 263168u;
@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= params.output_size.xy) { return; }
    var rgb = sample_rgb(id.xy);
    if params.output_size.z == 1u { rgb /= 4095.; }
    // Shift before scaling so the signed range's endpoints remain exact.
    if params.output_size.z == 2u { rgb = (rgb + 1000.) / 2000.; }
    if params.output_size.z == 3u { rgb /= 2.; }
    let weights = select(vec3(0.2126, 0.7152, 0.0722), vec3(0.297344975, 0.627363566, 0.075291459), params.output_size.w == 1u);
    let values = vec4(rgb, dot(rgb, weights));
    let x = min(255u, id.x * 256u / params.output_size.x);
    for (var c = 0u; c < 4u; c++) {
        let value = values[c];
        if value != value { atomicAdd(&counts[OUTSIDE + 8u + c], 1u); continue; }
        if value < 0. { atomicAdd(&counts[OUTSIDE + c], 1u); }
        if value > 1. { atomicAdd(&counts[OUTSIDE + 4u + c], 1u); }
        let level = u32(round(clamp(value, 0., 1.) * 255.));
        atomicAdd(&counts[c * 256u + level], 1u);
        atomicAdd(&counts[DENSITY + c * 65536u + (255u - level) * 256u + x], 1u);
    }
}
