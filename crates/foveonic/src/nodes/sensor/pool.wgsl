struct Params { shape: vec4<u32>, controls: vec4<u32>, origin: vec4<u32> }
@group(0) @binding(0) var<storage, read> source: array<u32>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage, read_write> destination: array<u32>;

@compute @workgroup_size(8, 8)
fn pool_signal(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= p.origin.zw) { return; }
    let cell = id.xy;
    // Each edge cell owns its adjacent halo, so its reduction runs only once.
    let output_first = select(p.origin.xy + cell, vec2<u32>(0), cell == vec2<u32>(0));
    let output_end = select(p.origin.xy + cell + vec2<u32>(1), p.shape.zw, cell + vec2<u32>(1) == p.origin.zw);
    let first = cell * p.controls.x;
    let end = min(p.shape.xy, first + vec2(p.controls.x));
    if p.controls.x == 1u {
        let input = (first.y * p.shape.x + first.x) * p.controls.y;
        for (var y = output_first.y; y < output_end.y; y++) {
            for (var x = output_first.x; x < output_end.x; x++) {
                let output = (y * p.shape.z + x) * p.controls.y;
                for (var c = 0u; c < p.controls.y; c++) { destination[output+c] = source[input+c]; }
            }
        }
        return;
    }
    if p.controls.z == 1u || p.controls.z == 2u {
        var category = 0u;
        for (var y = first.y; y < end.y; y++) {
            for (var x = first.x; x < end.x; x++) {
                let value = source[y * p.shape.x + x];
                if p.controls.z == 1u { category = max(category, value); }
                else { category |= value; }
            }
        }
        for (var y = output_first.y; y < output_end.y; y++) {
            for (var x = output_first.x; x < output_end.x; x++) {
                destination[y * p.shape.z + x] = category;
            }
        }
        return;
    }
    // Keep the same row-mean then column-mean arithmetic for RGB and blue.
    // RGB components share loads and addressing instead of separate channel loops.
    var total = vec3(0.);
    for (var y = first.y; y < end.y; y++) {
        var row = vec3(0.);
        for (var x = first.x; x < end.x; x++) {
            let input = (y * p.shape.x + x) * p.controls.y;
            if p.controls.y == 3u {
                row += bitcast<vec3<f32>>(vec3(source[input], source[input+1u], source[input+2u]));
            } else { row.x += bitcast<f32>(source[input]); }
        }
        total += row / f32(end.x - first.x);
    }
    var mean = total / f32(end.y - first.y);
    if p.controls.z == 3u { mean = sign(mean) * floor(abs(mean) + vec3(0.5)); }
    for (var y = output_first.y; y < output_end.y; y++) {
        for (var x = output_first.x; x < output_end.x; x++) {
            let output = (y * p.shape.z + x) * p.controls.y;
            destination[output] = bitcast<u32>(mean.x);
            if p.controls.y == 3u {
                destination[output+1u] = bitcast<u32>(mean.y);
                destination[output+2u] = bitcast<u32>(mean.z);
            }
        }
    }
}
