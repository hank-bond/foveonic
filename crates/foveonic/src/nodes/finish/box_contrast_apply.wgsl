struct Params { shape:vec4<u32>, controls:vec4<f32> }
@group(0) @binding(0) var<storage,read> source:array<f32>;
@group(0) @binding(1) var<storage,read> horizontal:array<f32>;
@group(0) @binding(2) var<uniform> params:Params;
@group(0) @binding(3) var<storage,read_write> destination:array<f32>;
fn divide(a:f32,b:f32)->f32 { let q=a/b; return fma(1.0,q,fma(-q,b,a)/b); }
@compute @workgroup_size(8,8)
fn box_contrast_apply(@builtin(global_invocation_id) id:vec3<u32>) {
    let width=params.shape.x; let height=params.shape.y; let radius=params.shape.z;
    if id.x>=width || id.y>=height { return; }
    let pixel=id.y*width+id.x;
    for(var ch=0u;ch<3u;ch++) { destination[pixel*3u+ch]=source[pixel*3u+ch]; }
    if id.x<radius || id.y<radius || id.x+radius>width || id.y+radius>height { return; }
    var mean=0.0;
    for(var dy=-i32(radius);dy<=i32(radius);dy++) {
        let y=min(u32(i32(id.y)+dy),height-1u);
        mean=fma(horizontal[y*width+id.x],params.controls.x,mean);
    }
    let signal=fma(source[pixel*3u+2u],0.114,fma(source[pixel*3u],0.299,fma(source[pixel*3u+1u],0.587,0.0)));
    let difference=fma(-1.0,mean,signal);
    let magnitude=max(0.0,fma(-1.0,params.controls.z,abs(difference)));
    let directed=fma(sign(difference),magnitude,0.0);
    let change=fma(params.controls.y,directed,0.0);
    let adjusted=clamp(fma(1.0,signal,change),fma(-1.0,20.0,signal),fma(1.0,20.0,signal));
    let gain=clamp(divide(fma(1.0,adjusted,5.0),max(0.0001,fma(1.0,signal,5.0))),fma(-1.0,params.controls.w,1.0),fma(1.0,params.controls.w,1.0));
    for(var ch=0u;ch<3u;ch++) { destination[pixel*3u+ch]=floor(clamp(fma(gain,source[pixel*3u+ch],0.0),0.0,65534.0)); }
}
