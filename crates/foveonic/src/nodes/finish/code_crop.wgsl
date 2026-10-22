struct Params {
    shape: vec4<u32>,
    column: vec4<u32>,
    row: vec4<u32>,
}
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read> weights: array<f32>;
@group(0) @binding(3) var<storage, read_write> destination: array<f32>;

@compute @workgroup_size(8,8)
fn code_crop(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.shape.z || id.y >= params.shape.w { return; }
    // Unsigned arithmetic preserves the native wrapping16.16 coordinate map,
    // including negative increments for right-angle rotations.
    let xx = params.column.x + id.x*params.column.z + id.y*params.row.x;
    let yy = params.column.y + id.x*params.column.w + id.y*params.row.y;
    let out = (id.y*params.shape.z+id.x)*3u;
    if params.row.z != 0u {
        let input = ((yy>>16u)*params.shape.x+(xx>>16u))*3u;
        for (var c=0u;c<3u;c++) { destination[out+c] = source[input+c]; }
        return;
    }
    if xx < 131072u || yy < 131072u || (xx>>12u) >= params.shape.x*16u-32u || (yy>>12u) >= params.shape.y*16u-32u {
        destination[out] = 8000.;
        destination[out+1u] = 0.;
        destination[out+2u] = 0.;
        return;
    }
    let left = (xx>>16u)-1u;
    let top = (yy>>16u)-1u;
    let phase = (((yy>>12u)&15u)*16u+((xx>>12u)&15u))*16u;
    // Share coordinates and weights while retaining each channel's tap order.
    var sum = vec3(0.);
    for (var dy=0u;dy<4u;dy++) {
        for (var dx=0u;dx<4u;dx++) {
            let input = ((top+dy)*params.shape.x+left+dx)*3u;
            let sample = vec3(source[input], source[input+1u], source[input+2u]);
            let weight = weights[phase+dy*4u+dx];
            sum = fma(sample, vec3(weight), sum);
        }
    }
    let result = trunc(clamp(fma(vec3(1.),sum,vec3(0.5)),vec3(0.),vec3(f32(params.row.w))));
    for (var c=0u;c<3u;c++) {
        destination[out+c] = result[c];
    }
}
