// Separable Gaussian blur with clamped full-sensor borders.
struct Params { source: vec4<u32> };
@group(0) @binding(0) var<storage, read> input_pixels: array<f32>;
@group(0) @binding(1) var<storage, read> weights: array<f32>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read_write> output_pixels: array<f32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.source.x || id.y >= params.source.y { return; }
    let p = (id.y * params.source.x + id.x) * 3u;
    let radius = i32(params.source.w);
    for (var c = 0u; c < 3u; c++) {
        var sum = 0.0;
        for (var offset = -radius; offset <= radius; offset++) {
            var point = vec2<i32>(id.xy);
            if params.source.z == 0u { point.x += offset; } else { point.y += offset; }
            point = clamp(point, vec2<i32>(0), vec2<i32>(params.source.xy) - vec2<i32>(1));
            let neighbor = (u32(point.y) * params.source.x + u32(point.x)) * 3u + c;
            sum += weights[u32(offset + radius)] * input_pixels[neighbor];
        }
        output_pixels[p + c] = sum;
    }
}
