// Physical RGB scratch samples retained before color shading.
const SCRATCH_X_STEP=16u;
const SCRATCH_Y_STEP=4u;
struct Params {shape:vec4<u32>,special:vec4<u32>,controls:vec4<f32>}
struct Gain {ordinary:vec4<f32>,mono:vec4<f32>,red_green:vec4<f32>,blue:vec4<f32>,phases:vec4<f32>}
@group(0) @binding(0) var<storage,read> rgb:array<f32>;
@group(0) @binding(1) var<storage,read> blue:array<f32>;
@group(0) @binding(2) var<uniform> p:Params;
@group(0) @binding(3) var<storage,read> lut:array<u32>;
@group(0) @binding(4) var<storage,read> gains:array<Gain>;
@group(0) @binding(5) var<storage,read_write> out_rgb:array<f32>;
@group(0) @binding(6) var<storage,read_write> out_blue:array<f32>;
@group(0) @binding(7) var<storage,read_write> scratch:array<f32>;
fn multiply_pair(value:f32,gain:vec2<f32>)->f32 {
    let product=value*gain.x;
    let error=fma(value,gain.x,-product);
    return product+(error+value*gain.y);
}
@compute @workgroup_size(8,8)
fn hlp_calibration(@builtin(global_invocation_id) id:vec3<u32>){
    let w=p.shape.x;let h=p.shape.y;if id.x>=w||id.y>=h{return;}
    let offset=(id.y*w+id.x)*3u;
    var color=vec3<f32>(rgb[offset],rgb[offset+1u],rgb[offset+2u]);
    let first=(id.y*2u)*(w*2u)+id.x*2u;
    var phases=vec4<f32>(blue[first],blue[first+1u],blue[first+w*2u],blue[first+w*2u+1u]);
    let sparse=id.x>=p.special.x&&id.y>=p.special.y&&(id.x-p.special.x)%p.special.z==0u&&(id.y-p.special.y)%p.special.w==0u;
    var gain:Gain;
    if sparse {
        let ix=(id.x-p.special.x)/p.special.z;let iy=(id.y-p.special.y)/p.special.w;
        gain=gains[iy*p.shape.z+ix];
        var codes:vec4<f32>;
        for(var c=0u;c<4u;c++) {let v=max(phases[c]*p.controls.x,0.);codes[c]=f32(lut[min(u32(v),32767u)]);phases[c]=p.controls.y*codes[c];}
        let total=((codes.x+codes.y)+codes.z)+codes.w;
        color.b=(p.controls.y*total)*0.25;
        color.b=multiply_pair(color.b,gain.ordinary.xy);
        phases=phases*gain.mono.x;
    }
    if id.x>0u&&id.y>0u&&id.x%SCRATCH_X_STEP==0u&&id.y%SCRATCH_Y_STEP==0u {
        let i=((id.y/SCRATCH_Y_STEP-1u)*((w-1u)/SCRATCH_X_STEP)+(id.x/SCRATCH_X_STEP-1u))*3u;
        scratch[i]=color.r;scratch[i+1u]=color.g;scratch[i+2u]=color.b;
    }
    if sparse {
        color.b=multiply_pair(color.b,gain.ordinary.zw);
        phases=phases*gain.mono.y;
        color.r=multiply_pair(color.r,gain.red_green.xy);
        color.g=multiply_pair(color.g,gain.red_green.zw);
        color.b=multiply_pair(color.b,gain.blue.xy);
        phases=phases*gain.phases;
    }
    phases=min(phases,vec4<f32>(p.controls.z));
    out_rgb[offset]=color.r;out_rgb[offset+1u]=color.g;out_rgb[offset+2u]=color.b;
    out_blue[first]=phases.x;out_blue[first+1u]=phases.y;out_blue[first+w*2u]=phases.z;out_blue[first+w*2u+1u]=phases.w;
}
