struct Params { shape: vec4<u32>, controls: vec4<u32>, origin: vec4<u32> }
@group(0) @binding(0) var<storage, read> source: array<u32>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage, read_write> destination: array<u32>;

fn aggregate(id: vec2<u32>, vertical: bool) {
    let out_width = p.origin.z;
    let out_height = select(p.shape.y, p.origin.w, vertical);
    if id.x >= out_width || id.y >= out_height { return; }
    let coordinate = select(id.x, id.y, vertical);
    let length = select(p.shape.x, p.shape.y, vertical);
    let first = coordinate * p.controls.x;
    let end = min(length, first + p.controls.x);
    let source_width = select(p.shape.x, out_width, vertical);
    for (var channel = 0u; channel < p.controls.y; channel++) {
        var total = 0.;
        var flags = 0u;
        for (var position = first; position < end; position++) {
            let x = select(position, id.x, vertical);
            let y = select(id.y, position, vertical);
            let value = source[(y * source_width + x) * p.controls.y + channel];
            if p.controls.z == 1u { flags = max(flags, value); }
            else if p.controls.z == 2u { flags = flags | value; }
            else { total = total + bitcast<f32>(value); }
        }
        let index = (id.y * out_width + id.x) * p.controls.y + channel;
        if p.controls.z != 0u { destination[index] = flags; }
        else { destination[index] = bitcast<u32>(total / f32(end - first)); }
    }
}
@compute @workgroup_size(8, 8)
fn pool_signal_horizontal(@builtin(global_invocation_id) id: vec3<u32>) { aggregate(id.xy, false); }
@compute @workgroup_size(8, 8)
fn pool_signal_vertical(@builtin(global_invocation_id) id: vec3<u32>) { aggregate(id.xy, true); }
@compute @workgroup_size(8, 8)
fn pool_signal_pad(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= p.shape.z || id.y >= p.shape.w { return; }
    let x = u32(clamp(i32(id.x) - i32(p.origin.x), 0, i32(p.origin.z) - 1));
    let y = u32(clamp(i32(id.y) - i32(p.origin.y), 0, i32(p.origin.w) - 1));
    for (var channel = 0u; channel < p.controls.y; channel++) {
        destination[(id.y * p.shape.z + id.x) * p.controls.y + channel] = source[(y * p.origin.z + x) * p.controls.y + channel];
    }
}
