// Apply the prepared three-channel matrix, preserving signed values and headroom.
struct Params { source: vec4<u32>, matrix: array<vec4<f32>, 3> };
@group(0) @binding(0) var<storage, read> input_pixels: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read_write> output_pixels: array<f32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.source.x || id.y >= params.source.y { return; }
    let p = (id.y * params.source.x + id.x) * 3u;
    let layers = vec3<f32>(input_pixels[p], input_pixels[p + 1u], input_pixels[p + 2u]);
    for (var c = 0u; c < 3u; c++) { output_pixels[p + c] = dot(params.matrix[c].xyz, layers); }
}
