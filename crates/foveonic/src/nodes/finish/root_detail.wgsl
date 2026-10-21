// Signed linear→root conversion and five-tap Q12 detail extraction.
struct Params {shape:vec4<u32>,crop:vec4<u32>,weights:vec4<i32>}
@group(0) @binding(0) var<storage,read> source:array<f32>;
@group(0) @binding(1) var<uniform> params:Params;
@group(0) @binding(2) var<storage,read> tables:array<i32>;
@group(0) @binding(3) var<storage,read_write> rgb:array<f32>;
@group(0) @binding(4) var<storage,read_write> mono:array<f32>;
@group(0) @binding(5) var<storage,read_write> vertical:array<f32>;
@group(0) @binding(6) var<storage,read_write> detail:array<f32>;
fn narrow(value:i32)->i32 {return (value<<16u)>>16u;}
// First 65536 entries are a signed-16 root LUT. The tail stores Float32 ramp bits.
fn root(value:i32)->i32 {return tables[u32(value)&65535u];}
fn factor(value:i32)->f32 {if value<0 {return 0.0;} return bitcast<f32>(tables[65536u+u32(min(value,999))]);}
fn index(x:i32,y:i32)->u32 {return u32(clamp(y,i32(params.crop.y),i32(params.crop.w)-1))*params.shape.x+u32(clamp(x,i32(params.crop.x),i32(params.crop.z)-1));}
fn visible(id:vec2<u32>)->bool {return all(id>=params.crop.xy) && all(id<params.crop.zw);}
@compute @workgroup_size(8,8)
fn root_map(@builtin(global_invocation_id)id:vec3<u32>) {
    if id.x>=params.shape.x || id.y>=params.shape.y {return;}
    let p=id.y*params.shape.x+id.x;
    let r=i32(source[p*3u]);let g=i32(source[p*3u+1u]);let b=i32(source[p*3u+2u]);
    let rr=root(r);let gg=root(g);let bb=root(b);
    let correction=(i32(factor(gg-rr)*factor(gg-bb))*params.weights.w)>>14u;
    rgb[p*3u]=f32(narrow(rr));rgb[p*3u+1u]=f32(narrow(gg+correction));rgb[p*3u+2u]=f32(narrow(bb));
    // Fixed Q14 channel mix (approximately 0.35, 0.55, 0.10), rounded before root lookup.
    mono[p]=f32(narrow(root((r*5734+g*9011+b*1638+8192)>>14u)));
}
@compute @workgroup_size(8,8)
fn detail_vertical(@builtin(global_invocation_id)id:vec3<u32>) {
    if id.x>=params.shape.x || id.y>=params.shape.y {return;}
    let p=id.y*params.shape.x+id.x;let x=i32(id.x);let y=i32(id.y);
    let q=params.weights;
    let dx=select(0,1,params.shape.z!=0u);let dy=1-dx;
    let sum=q.x*i32(mono[p])+q.y*(i32(mono[index(x-dx,y-dy)])+i32(mono[index(x+dx,y+dy)]))+q.z*(i32(mono[index(x-2*dx,y-2*dy)])+i32(mono[index(x+2*dx,y+2*dy)]));
    // Q12 rounding and signed-16 narrowing happen after each axis, not only at the end.
    vertical[p]=f32(narrow((sum+2048)>>12u));
}
@compute @workgroup_size(8,8)
fn detail_horizontal(@builtin(global_invocation_id)id:vec3<u32>) {
    if id.x>=params.shape.x || id.y>=params.shape.y {return;}
    let p=id.y*params.shape.x+id.x;
    if !visible(id.xy) {detail[p]=0.0;return;}
    let x=i32(id.x);let y=i32(id.y);let q=params.weights;
    let dx=select(1,0,params.shape.z!=0u);let dy=1-dx;
    let sum=q.x*i32(vertical[p])+q.y*(i32(vertical[index(x-dx,y-dy)])+i32(vertical[index(x+dx,y+dy)]))+q.z*(i32(vertical[index(x-2*dx,y-2*dy)])+i32(vertical[index(x+2*dx,y+2*dy)]));
    detail[p]=f32(narrow(i32(mono[p])-narrow((sum+2048)>>12u)));
}
