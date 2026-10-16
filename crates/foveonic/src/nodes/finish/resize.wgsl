// Reference photographic 4x4 phase interpolation and signed 16.16 crop mapping.
// Adapted with owner permission from the reference implementation's resizing.rs at e47af35.
// Retain floating samples until export transfer. Clamp taps to owned storage at
// sensor edges instead of emitting the reference diagnostic's red marker.
@group(0) @binding(2) var<storage, read_write> resized_pixels: array<f32>;
@group(0) @binding(3) var<storage, read> resize_coefficients: array<f32>;
struct ResizeMapping { origin: vec4<i32>, increments: vec4<i32> };
@group(0) @binding(4) var<uniform> resize_mapping: ResizeMapping;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= params.output_size.xy) { return; }
    let fixed = resize_mapping.origin.xy
        + i32(id.x) * resize_mapping.increments.xy
        + i32(id.y) * resize_mapping.increments.zw;
    let integer = fixed >> vec2<u32>(16u);
    let limit = vec2<i32>(params.source.xy) - vec2<i32>(1);
    var rgb = vec3<f32>(0.0);
    if resize_mapping.origin.z != 0 {
        let point = vec2<u32>(clamp(integer, vec2<i32>(0), limit));
        for (var c = 0u; c < 3u; c++) { rgb[c] = pixel(point, c); }
    } else {
        let phase = vec2<u32>(fixed >> vec2<u32>(12u)) & vec2<u32>(15u);
        let offset = (phase.y * 16u + phase.x) * 16u;
        for (var y = 0u; y < 4u; y++) {
            for (var x = 0u; x < 4u; x++) {
                let point = vec2<u32>(clamp(integer + vec2<i32>(i32(x), i32(y))
                    - vec2<i32>(1), vec2<i32>(0), limit));
                let weight = resize_coefficients[offset + y * 4u + x];
                for (var c = 0u; c < 3u; c++) {
                    rgb[c] = fma(weight, pixel(point, c), rgb[c]);
                }
            }
        }
    }
    let output_index = (id.y * params.output_size.x + id.x) * 3u;
    resized_pixels[output_index] = rgb.x;
    resized_pixels[output_index + 1u] = rgb.y;
    resized_pixels[output_index + 2u] = rgb.z;
}
