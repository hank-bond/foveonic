// Guided log-residual reconstruction on explicit signed linear code inputs.
// Match the host's quantized inverse-exponential table and signal scale.
const LOG_TABLE_LEN: u32 = 245760u;
const LOG_CODE_SCALE: f32 = 23637.115;
// Reference gradient-to-log-table scale, retained at its recovered precision.
const GRADIENT_CODE_SCALE: f32 = 11818.558;
struct Params {shape:vec4<u32>,controls:vec4<f32>,extra:vec4<f32>}
@group(0) @binding(0) var<storage,read> source:array<f32>;
@group(0) @binding(1) var<storage,read> reference:array<f32>;
@group(0) @binding(2) var<uniform> params:Params;
@group(0) @binding(3) var<storage,read> tables:array<f32>;
@group(0) @binding(4) var<storage,read_write> destination:array<f32>;
@group(0) @binding(5) var<storage,read_write> guidance:array<f32>;
@group(0) @binding(6) var<storage,read_write> scratch:array<f32>;
@group(0) @binding(7) var<storage,read_write> statistics:array<f32>;
fn clipped(v:f32)->u32 {return u32(clamp(i32(v),1,i32(params.extra.x)-1));}
@compute @workgroup_size(8,8)
fn prepare(@builtin(global_invocation_id)id:vec3<u32>) {
 if id.x>=params.shape.x || id.y>=params.shape.y {return;}
 let p=(id.y*params.shape.x+id.x)*3u;let n=u32(params.extra.x);
 for(var c=0u;c<3u;c++) {let a=clipped(source[p+c]);let b=clipped(reference[p+c]);destination[p+c]=tables[b];guidance[p+c]=tables[n+a]-tables[n+b];}
}
@compute @workgroup_size(8,8)
fn gradient(@builtin(global_invocation_id)id:vec3<u32>) {
 let w=params.shape.x;let h=params.shape.y;if id.x>=w || id.y>=h {return;}
 let p=id.y*w+id.x;
 if id.y==0u || id.y>=h-2u {destination[p]=0.0;return;}
 let x=clamp(id.x,1u,w-2u);let center=id.y*w+x;var sum=0.0;
 for(var c=0u;c<3u;c++) {let d=source[(center-1u)*3u+c]-source[(center+1u)*3u+c];sum+=d*d;}
 for(var c=0u;c<3u;c++) {let d=source[(center+w)*3u+c]-source[(center-w)*3u+c];sum+=d*d;}
 destination[p]=sqrt(sum);
}
fn compensated(sum:f32,error:f32,value:f32)->vec2<f32> {let corrected=value-error;let next=sum+corrected;return vec2(next,(next-sum)-corrected);}
@compute @workgroup_size(64)
fn row_statistics(@builtin(global_invocation_id)id:vec3<u32>) {
 let y=id.x;let w=params.shape.x;if y>=params.shape.y {return;}
 var total=vec2(0.0);var squared=vec2(0.0);
 for(var x=0u;x<w;x++) {let v=source[y*w+x];total=compensated(total.x,total.y,v);squared=compensated(squared.x,squared.y,v*v);}
 statistics[y*4u]=total.x;statistics[y*4u+1u]=total.y;statistics[y*4u+2u]=squared.x;statistics[y*4u+3u]=squared.y;
}
@compute @workgroup_size(1)
fn reduce_statistics() {
 var total=vec2(0.0);var squared=vec2(0.0);
 for(var y=0u;y<params.shape.y;y++) {total=compensated(total.x,total.y,statistics[y*4u]-statistics[y*4u+1u]);squared=compensated(squared.x,squared.y,statistics[y*4u+2u]-statistics[y*4u+3u]);}
 let count=f32(params.shape.x*params.shape.y);
 let deviation=sqrt(max(0.0,(squared.x-total.x*total.x/count)/(count-1.0)));
 destination[0]=deviation;
}
@compute @workgroup_size(8,8)
fn edge_guide(@builtin(global_invocation_id)id:vec3<u32>) {
 if id.x>=params.shape.x || id.y>=params.shape.y {return;}
 let deviation=statistics[0];
 let p=id.y*params.shape.x+id.x;
 let ordinate=f32(clamp(i32(source[p]*GRADIENT_CODE_SCALE),0,i32(LOG_TABLE_LEN)-1));
 var t=-1.0;if deviation>0.0 {t=tanh((ordinate/-LOG_CODE_SCALE)/(deviation*params.controls.y*0.5));}
 destination[p]=1.0/(exp(-((t+1.0)-params.controls.z)*params.controls.w)+1.0);
}
fn guide_at(x:u32,y:u32)->f32 {if (params.shape.w&4u)!=0u {return reference[x*params.shape.y+y];}return reference[y*params.shape.x+x];}
@compute @workgroup_size(64)
fn forward(@builtin(global_invocation_id)id:vec3<u32>) {
 // Transposed scratch keeps adjacent row workers writing adjacent records.
 let y=id.x;let w=params.shape.x;let h=params.shape.y;let channels=params.shape.z;
 if y>=h || ((params.shape.w&2u)!=0u && y==h-1u) {return;}
 var sums=vec3(0.0);var weight=0.0;
 for(var x=0u;x<w;x++) {
  let p=y*w+x;var decay=params.controls.x;if (params.shape.w&1u)!=0u {decay*=guide_at(x,y);}
  for(var c=0u;c<channels;c++) {if channels==1u {sums[c]=fma(sums[c],decay,source[p*channels+c]);}else{sums[c]=sums[c]*decay+source[p*channels+c];}scratch[(x*h+y)*4u+c]=sums[c];}
  weight=fma(weight,decay,1.0);scratch[(x*h+y)*4u+3u]=weight;
 }
}
@compute @workgroup_size(64)
fn backward(@builtin(global_invocation_id)id:vec3<u32>) {
 let y=id.x;let w=params.shape.x;let h=params.shape.y;let channels=params.shape.z;
 if y>=h || ((params.shape.w&2u)!=0u && y==h-1u) {return;}
 var sums=vec3(0.0);var weight=0.0;
 for(var xx=i32(w)-1;xx>=0;xx--) {
  let x=u32(xx);let p=y*w+x;var decay=params.controls.x;if (params.shape.w&1u)!=0u {decay*=guide_at(x,y);}
  var left_weight=0.0;if x>0u {left_weight=scratch[((x-1u)*h+y)*4u+3u];}
  let inverse=1.0/params.controls.x;let denominator=(left_weight+weight)+inverse;
  for(var c=0u;c<channels;c++) {
   var left=0.0;if x>0u {left=scratch[((x-1u)*h+y)*4u+c];}
   let center=source[p*channels+c];var numerator=(left+sums[c])+center*inverse;
   if channels==1u {numerator=fma(center,inverse,left+sums[c]);}
   var result=numerator/denominator;
   if (params.shape.w&1u)==0u {result=numerator*(1.0/denominator);}
   destination[(x*h+y)*channels+c]=result;
   if channels==1u {sums[c]=fma(sums[c],decay,center);}else{sums[c]=sums[c]*decay+center;}
  }
  weight=fma(weight,decay,1.0);
 }
}
@compute @workgroup_size(8,8)
fn combine(@builtin(global_invocation_id)id:vec3<u32>) {if id.x>=params.shape.x || id.y>=params.shape.y {return;}let p=id.y*params.shape.x+id.x;destination[p]=max(source[p],reference[p]);}
@compute @workgroup_size(8,8)
fn finalize(@builtin(global_invocation_id)id:vec3<u32>) {
 if id.x>=params.shape.x || id.y>=params.shape.y {return;}
 let p=id.y*params.shape.x+id.x;let n=u32(params.extra.x);let strength=i32(params.extra.y);
 let r=clipped(reference[p*3u]);let g=clipped(reference[p*3u+1u]);let b=clipped(reference[p*3u+2u]);let brightness=(r+2u*g+b)>>2u;
 let blend=i32((max(1.0-guidance[p],0.0)*tables[n*2u+LOG_TABLE_LEN+brightness])*f32(strength));
 for(var c=0u;c<3u;c++) {
  let old=i32(clipped(source[p*3u+c]));let filtered=clipped(reference[p*3u+c]);
  let j=u32(clamp(i32((tables[n+filtered]+scratch[p*3u+c])*-LOG_CODE_SCALE),0,i32(LOG_TABLE_LEN)-1));
  let recovered=i32(tables[n*2u+j]);let value=(recovered*strength+(2*old-recovered)*blend)>>13u;
  destination[p*3u+c]=f32(clamp(value,-500,24000));
 }
}
