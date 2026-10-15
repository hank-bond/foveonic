// Native mono high-pass and quarter-code brightness-proportional final sharpening.
// Adapted with owner permission from Foveonic sharpen.rs at e47af35.
struct Controls{source:vec4<u32>,value:vec4<f32>}
@group(0) @binding(0) var<storage,read> rgb:array<f32>;
@group(0) @binding(1) var<storage,read> detail:array<f32>;
@group(0) @binding(2) var<uniform> params:Controls;
@group(0) @binding(3) var<storage,read_write> output_pixels:array<f32>;
fn color(p:u32)->vec3<f32>{return vec3(rgb[p*3u],rgb[p*3u+1u],rgb[p*3u+2u]);}
fn brightness(p:u32)->f32{let v=max(color(p),vec3(0.0));return min((v.x+v.y)+v.z,32767.0);}
@compute @workgroup_size(8,8)
fn signal(@builtin(global_invocation_id)id:vec3<u32>){
    let w=params.source.x;let h=params.source.y;if id.x>=w || id.y>=h{return;}
    let p=id.y*w+id.x;var value=0.0;
    if id.x>=2u && id.x+2u<w && id.y>=2u && id.y+2u<h{
        var sum=0.0;for(var y=-1;y<=1;y++){for(var x=-1;x<=1;x++){sum+=brightness(u32(i32(p)+y*i32(w)+x));}}
        value=brightness(p)-sum/9.0;
    }
    output_pixels[p]=value;
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id)id:vec3<u32>){
    let w=params.source.x;let h=params.source.y;if id.x>=w || id.y>=h{return;}
    let p=id.y*w+id.x;let positive=max(color(p),vec3(0.0));let sum=(positive.x+positive.y)+positive.z;
    var fractions=vec3(0.0);if sum!=0.0{fractions=positive/sum;}
    // Native upper-only ceiling, half-up addition followed by truncation, then i16 wrap.
    let integer=i32(fma(min(detail[p],8191.0),4.0,0.5));let low=u32(integer)&65535u;let quarter_signal=i32(low<<16u)>>16;
    let delta=clamp(clamp((f32(quarter_signal)*0.25)*params.value.x,-1000.0,1000.0),-0.35*sum,0.35*sum);
    let value=min(fractions*(sum+delta),vec3(32767.0));
    output_pixels[p*3u]=value.x;output_pixels[p*3u+1u]=value.y;output_pixels[p*3u+2u]=value.z;
}
