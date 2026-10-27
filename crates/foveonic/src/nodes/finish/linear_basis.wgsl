struct Params { shape: vec4<u32>, rows: array<vec4<f32>,3>, scale: vec4<f32> }
@group(0) @binding(0) var<storage,read> src: array<f32>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage,read_write> dst: array<f32>;
@compute @workgroup_size(8,8)
fn linear_basis(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy>=p.shape.xy) { return; }
    let i=(id.y*p.shape.x+id.x)*3u;
    let v=vec3(src[i],src[i+1u],src[i+2u])*p.scale.x;
    for(var c=0u;c<3u;c++) { dst[i+c]=dot(v,p.rows[c].xyz); }
}
