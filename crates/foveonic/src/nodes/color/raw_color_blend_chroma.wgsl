@compute @workgroup_size(8, 8)
fn raw_color_chroma(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = params.shape.x;
    let height = params.shape.y;
    if id.x >= width || id.y >= height { return; }
    let i = (id.y * width + id.x) * 3u;
    let radius = 3;
    var lowest = vec2<f32>(3.4028234663852886e38);
    var second_low = lowest;
    var highest = -lowest;
    var second_high = highest;
    for (var dy = -radius; dy <= radius; dy++) {
        let y = u32(clamp(i32(id.y) + dy, 0, i32(height) - 1));
        for (var dx = -radius; dx <= radius; dx++) {
            if dx == 0 && dy == 0 { continue; }
            let x = u32(clamp(i32(id.x) + dx, 0, i32(width) - 1));
            let offset = (y * width + x) * 3u;
            let chroma = vec2<f32>(source[offset + 1u], source[offset + 2u]);
            // The larger of the old minimum and this sample can be the second minimum.
            second_low = min(second_low, max(lowest, chroma));
            lowest = min(lowest, chroma);
            second_high = max(second_high, min(highest, chroma));
            highest = max(highest, chroma);
        }
    }
    let chroma = clamp(vec2<f32>(source[i + 1u], source[i + 2u]), second_low, second_high);
    write_converted(i, vec3<f32>(source[i], chroma));
}
