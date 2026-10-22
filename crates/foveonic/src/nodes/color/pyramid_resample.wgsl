struct Params { shape: vec4<u32>, controls: vec4<u32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read_write> destination: array<f32>;

@compute @workgroup_size(8, 8)
fn pyramid_resample(@builtin(global_invocation_id) id: vec3<u32>) {
    let vertical = params.controls.y != 0u;
    let doubling = params.controls.x != 0u;
    let out_width = params.shape.z;
    let out_height = select(params.shape.y, params.shape.w, vertical);
    if id.x >= out_width || id.y >= out_height { return; }
    let coordinate = select(id.x, id.y, vertical);
    let source_length = select(params.shape.x, params.shape.y, vertical);
    let source_width = select(params.shape.x, out_width, vertical);

    // ComputeResampleParams at scale .5 has one four-tap phase advancing by two.
    // At scale 2 it alternates two-tap phases, advancing by one then zero.
    // Both use origin -1 and replicate source edges in each separable pass.
    var start = i32(coordinate * 2u) - 1;
    var count = 4u;
    var weights = vec4<f32>(0.125, 0.375, 0.375, 0.125);
    if doubling {
        start = i32((coordinate + 1u) / 2u) - 1;
        count = 2u;
        weights = select(vec4<f32>(0.25, 0.75, 0.0, 0.0), vec4<f32>(0.75, 0.25, 0.0, 0.0), (coordinate % 2u) != 0u);
    }
    var total = vec3<f32>(0.0);
    for (var k = 0u; k < count; k++) {
        let position = u32(clamp(start + i32(k), 0, i32(source_length) - 1));
        let x = select(position, id.x, vertical);
        let y = select(id.y, position, vertical);
        let i = (y * source_width + x) * 3u;
        let value = vec3<f32>(source[i], source[i+1u], source[i+2u]);
        // Each product and addition round separately in the native Float32 loop.
        let term = fma(value, vec3<f32>(weights[k]), vec3<f32>(0.0));
        total = fma(vec3<f32>(1.0), total, term);
    }
    let out = (id.y * out_width + id.x) * 3u;
    destination[out] = total.x;
    destination[out+1u] = total.y;
    destination[out+2u] = total.z;
}
