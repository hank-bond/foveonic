struct Params { shape: vec4<u32>, controls: vec4<u32> }
@group(0) @binding(0) var<storage, read> source: array<u32>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage, read_write> destination: array<u32>;

@compute @workgroup_size(8, 8)
fn pool_signal(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= p.shape.z || id.y >= p.shape.w { return; }
    let divisor = p.controls.x;
    let components = p.controls.y;
    for (var channel = 0u; channel < components; channel++) {
        var total = 0.;
        var coverage = 0u;
        for (var y = 0u; y < divisor; y++) {
            for (var x = 0u; x < divisor; x++) {
                let index = ((id.y * divisor + y) * p.shape.x + id.x * divisor + x) * components + channel;
                if p.controls.z != 0u { coverage = max(coverage, source[index]); }
                else { total = total + bitcast<f32>(source[index]); }
            }
        }
        let index = (id.y * p.shape.z + id.x) * components + channel;
        if p.controls.z != 0u { destination[index] = coverage; }
        else { destination[index] = bitcast<u32>(total / f32(divisor * divisor)); }
    }
}
