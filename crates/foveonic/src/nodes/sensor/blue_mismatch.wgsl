struct Params{extent:vec4<u32>,rectangle:vec4<u32>,controls:vec4<f32>}
@group(0) @binding(0) var<storage,read> pixels:array<f32>;
@group(0) @binding(1) var<uniform> p:Params;
@group(0) @binding(2) var<storage,read_write> output:array<f32>;
@group(0) @binding(3) var<storage,read_write> status:atomic<u32>;
fn eligible(x:u32,y:u32)->bool{let index=y*p.extent.x+x;let before=pixels[index-p.extent.x];let after=pixels[index+p.extent.x];let center=pixels[index];let average=fma(1.,before,after)*0.5;return (pixels[index-2u]>p.controls.x||pixels[index-1u]>p.controls.x)&&pixels[index+2u]<p.controls.x&&before<p.controls.x&&after<p.controls.x&&center<before&&center<after&&fma(-1.,center,average)<p.controls.y;}
@compute @workgroup_size(64)
fn blue_mismatch(@builtin(global_invocation_id) id:vec3<u32>){let x=p.rectangle.x+id.x*2u;if x>=p.rectangle.z{return;}
 // Native new[] counters are uninitialized. An ineligible first row writes zero.
 if eligible(x,p.rectangle.y){atomicStore(&status,1u);return;}
 var count=0u;
 // Only odd samples are repaired, always below the threshold. Earlier horizontal
 // writes cannot change the brightness predicate, so columns are independent.
 for(var y=p.rectangle.y+2u;y<p.rectangle.w;y+=2u){if !eligible(x,y){count=0u;continue;}let index=y*p.extent.x+x;
  if count>=2u{output[index]=fma(1.,pixels[index-p.extent.x],pixels[index+p.extent.x])*0.5;}
  if count==2u{output[index-4u*p.extent.x]=fma(1.,pixels[index-5u*p.extent.x],pixels[index-3u*p.extent.x])*0.5;output[index-2u*p.extent.x]=fma(1.,pixels[index-3u*p.extent.x],pixels[index-p.extent.x])*0.5;}
  count=(count+1u)&65535u;
 }
}
