// Keep DNG samples linear. One shared range preserves channel ratios and leaves
// needed highlight headroom instead of baking a shoulder into the samples.
struct Basis { rows: array<vec4<f32>, 3>, neutral: vec4<f32> };
@group(0) @binding(2) var<storage, read_write> camera: array<f32>;
@group(0) @binding(3) var<storage, read_write> statistics: array<atomic<u32>>;
@group(0) @binding(4) var<uniform> basis: Basis;
@group(0) @binding(5) var<storage, read_write> integers: array<u32>;

@compute @workgroup_size(8,8)
fn convert(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= params.output_size.xy) { return; }
    let rgb=sample_rgb(id.xy);
    let index=(id.y*params.output_size.x+id.x)*3u;
    for (var c=0u; c<3u; c++) {
        let v=dot(basis.rows[c].xyz,rgb);
        if !(abs(v)<=3.402823e38) { atomicAdd(&statistics[1],1u); }
        camera[index+c]=max(v,0.0);
        // As-shot WB changes where each channel reaches camera white in the reader.
        // Measure range against that neutral, not just the largest RGB component.
        atomicMax(&statistics[0],bitcast<u32>(max(v,0.0)/basis.neutral[c]));
    }
}
@compute @workgroup_size(8,8)
fn quantize(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= params.output_size.xy) { return; }
    let pixel=id.y*params.output_size.x+id.x;
    let scale=basis.neutral.w;
    var v: vec3<u32>;
    for (var c=0u; c<3u; c++) {
        v[c]=u32(clamp(floor(camera[pixel*3u+c]/scale*65535.0+0.5),0.0,65535.0));
    }
    integers[pixel*2u]=v.r|(v.g<<16u);
    integers[pixel*2u+1u]=v.b;
}
