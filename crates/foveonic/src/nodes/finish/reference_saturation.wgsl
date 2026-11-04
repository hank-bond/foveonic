// Apply the camera's RGB saturation matrix with optional shared protection.
struct Params { source: vec4<u32>, matrix: array<vec4<f32>, 3> };
@group(0) @binding(0) var<storage, read> input_pixels: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read_write> output_pixels: array<f32>;
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.source.x || id.y >= params.source.y { return; }
    let p = (id.y * params.source.x + id.x) * 3u;
    let input = vec3(input_pixels[p], input_pixels[p + 1u], input_pixels[p + 2u]);
    var output = vec3(dot(params.matrix[0].xyz, input), dot(params.matrix[1].xyz, input), dot(params.matrix[2].xyz, input));
    if params.source.z != 0u { output = guard_saturation(input, output, dot(input, vec3(0.299, 0.587, 0.114)), 1.); }
    for (var c = 0u; c < 3u; c++) { output_pixels[p + c] = output[c]; }
}
