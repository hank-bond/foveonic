// Preserve the DNG's linear sample basis while applying an already-resolved
// scalar edit from the selected photographic basis. No camera look is copied.
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<storage, read> before: array<f32>;
@group(0) @binding(2) var<storage, read> after: array<f32>;
@group(0) @binding(3) var<uniform> shape: vec4<u32>;
@group(0) @binding(4) var<storage, read_write> output: array<f32>;
@compute @workgroup_size(8,8)
fn transfer_linear_gain(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= shape.xy) { return; }
    let i=(id.y*shape.x+id.x)*3u;
    let v=vec3(before[i],before[i+1u],before[i+2u]);
    var anchor=0u;
    if abs(v.y)>abs(v.x) { anchor=1u; }
    if abs(v.z)>abs(v[anchor]) { anchor=2u; }
    var gain=1.;
    if v[anchor]!=0. { gain=after[i+anchor]/v[anchor]; }
    for(var c=0u;c<3u;c++) { output[i+c]=source[i+c]*gain; }
}
