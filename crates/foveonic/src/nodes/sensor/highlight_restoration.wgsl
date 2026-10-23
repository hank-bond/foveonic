const UNSET_ANCHOR_COORDINATE=3000u;
const NEUTRAL_RATIO_MIN=0.66;
const NEUTRAL_RATIO_MAX=1.5;
const BRIGHTNESS_OFFSET=5.;
@group(0) @binding(4) var<storage,read_write> output:array<f32>;




@compute @workgroup_size(8,8)
fn highlight_prepare(@builtin(global_invocation_id) id:vec3<u32>){
 let x=id.x;let y=id.y;if x>=p.shape.x||y>=p.shape.y{return;}let n=y*p.shape.x+x;let i=n*3u;
 let source=vec3(pixels[i],pixels[i+1u],pixels[i+2u]);let weighted=min(source,vec3(p.thresholds.x))*p.gains.xyz;store_rgb(0u,n,weighted);
 work[p.shape.z*GREEN_MARKER+n]=f32(source.y>=p.thresholds.y);
 let flags=vec3<f32>(weighted>=p.gains.xyz*p.thresholds.x);work[p.shape.z*CLIPPED_MARKER+n]=f32(fma(1.,fma(1.,flags.x,flags.y),flags.z)>=1.);
 let converted=color(weighted,0u);let minimum=min(converted.x,min(converted.y,converted.z));work[p.shape.z*LOW_CAMERA_MARKER+n]=f32(f32(i32(minimum))<p.thresholds.z);
 var total=0.;if x>=2u&&x<p.shape.x-2u&&y>=2u&&y<p.shape.y-1u{
   // Only the central three rows have nonzero convolution weights.
   for(var dy=-1;dy<=1;dy++){for(var dx=-1;dx<=1;dx++){let weight=select(1.,0.25,dx!=0&&dy!=0);let offset=(u32(i32(y)+dy)*p.shape.x+u32(i32(x)+dx))*3u;total=fma(weight,f32(pixels[offset+1u]>=p.thresholds.y),total);}}
 }
 green_mask[n]=u32(ceil(min(total*255.,255.)));
}
@compute @workgroup_size(8,8)
fn highlight_classify(@builtin(global_invocation_id) id:vec3<u32>){
 let x=id.x;let y=id.y;if x>=p.shape.x||y>=p.shape.y{return;}let n=y*p.shape.x+x;let cells=p.shape.z;var ring=work[cells*CLIPPED_MARKER+n];
 if x>0u&&y>0u&&x<p.shape.x-1u&&y<p.shape.y-1u{var any_set=false;var any_clear=false;for(var dy=-1;dy<=1;dy++){for(var dx=-1;dx<=1;dx++){let v=work[cells*CLIPPED_MARKER+u32(i32(y)+dy)*p.shape.x+u32(i32(x)+dx)];any_set=any_set||v!=0.;any_clear=any_clear||v==0.;}}ring=f32(any_set&&any_clear);}
 var directional=0.;if x>=p.rectangle.x&&y>=p.rectangle.y&&x<p.rectangle.z&&y<p.rectangle.w&&work[cells*GREEN_MARKER+n]==0.{
   var count=0.;for(var dy=-1;dy<=1;dy++){for(var dx=-1;dx<=1;dx++){if dx!=0||dy!=0{count=count+work[cells*GREEN_MARKER+u32(i32(y)+dy)*p.shape.x+u32(i32(x)+dx)];}}}
   let value=load_rgb(0u,n);let rg=(value.x+0.01)/(value.y+0.01);let bg=(value.z+0.01)/(value.y+0.01);
   if count>=1.&&(rg<NEUTRAL_RATIO_MIN||rg>NEUTRAL_RATIO_MAX||bg<NEUTRAL_RATIO_MIN||bg>NEUTRAL_RATIO_MAX){
    let offsets=array<i32,4>(-1,1,-i32(p.shape.x),i32(p.shape.x));for(var k=0u;k<4u;k++){let other=u32(i32(n)+offsets[k]);if work[cells*GREEN_MARKER+other]!=0.&&(load_rgb(0u,other).y-value.y)/p.gains.y>p.thresholds.w{if k<2u{if directional!=1.{directional=directional+1.;}}else{directional=directional+2.;}}}
   }
 }
 work[cells*DIRECTIONAL_MARKER+n]=min(1.,fma(work[cells*LOW_CAMERA_MARKER+n],ring,directional));
}
fn interpolate_line(line:u32,vertical:bool){
 let length=select(p.shape.x,p.shape.y,vertical);let cells=p.shape.z;let destination=select(HORIZONTAL_RGB,VERTICAL_RGB,vertical);var prior=UNSET_ANCHOR_COORDINATE;
 for(var coordinate=0u;coordinate<length;coordinate++){
  let index=select(line*p.shape.x+coordinate,coordinate*p.shape.x+line,vertical);store_rgb(destination,index,load_rgb(0u,index));
 }
 for(var coordinate=0u;coordinate<length;coordinate++){
  let index=select(line*p.shape.x+coordinate,coordinate*p.shape.x+line,vertical);if work[cells*DIRECTIONAL_MARKER+index]!=0.{continue;}
  if prior!=UNSET_ANCHOR_COORDINATE&&coordinate>prior+1u{
   let start_index=select(line*p.shape.x+prior,prior*p.shape.x+line,vertical);let start=load_rgb(0u,start_index);let end=load_rgb(0u,index);let delta=end-start;
   for(var k=prior+1u;k<coordinate;k++){let other=select(line*p.shape.x+k,k*p.shape.x+line,vertical);let factor=f32(k-prior);let denominator=f32(coordinate-prior);let value=start+(delta*factor)/denominator;store_rgb(destination,other,value);}
  }
  prior=coordinate;
 }
 for(var coordinate=0u;coordinate<length;coordinate++){
  let index=select(line*p.shape.x+coordinate,coordinate*p.shape.x+line,vertical);let source=load_rgb(0u,index);let interpolated=load_rgb(destination,index);let sm=fma(1.,fma(1.,source.x,source.y),source.z);let im=fma(1.,fma(1.,interpolated.x,interpolated.y),interpolated.z);if sm==0.||im==0.{continue;}
  let a=sm/3.+BRIGHTNESS_OFFSET;let b=im/3.+BRIGHTNESS_OFFSET;if a==0.||b==0.{continue;}var ratio=a/b;if vertical{ratio=(a+BRIGHTNESS_OFFSET)/(b+BRIGHTNESS_OFFSET);}store_rgb(destination,index,interpolated*ratio);
 }
}
@compute @workgroup_size(64)
fn highlight_horizontal(@builtin(global_invocation_id) id:vec3<u32>){if id.x<p.shape.y{interpolate_line(id.x,false);}}
@compute @workgroup_size(64)
fn highlight_vertical(@builtin(global_invocation_id) id:vec3<u32>){if id.x<p.shape.x{interpolate_line(id.x,true);}}
@compute @workgroup_size(8,8)
fn highlight_finish(@builtin(global_invocation_id) id:vec3<u32>){
 if id.x>=p.shape.x||id.y>=p.shape.y{return;}let n=id.y*p.shape.x+id.x;var value=load_rgb(0u,n);
 if work[p.shape.z*DIRECTIONAL_MARKER+n]==1.{let a=load_rgb(HORIZONTAL_RGB,n);let b=load_rgb(VERTICAL_RGB,n);let square=vec3(fma(a.x,a.x,fma(b.x,b.x,0.)),fma(a.y,a.y,fma(b.y,b.y,0.)),fma(a.z,a.z,fma(b.z,b.z,0.)));value=sqrt(square*0.5);}
 value=finite(value);if p.shape.w==CHANNEL_EXTENSION{value=value/p.gains.xyz;let i=n*3u;output[i]=value.x;output[i+1u]=value.y;output[i+2u]=value.z;return;}let mean=fma(1.,fma(1.,value.x,value.y),value.z)/3.;var amount=clamp((mean-p.neutralization.x)/p.neutralization.y,0.,1.);if value.z<=p.neutralization.z{amount=amount*0.;}let transformed=color(value,INVERSE_CAMERA_MATRIX);let other=1.-amount;
 value=vec3(fma(amount,transformed.x,value.x*other),fma(amount,transformed.y,value.y*other),fma(amount,transformed.z,value.z*other))/p.gains.xyz;let i=n*3u;output[i]=value.x;output[i+1u]=value.y;output[i+2u]=value.z;
}
