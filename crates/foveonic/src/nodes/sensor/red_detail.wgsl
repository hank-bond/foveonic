struct Params { shape:vec4<u32>, weights:vec4<i32> }
@group(0) @binding(0) var<storage,read> source:array<f32>;
@group(0) @binding(1) var<uniform> params:Params;
@group(0) @binding(2) var<storage,read_write> horizontal_values:array<i32>;
@group(0) @binding(3) var<storage,read_write> destination:array<f32>;
fn position(x:i32,y:i32)->u32 { return u32(clamp(y,0,i32(params.shape.y)-1))*params.shape.x+u32(clamp(x,0,i32(params.shape.x)-1)); }
fn vertical(x:i32,y:i32)->i32 {
 return (horizontal_values[position(x,y-2)]+4*horizontal_values[position(x,y-1)]+6*horizontal_values[position(x,y)]+4*horizontal_values[position(x,y+1)]+horizontal_values[position(x,y+2)]+8)>>4u;
}
@compute @workgroup_size(8,8)
fn horizontal(@builtin(global_invocation_id) id:vec3<u32>) {
 if id.x>=params.shape.x || id.y>=params.shape.y { return; }
 let x=i32(id.x);let y=i32(id.y);
 horizontal_values[position(x,y)]=(i32(source[position(x-2,y)*3u])+4*i32(source[position(x-1,y)*3u])+6*i32(source[position(x,y)*3u])+4*i32(source[position(x+1,y)*3u])+i32(source[position(x+2,y)*3u])+8)>>4u;
}
@compute @workgroup_size(8,8)
fn apply(@builtin(global_invocation_id) id:vec3<u32>) {
 if id.x>=params.shape.x || id.y>=params.shape.y { return; }
 let x=i32(id.x);let y=i32(id.y);let p=position(x,y)*3u;
 var red=i32(source[p]);
 if x>0 && id.x+1u<params.shape.x {
  // The row callback primes these two samples from the horizontal center row.
  // Subsequent samples consume its vertically filtered stream.
  var previous=vertical(x-1,y);var center=vertical(x,y);
  if x==1 { previous=horizontal_values[position(0,y)];center=horizontal_values[position(1,y)]; }
  if x==2 { previous=horizontal_values[position(1,y)]; }
  let current=vertical(x+1,y);
  let mixed=(previous*params.weights.x+center*(16384-params.weights.x-params.weights.y)+current*params.weights.y)>>14u;
  let factor=i32((params.shape.z<<1u)/3u);
  let delta=((mixed-red)*factor)>>9u;
  red=min(red-delta,32000);red=(red<<16u)>>16u;
 }
 destination[p]=f32(red);destination[p+1u]=source[p+1u];destination[p+2u]=source[p+2u];
}
