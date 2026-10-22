struct Params{shape:vec4<u32>,controls:vec4<f32>,limits:vec4<f32>}
@group(0) @binding(0) var<storage,read> rgb:array<f32>;
@group(0) @binding(1) var<storage,read> detail:array<f32>;
@group(0) @binding(2) var<uniform> p:Params;
@group(0) @binding(3) var<storage,read> table:array<f32>;
@group(0) @binding(4) var<storage,read_write> output:array<f32>;
fn product(a:f32,b:f32)->vec2<f32>{let high=fma(a,b,0.);return vec2(high,fma(a,b,-high));}
fn add_pair(a:vec2<f32>,b:vec2<f32>)->vec2<f32>{let sum=fma(1.,a.x,b.x);let vb=fma(-1.,a.x,sum);let ra=fma(-1.,fma(-1.,vb,sum),a.x);let rb=fma(-1.,vb,b.x);let error=fma(1.,fma(1.,ra,rb),fma(1.,a.y,b.y));let high=fma(1.,sum,error);return vec2(high,fma(-1.,fma(-1.,sum,high),error));}
fn multiply_pair(a:vec2<f32>,b:f32)->vec2<f32>{return add_pair(product(a.x,b),product(a.y,b));}

// Metal's quotient may differ by one ulp from the source Float32 division.
fn divide(a:f32,b:f32)->f32{let q=a/b;return fma(1.,q,fma(-q,b,a)/b);}
fn store_code(original:f32,factor:f32)->f32{
 var value=product(original,factor);
 if value.x>65534.||(value.x==65534.&&value.y>0.){return select(original,65534.,original==65535.);}
 if value.x<0.||(value.x==0.&&value.y<0.){return 0.;}
 value=add_pair(value,vec2(0.5,0.));var rounded=floor(value.x);if value.x==rounded&&value.y<0.{rounded=rounded-1.;}return clamp(rounded,0.,65535.);
}
@compute @workgroup_size(8,8)
fn proportional_sharpen(@builtin(global_invocation_id) id:vec3<u32>){
 if id.x>=p.shape.x||id.y>=p.shape.y{return;}let n=id.y*p.shape.x+id.x;let i=n*3u;let source=vec3(rgb[i],rgb[i+1u],rgb[i+2u]);let adapted=vec3(divide(source.x,p.controls.x),divide(source.y,p.controls.x),divide(source.z,p.controls.x));
 let total=fma(1.,fma(1.,adapted.z,adapted.x),adapted.y);let lookup=fma(total,0.33,0.);let base=floor(lookup);let rounded=base+f32(lookup-base>=0.5);let index=u32(clamp(rounded,0.,f32(p.shape.z-1u)));let amount=table[index]*p.controls.y;
 let delta=detail[n]*p.controls.z;
 let green=product(source.y,0.587);let red=add_pair(green,product(source.x,0.299));let sum=add_pair(red,product(source.z,0.114));let luma=fma(1.,sum.x,sum.y);
 var change=sign(delta)*min(p.limits.x,min(abs(delta)*amount,p.limits.y*luma));if p.shape.w!=0u&&luma<=p.controls.w{change=0.;}
 let factor=fma(1.,divide(change,max(1.,luma)),1.);output[i]=store_code(source.x,factor);output[i+1u]=store_code(source.y,factor);output[i+2u]=store_code(source.z,factor);
}
