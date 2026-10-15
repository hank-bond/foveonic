// Apply exposure and interpolate the camera-derived tone table in linear RGB.
struct Params { source: vec4<u32>, exposure: vec4<f32> };
@group(0) @binding(0) var<storage, read> input_pixels: array<f32>;
@group(0) @binding(1) var<storage, read> curve: array<f32>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read_write> output_pixels: array<f32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.source.x || id.y >= params.source.y { return; }
    let p = (id.y * params.source.x + id.x) * 3u;
    for (var c = 0u; c < 3u; c++) {
        let coordinate = clamp(input_pixels[p + c] * params.exposure.x, 0.0, f32(arrayLength(&curve) - 1u));
        let lower = u32(floor(coordinate));
        let upper = min(lower + 1u, arrayLength(&curve) - 1u);
        output_pixels[p + c] = mix(curve[lower], curve[upper], fract(coordinate));
    }
}
