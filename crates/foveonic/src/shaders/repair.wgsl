// Repair isolated marked layers from original neighbors, keeping all reads immutable.
struct Params { source: vec4<u32> };
@group(0) @binding(0) var<storage, read> input_pixels: array<f32>;
@group(0) @binding(1) var<storage, read> flags: array<u32>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read_write> output_pixels: array<f32>;
const OFFSETS = array<vec2<i32>, 8>(
    vec2<i32>(-1, -1), vec2<i32>(0, -1), vec2<i32>(1, -1), vec2<i32>(-1, 0),
    vec2<i32>(1, 0), vec2<i32>(-1, 1), vec2<i32>(0, 1), vec2<i32>(1, 1)
);

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.source.x || id.y >= params.source.y { return; }
    let pixel = id.y * params.source.x + id.x;
    let p = pixel * 3u;
    let marked = flags[pixel];
    for (var c = 0u; c < 3u; c++) {
        var value = input_pixels[p + c];
        let channel = (4u >> c) << 12u;
        if (marked & channel) != 0u && (marked & 255u) != 0u && id.x > 0u && id.y > 0u && id.x + 1u < params.source.x && id.y + 1u < params.source.y {
            var sum = 0.0;
            var count = 0u;
            var isolated = true;
            for (var n = 0u; n < 8u; n++) {
                let point = vec2<u32>(vec2<i32>(id.xy) + OFFSETS[n]);
                let neighbor = point.y * params.source.x + point.x;
                if (flags[neighbor] & channel) != 0u { isolated = false; }
                if (marked & (1u << n)) != 0u {
                    sum += input_pixels[neighbor * 3u + c];
                    count++;
                }
            }
            if isolated && count > 0u { value = sum / f32(count); }
        }
        output_pixels[p + c] = value;
    }
}
