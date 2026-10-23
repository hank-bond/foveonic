struct Params { shape: vec4<u32>, neutral: vec4<f32>, gains: vec4<f32>, appearance: vec4<f32>, response: vec4<f32>, hue_first: vec4<f32>, hue_last: vec4<f32>, knees: vec4<f32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read_write> destination: array<f32>;
@compute @workgroup_size(8, 8)
fn raw_color_chroma(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = params.shape.x;
    let height = params.shape.y;
    if id.x >= width || id.y >= height { return; }
    let i = (id.y * width + id.x) * 3u;
    destination[i] = source[i];
    for (var channel = 1u; channel < 3u; channel++) {
        var low = vec2<f32>(3.4028234663852886e38);
        var high = vec2<f32>(-3.4028234663852886e38);
        for (var dy = -3; dy <= 3; dy++) {
            let y = u32(clamp(i32(id.y) + dy, 0, i32(height) - 1));
            for (var dx = -3; dx <= 3; dx++) {
                if dx == 0 && dy == 0 { continue; }
                let x = u32(clamp(i32(id.x) + dx, 0, i32(width) - 1));
                let value = source[(y * width + x) * 3u + channel];
                if value < low.x { low.y = low.x; low.x = value; } else if value < low.y { low.y = value; }
                if value > high.x { high.y = high.x; high.x = value; } else if value > high.y { high.y = value; }
            }
        }
        destination[i + channel] = clamp(source[i + channel], low.y, high.y);
    }
}
