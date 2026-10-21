// Signed-code smoothing with explicit support, signed16 narrowing and lookup correction.
struct Params { shape:vec4<u32>, correction:vec4<u32>, decay:vec4<i32>, coarse_decay:vec4<i32> }
@group(0) @binding(0) var<storage,read> source:array<f32>;
@group(0) @binding(1) var<uniform> params:Params;
@group(0) @binding(2) var<storage,read_write> blurred:array<i32>;
@group(0) @binding(3) var<storage,read> table:array<i32>;
@group(0) @binding(4) var<storage,read_write> destination:array<f32>;
@group(0) @binding(5) var<storage,read_write> recursive_horizontal:array<i32>;
fn narrow(v:i32)->i32 { return (v<<16u)>>16u; }
// Divide an unsigned 48-bit Q16 numerator without losing low bits to Float32.
fn ratio_q16(value:u32,denominator:u32)->i32 {
    var remainder=0u;var quotient=0u;
    for(var bit=47;bit>=0;bit--) {
        var digit=0u;
        if bit>=16 { digit=(value>>u32(bit-16))&1u; }
        remainder=(remainder<<1u)|digit;
        if remainder>=denominator { remainder-=denominator;if bit<32 {quotient|=1u<<u32(bit);} }
    }
    return i32(quotient);
}
fn position(x:i32,y:i32)->u32 { return (u32(clamp(y,0,i32(params.shape.y)-1))*params.shape.x+u32(clamp(x,0,i32(params.shape.x)-1)))*3u; }
@compute @workgroup_size(8,8)
fn horizontal(@builtin(global_invocation_id) id:vec3<u32>) {
    if id.x>=params.shape.x || id.y>=params.shape.y { return; }
    let x=i32(id.x); let y=i32(id.y); let p=position(x,y);
    for(var c=0u;c<3u;c++) {
        var sum=0;
        if params.shape.z==0u {
            sum=i32(source[position(x-1,y)+c])+2*i32(source[p+c])+i32(source[position(x+1,y)+c]);
        } else {
            for(var dx=-2;dx<=2;dx++) { sum+=i32(source[position(x+dx,y)+c]); }
        }
        blurred[p+c]=narrow((sum+2)>>2u);
    }
}
@compute @workgroup_size(8,8)
fn apply(@builtin(global_invocation_id) id:vec3<u32>) {
    if id.x>=params.shape.x || id.y>=params.shape.y { return; }
    let x=i32(id.x);let y=i32(id.y);let p=position(x,y);
    var old:array<i32,3>;var smoothed:array<i32,3>;var corrections:array<i32,3>;
    for(var c=0u;c<3u;c++) {
        old[c]=i32(source[p+c]);
        if params.shape.z==0u {
            smoothed[c]=(blurred[position(x,y-1)+c]+2*blurred[p+c]+blurred[position(x,y+1)+c])>>2u;
        } else {
            smoothed[c]=0;
            for(var dy=-2;dy<=2;dy++) { smoothed[c]+=blurred[position(x,y+dy)+c]; }
        }
    }
    if params.shape.z!=0u {
        let setting=i32(params.correction.x);let regularizer=(setting*25)/4;
        let brightness=old[0]+old[1]+old[2]+setting;let total=smoothed[0]+smoothed[1]+smoothed[2];
        var ratio=0;
        if brightness>=0 {
            if total<1 { if regularizer!=0 { ratio=brightness*(65536/regularizer); } }
            else { ratio=ratio_q16(u32(brightness),u32(total+regularizer)); }
        }
        for(var c=0u;c<3u;c++) { smoothed[c]=(ratio*smoothed[c]+32768)>>16u; }
    }
    let length=params.shape.w;let limit=i32(length/2u)-1;
    for(var c=0u;c<3u;c++) { corrections[c]=table[c*length+(u32(clamp(smoothed[c]-old[c],-limit,limit))&(length-1u))]; }
    let shared_correction=narrow((corrections[0]+corrections[1]+corrections[2])>>params.correction.y);
    for(var c=0u;c<3u;c++) { destination[p+c]=f32(narrow(old[c]+corrections[c]-shared_correction)); }
}

@compute @workgroup_size(64)
fn recursive_init(@builtin(global_invocation_id) id:vec3<u32>) {
 let coarse_width=(params.shape.x+3u)/4u;let coarse_height=(params.shape.y+3u)/4u;
 if id.x>=coarse_width {return;}
 var state:array<i32,3>;
 for(var row=i32(coarse_height)-1;row>=0;row--) {
  for(var c=0u;c<3u;c++) {
   var sum=0;
   for(var dy=0;dy<4;dy++) {for(var dx=0;dx<4;dx++) {sum+=i32(source[position(i32(id.x)*4+dx,row*4+dy)+c]);}}
   sum=min(sum,32767);
   state[c]=narrow((params.coarse_decay.x*sum+params.coarse_decay.y*state[c])>>8u);
   blurred[(u32(row)*coarse_width+id.x)*3u+c]=state[c];
  }
 }
}

@compute @workgroup_size(64)
fn recursive_horizontal_pass(@builtin(global_invocation_id) id:vec3<u32>) {
 let coarse_width=(params.shape.x+3u)/4u;let coarse_height=(params.shape.y+3u)/4u;
 if id.x>=coarse_height {return;}
 for(var c=0u;c<3u;c++) {
  var state=0;
  for(var group=i32(coarse_width)-1;group>=0;group--) {
   let sample=blurred[(id.x*coarse_width+u32(group))*3u+c];
   // A partial coarse group repeats writes to the final full-resolution column.
   for(var dx=3;dx>=0;dx--) {
    let x=min(u32(group*4+dx),params.shape.x-1u);
    state=(params.decay.x*sample+params.decay.y*state)>>8u;
    recursive_horizontal[(id.x*params.shape.x+x)*3u+c]=narrow(state);
   }
  }
  state=0;
  for(var x=0u;x<params.shape.x;x++) {
   let pixel_index=(id.x*params.shape.x+x)*3u+c;
   state=(params.decay.x*recursive_horizontal[pixel_index]+params.decay.y*state)>>8u;
   recursive_horizontal[pixel_index]=narrow(state);
  }
 }
}

@compute @workgroup_size(64)
fn recursive_apply(@builtin(global_invocation_id) id:vec3<u32>) {
 if id.x>=params.shape.x {return;}
 var state:array<i32,3>;
 for(var y=0u;y<params.shape.y;y++) {
  let pixel_index=(y*params.shape.x+id.x)*3u;
  let horizontal_index=((y/4u)*params.shape.x+id.x)*3u;
  var signal:array<i32,3>;var old:array<i32,3>;
  for(var c=0u;c<3u;c++) {
   let accumulated=params.decay.w*state[c]+params.decay.z*recursive_horizontal[horizontal_index+c];
   state[c]=narrow(accumulated>>8u);
   signal[c]=(accumulated*256)>>20u;
   old[c]=i32(source[pixel_index+c]);
  }
  let setting=i32(params.correction.x);let brightness=old[0]+old[1]+old[2]+setting;
  let total=signal[0]+signal[1]+signal[2];var ratio=0;
  if brightness>=0 {
   if total<1 {if setting!=0 {ratio=brightness*(65536/setting);}}
   else {
    // This callback divides a wrapped signed 32-bit numerator.
    ratio=(brightness*65536)/(total+setting);
   }
  }
  let length=params.shape.w;let limit=i32(length/2u)-1;
  var corrections:array<i32,3>;
  for(var c=0u;c<3u;c++) {
   let normalized=(ratio*signal[c]+32768)>>16u;
   corrections[c]=table[c*length+(u32(clamp(normalized-old[c],-limit,limit))&(length-1u))];
  }
  let shared_term=narrow((corrections[0]+corrections[1]+corrections[2])>>params.correction.y);
  for(var c=0u;c<3u;c++) {destination[pixel_index+c]=f32(narrow(old[c]+corrections[c]-shared_term));}
 }
}
