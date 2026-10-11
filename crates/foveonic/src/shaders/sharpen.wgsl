// Unsharp-mask blend in tone-mapped linear RGB, before output encoding.
struct Params { source: vec4<u32>, amount: vec4<f32> };
@group(0) @binding(0) var<storage, read> input_pixels: array<f32>;
@group(0) @binding(1) var<storage, read> blurred: array<f32>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read_write> output_pixels: array<f32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.source.x || id.y >= params.source.y { return; }
    let p = (id.y * params.source.x + id.x) * 3u;
    for (var c = 0u; c < 3u; c++) {
        output_pixels[p + c] = input_pixels[p + c] + params.amount.x * (input_pixels[p + c] - blurred[p + c]);
    }
}
