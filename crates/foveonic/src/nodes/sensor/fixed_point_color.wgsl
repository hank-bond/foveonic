// Two channel-specific nonlinear corrections followed by a signed fixed-point matrix.
struct Params {source:vec4<u32>,matrix:array<vec4<i32>,3>,bounds:vec4<i32>};
@group(0) @binding(0) var<storage,read> input_pixels:array<f32>;
@group(0) @binding(1) var<storage,read> tables:array<i32>;
@group(0) @binding(2) var<uniform> params:Params;
@group(0) @binding(3) var<storage,read_write> output_pixels:array<f32>;
@group(0) @binding(4) var<storage,read_write> corrected_layers:array<f32>;
fn lookup(c:u32,value:i32)->i32{
    let limit=i32(params.source.z/2u)-1;
    // The power-of-two table stores negative signed codes at its upper indices.
    let index=bitcast<u32>(clamp(value,-limit,limit))&(params.source.z-1u);
    return tables[c*params.source.z+index];
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3<u32>){
    if id.x>=params.source.x || id.y>=params.source.y{return;}
    let base=(id.y*params.source.x+id.x)*3u;
    var a=vec3<i32>(0);
    for(var c=0u;c<3u;c++){let s=i32(input_pixels[base+c]);a[c]=s-lookup(c,s);}
    // Reference correction uses a 1:2:1 channel proxy, not display luminance.
    let luma=(a.x+2*a.y+a.z)>>2u;
    var b=vec3<i32>(0);
    for(var c=0u;c<3u;c++){
        b[c]=a[c]-lookup(c,a[c]-luma);
        corrected_layers[base+c]=f32((b[c]<<16u)>>16u);
    }
    for(var c=0u;c<3u;c++){
        let row=params.matrix[c];
        // Add half a matrix unit before the arithmetic shift.
        let value=(row.x*b.x+row.y*b.y+row.z*b.z+(1i<<(params.source.w-1u)))>>params.source.w;
        output_pixels[base+c]=f32(clamp(value,params.bounds.x,params.bounds.y));
    }
}
