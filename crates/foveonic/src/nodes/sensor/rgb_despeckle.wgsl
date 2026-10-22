struct Params {extent:vec2<u32>,strength:f32,padding:u32}
@group(0) @binding(0) var<storage,read> pixels:array<f32>;
@group(0) @binding(1) var<storage,read> mask:array<u32>;
@group(0) @binding(2) var<uniform> p:Params;
@group(0) @binding(3) var<storage,read> noise:array<f32>;
@group(0) @binding(4) var<storage,read> matrix:array<vec2<f32>>;
@group(0) @binding(5) var<storage,read_write> output:array<f32>;
fn load(x:u32,y:u32)->vec3<f32>{let i=(y*p.extent.x+x)*3u;return vec3(pixels[i],pixels[i+1u],pixels[i+2u]);}
fn product(a:f32,b:f32)->vec2<f32>{let high=fma(a,b,0.);return vec2(high,fma(a,b,-high));}
fn add_pair(a:vec2<f32>,b:vec2<f32>)->vec2<f32>{let sum=fma(1.,a.x,b.x);let vb=fma(-1.,a.x,sum);let ra=fma(-1.,fma(-1.,vb,sum),a.x);let rb=fma(-1.,vb,b.x);let error=fma(1.,fma(1.,ra,rb),fma(1.,a.y,b.y));let high=fma(1.,sum,error);return vec2(high,fma(-1.,fma(-1.,sum,high),error));}
fn multiply_pair(a:vec2<f32>,b:f32)->vec2<f32>{return add_pair(product(a.x,b),product(a.y,b));}
fn color(v:vec3<f32>,base:u32)->vec3<f32>{var result:vec3<f32>;for(var c=0u;c<3u;c++){let row=base+c*3u;let first=add_pair(multiply_pair(matrix[row+1u],v.y),multiply_pair(matrix[row],v.x));let total=add_pair(first,multiply_pair(matrix[row+2u],v.z));result[c]=fma(1.,total.x,total.y);}return result;}

@compute @workgroup_size(8,8)
fn rgb_despeckle(@builtin(global_invocation_id) id:vec3<u32>){
 let x=id.x;let y=id.y;if x>=p.extent.x||y>=p.extent.y{return;}
 let index=(y*p.extent.x+x)*3u;let center=load(x,y);var result=center;
 if x>=2u&&y>=2u&&x<p.extent.x-2u&&y<p.extent.y-2u&&mask[y*p.extent.x+x]==0u{
    // The native extrema seed is retained even if this corner is masked.
    var low=load(x-2u,y-2u);var high=low;var chroma_low=vec3(3.402823466e38);var chroma_high=-chroma_low;var inner=0u;
    for(var dy=-2;dy<=2;dy++){for(var dx=-2;dx<=2;dx++){
      if (dx==0&&dy==0)||mask[u32(i32(y)+dy)*p.extent.x+u32(i32(x)+dx)]!=0u{continue;}
      let neighbor=load(u32(i32(x)+dx),u32(i32(y)+dy));low=min(low,neighbor);high=max(high,neighbor);
      if abs(dx)<=1&&abs(dy)<=1{let projected=color(neighbor,0u);chroma_low=min(chroma_low,projected);chroma_high=max(chroma_high,projected);inner++;}
    }}
    var sigma:vec3<f32>;for(var c=0u;c<3u;c++){let k=c*3u;let square=fma(center[c],center[c],0.);sigma[c]=p.strength*sqrt(fma(noise[k+2u],square,fma(abs(center[c]),noise[k+1u],noise[k])));}
    let threshold=fma(1.,fma(1.,sigma.x,sigma.y),sigma.z);
    let bright=fma(-1.,high.z,fma(1.,center.z,fma(-1.,high.y,fma(1.,center.y,fma(-1.,high.x,center.x)))));
    let dark=fma(-1.,center.z,fma(1.,low.z,fma(-1.,center.y,fma(1.,low.y,fma(-1.,center.x,low.x)))));
    var candidate=center;var replace=false;if bright>threshold{candidate=high;replace=true;}else if dark>threshold{candidate=low;replace=true;}
    // No valid immediate neighbor leaves no source-defined chroma range.
    if replace&&inner!=0u{var projected=color(candidate,0u);projected.y=clamp(projected.y,chroma_low.y,chroma_high.y);projected.z=clamp(projected.z,chroma_low.z,chroma_high.z);result=color(projected,9u);}
 }
 output[index]=result.x;output[index+1u]=result.y;output[index+2u]=result.z;
}
