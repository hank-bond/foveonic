struct Params { shape:vec4<u32>, matrix:array<vec4<f32>,6> }
@group(0) @binding(0) var<storage,read> source:array<f32>;
@group(0) @binding(1) var<uniform> params:Params;
@group(0) @binding(2) var<storage,read_write> destination:array<f32>;

fn product(a:f32,b:f32)->vec2<f32>{let h=fma(a,b,0.);return vec2<f32>(h,fma(a,b,-h));}
fn add(a:vec2<f32>,b:vec2<f32>)->vec2<f32>{
 let s=fma(1.,a.x,b.x);let v=fma(-1.,a.x,s);
 let e=fma(1.,fma(-1.,fma(-1.,v,s),a.x),fma(-1.,v,b.x));
 let t=fma(1.,e,fma(1.,a.y,b.y));let h=fma(1.,s,t);
 return vec2<f32>(h,fma(-1.,fma(-1.,s,h),t));
}
fn multiply(a:vec2<f32>,b:f32)->vec2<f32>{return add(product(a.x,b),product(a.y,b));}
fn transform(v:vec3<f32>)->vec3<f32>{
 var out:vec3<f32>;
 for(var r=0u;r<3u;r++){
  let hi=params.matrix[r*2u];let lo=params.matrix[r*2u+1u];
  var value=multiply(vec2<f32>(hi.y,lo.y),v.y);
  value=add(value,multiply(vec2<f32>(hi.x,lo.x),v.x));
  value=add(value,multiply(vec2<f32>(hi.z,lo.z),v.z));
  out[r]=fma(1.,value.x,value.y);
 }
 return out;
}
fn forward_curve(v:f32)->f32{
 if v>=0.008856452{return pow(v,1./3.);}
 return fma(v,7.787037,0.13793103);
}
fn inverse_curve(v:f32)->f32{
 if v>=0.20689656{return fma(fma(v,v,0.),v,0.);}
 return fma(fma(1.,v,-0.13793103),0.12841855,0.);
}
@compute @workgroup_size(8,8)
fn packed_lab(@builtin(global_invocation_id) id:vec3<u32>){
 if id.x>=params.shape.x||id.y>=params.shape.y{return;}
 let index=(id.y*params.shape.x+id.x)*3u;
 let v=vec3<f32>(source[index],source[index+1u],source[index+2u]);
 var result:vec3<f32>;
 if params.shape.z==0u{
  var xyz=transform(v*6.103888e-5);
  xyz.x=xyz.x*1.0521266;xyz.z=xyz.z*0.9184811;
  xyz=vec3<f32>(forward_curve(xyz.x),forward_curve(xyz.y),forward_curve(xyz.z));
  result.x=fma(1.,fma(fma(xyz.y,116.,-16.),81.915,0.),0.5);
  result.y=fma(1.,fma(1.,fma(fma(fma(1.,xyz.x,-xyz.y),500.,0.),27.305,0.),32767.),0.5);
  result.z=fma(1.,fma(1.,fma(fma(fma(1.,xyz.y,-xyz.z),200.,0.),54.61,0.),32767.),0.5);
  for(var c=0u;c<3u;c++){result[c]=f32(u32(i32(result[c]))&65535u);}
 }else{
  let y=fma(fma(1.,fma(v.x,0.012207776,0.),16.),0.00862069,0.);
  let x=fma(fma(fma(1.,v.y,-32767.),0.03662333,0.),0.002,y);
  let z=fma(fma(fma(1.,v.z,-32767.),-0.018311664,0.),0.005,y);
  let xyz=vec3<f32>(inverse_curve(x)*0.950456,inverse_curve(y),inverse_curve(z)*1.088754);
  result=transform(xyz);
  for(var c=0u;c<3u;c++){result[c]=f32(u32(fma(1.,clamp(fma(result[c],16383.,0.),0.,65534.),0.5)));}
 }
 destination[index]=result.x;destination[index+1u]=result.y;destination[index+2u]=result.z;
}
