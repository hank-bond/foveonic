// Reference-derived pointwise ColorDQ conditioning after the camera matrix.
struct Params { source: vec4<u32> };
@group(0) @binding(0) var<storage, read> input_pixels: array<f32>;
@group(0) @binding(1) var<storage, read> table: array<i32>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read_write> output_pixels: array<f32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.source.x || id.y >= params.source.y { return; }
    let p = (id.y * params.source.x + id.x) * 3u;
    let mask = params.source.z - 1u;
    var residual: array<i32, 3>;
    for (var c = 0u; c < 3u; c++) {
        let n = i32(floor(input_pixels[p + c] + 0.5));
        residual[c] = n - table[(u32(n) & mask) * 3u + c];
    }
    let mean = (residual[0] + 2 * residual[1] + residual[2]) >> 2u;
    for (var c = 0u; c < 3u; c++) {
        let correction = table[(u32(residual[c] - mean) & mask) * 3u + c];
        output_pixels[p + c] = input_pixels[p + c] - f32(correction);
    }
}
