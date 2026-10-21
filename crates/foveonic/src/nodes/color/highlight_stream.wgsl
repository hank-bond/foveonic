// Preserve the reference five-row stream, including its unflushed final rows.
const INITIALIZE:u32=0u;
const WORKGROUP_THREADS:u32=256u;
// size.w selects parallel initialization or the ordered delayed sweep.
struct Params { size:vec4<u32>, fix:vec4<i32>, apriori:vec4<i32>, scaled:vec4<i32> }
@group(0) @binding(0) var<uniform> params:Params;
@group(0) @binding(1) var<storage,read> input_rgb:array<f32>;
@group(0) @binding(2) var<storage,read_write> rgb:array<f32>;
@group(0) @binding(3) var<storage,read_write> history:array<atomic<u32>>;
@group(0) @binding(4) var<storage,read_write> scratch:array<i32>;
fn narrow(v:i32)->i32 {return (v<<16u)>>16u;}
fn read_rgb(x:u32,y:u32)->vec3<i32> {let p=(y*params.size.x+x)*3u;return vec3<i32>(i32(rgb[p]),i32(rgb[p+1u]),i32(rgb[p+2u]));}
fn put_rgb(x:u32,y:u32,v:vec3<i32>) {let p=(y*params.size.x+x)*3u;rgb[p]=f32(narrow(v.x));rgb[p+1u]=f32(narrow(v.y));rgb[p+2u]=f32(narrow(v.z));}
fn marked(x:u32,y:u32,count:u32,shape:bool)->bool {
 for(var age=0u;age<count;age++) {
  if age>y {break;}
  var radius=1i;
  if shape && age!=0u && age!=4u {radius=2;}
  for(var dx=-radius;dx<=radius;dx++) {
   let xx=i32(x)+dx;
   if xx>=0 && xx<i32(params.size.x) && (atomicLoad(&history[2u*params.size.y+((y-age)*params.size.x+u32(xx))/32u]) & (1u<<(((y-age)*params.size.x+u32(xx))%32u)))!=0u {return true;}
  }
 }
 return false;
}
fn row_bounds(y:u32)->vec2<u32> {
 var left=0u;var right=0u;
 for(var age=0u;age<7u;age++) {
  if age>y {break;}
  left=max(left,atomicLoad(&history[(y-age)*2u]));
  right=max(right,atomicLoad(&history[(y-age)*2u+1u]));
 }
 if left==0u {return vec2<u32>(0u);}
 // A flag can affect the five-row stencil at most two columns away.
 return vec2<u32>(max(2u,params.size.x-1u-left-2u),min(params.size.x-2u,right+3u));
}
fn delayed(x:u32,y:u32,previous:vec2<u32>)->vec3<i32> {
 // Outside a previously computed interval the delayed image row has not
 // changed, so its neutralized samples supply the scratch value directly.
 if x<previous.x || x>=previous.y {return read_rgb(x,y-4u);}
 let p=(((y-2u)%3u)*params.size.x+x)*3u;
 return vec3<i32>(scratch[p],scratch[p+1u],scratch[p+2u]);
}
@compute @workgroup_size(WORKGROUP_THREADS)
fn main(@builtin(local_invocation_index) lane:u32,@builtin(global_invocation_id) gid:vec3<u32>) {
 let w=params.size.x;let h=params.size.y;let span=params.apriori.y-params.fix.y;
 if params.size.w==INITIALIZE {
  let x=gid.x;let y=gid.y;if x>=w || y>=h {return;}
  let p=y*w+x;let at=p*3u;
  var value=vec3<i32>(i32(input_rgb[at]),i32(input_rgb[at+1u]),i32(input_rgb[at+2u]));
  if w>=5u && span>=20 && x>=2u && x+2u<w {
   let e=value-params.fix.xyz;let s=min(e.x,min(e.y,e.z))+max(e.x,max(e.y,e.z));
   if s>1 {
    let strength=s>>1u;let sum=value.x+value.y+value.z;
    var delta=((params.scaled.xyz*sum)>>vec3<u32>(12u))-value;
    delta-=vec3<i32>(min(0,min(delta.x,min(delta.y,delta.z))));
    if strength<span {
     let t=16384-((strength*(268435456/span))>>14u);
     let a=16384-i32(u32(t*t)>>14u);let alpha=i32(u32(a*a)>>14u);
     value+=((delta*alpha)>>vec3<u32>(14u));
    } else {
     value+=delta;
     atomicOr(&history[2u*h+p/32u],1u<<(p%32u));
     atomicMax(&history[y*2u],w-1u-x);atomicMax(&history[y*2u+1u],x);
    }
    value=vec3<i32>(narrow(value.x),narrow(value.y),narrow(value.z));
   }
  }
  put_rgb(x,y,value);
  return;
 }
 if w<5u || span<20 {return;}
 for(var y=4u;y<h;y++) {
  let bounds=row_bounds(y);
  if bounds.x==bounds.y {continue;}
  var previous=vec2<u32>(0u);if y>=6u {previous=row_bounds(y-2u);}
  for(var x=lane+bounds.x;x<bounds.y;x+=WORKGROUP_THREADS) {
    if marked(x,y,3u,false) {
     let old=read_rgb(x,y-1u);let mean=((old.x+old.y+old.z)*1365)>>12u;
     var value=(old+vec3<i32>(mean))/2;
     if params.size.z!=0u {value=vec3<i32>(mean);}
     put_rgb(x,y-1u,min(vec3<i32>(32000),value));
    }
    if params.size.z!=0u && marked(x,y,2u,false) {
     let old=read_rgb(x,y);let mean=((old.x+old.y+old.z)*1365)>>12u;put_rgb(x,y,vec3<i32>(mean));
    }
  }
  storageBarrier();
  for(var x=lane+bounds.x;x<bounds.y;x+=WORKGROUP_THREADS) {
   let p=((y%3u)*w+x)*3u;
   let center=read_rgb(x,y-2u);var v=center;
    if marked(x,y,5u,true) {
     var sum=vec3<i32>(0);
     for(var age=0u;age<5u;age++) {
      var radius=2i;if age==0u || age==4u {radius=1;}
      for(var dx=-radius;dx<=radius;dx++) {
       if age==4u && dx == -1 && x>2u {
        // The native left-to-right sweep has already replaced this oldest
        // neighbor. Its replacement comes from a previously completed scratch
        // row, so read that known value without serializing the current row.
        sum+=delayed(x-1u,y,previous);
       } else {sum+=read_rgb(u32(i32(x)+dx),y-age);}
      }
     }
     let average=(sum*195)>>vec3<u32>(12u);
     let common_value=((center.x+center.y+center.z)-(average.x+average.y+average.z))>>2u;
     v=min(vec3<i32>(32000),average+vec3<i32>(common_value));
    }
    scratch[p]=narrow(v.x);scratch[p+1u]=narrow(v.y);scratch[p+2u]=narrow(v.z);
  }
  storageBarrier();
  for(var x=lane+bounds.x;x<bounds.y;x+=WORKGROUP_THREADS) {
    put_rgb(x,y-4u,delayed(x,y,previous));
  }
  storageBarrier();
 }
}
