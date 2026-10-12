// Low-level ordered camera-record replay uses camera_common.wgsl arithmetic.
struct Params { source: vec4<u32> };
@group(0) @binding(1) var<storage,read> records:array<vec2<u32>>;
@group(0) @binding(2) var<uniform> params:Params;
@group(0) @binding(3) var<storage,read_write> pixels:array<f32>;
@compute @workgroup_size(1)
fn main(){for(var i=0u;i<params.source.z;i++){camera_correct(records[i],params.source.x,params.source.y);}}
