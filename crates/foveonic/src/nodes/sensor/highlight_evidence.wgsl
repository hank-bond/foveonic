// Pool original recovery controls separately from numerical area means.
// The block geometry is the shared sensor-pool uniform contract.
struct BlockGrid {shape:vec4<u32>,controls:vec4<u32>,origin:vec4<u32>}
@group(0) @binding(9) var<uniform> blocks:BlockGrid;
@group(0) @binding(4) var<storage,read_write> output:array<vec4<f32>>;
@group(0) @binding(11) var<storage,read> row_evidence:array<vec4<f32>>;

fn evidence_weighted(source:vec3<f32>)->vec3<f32> {
 var value=min(source,vec3(p.thresholds.x))*p.gains.xyz;
 if p.shape.w!=0u {
  value=min(source,p.ceilings.xyz)*p.gains.xyz;
  let limits=p.ceilings.xyz*p.gains.xyz;
  let flags=vec3<u32>(value>=limits);
  if flags.x+flags.y+flags.z>=2u {value=vec3(max(min(limits.x,limits.y),min(max(limits.x,limits.y),limits.z)));}
 }
 return value;
}
fn evidence_source(x:u32,y:u32)->vec3<f32> {
 let i=(y*p.shape.x+x)*3u;
 return vec3(pixels[i],pixels[i+1u],pixels[i+2u]);
}
fn evidence_clipped(source:vec3<f32>)->bool {
 if p.shape.w!=0u{return any(source>=p.ceilings.xyz);}
 return any(min(source,vec3(p.thresholds.x))*p.gains.xyz>=p.gains.xyz*p.thresholds.x);
}
// Keep native detector coverage. Reclassifying an averaged cell's neighbors
// expands a thin clipped boundary into a whole working-pixel color correction.
fn native_classification(x:u32,y:u32,source:vec3<f32>,weighted:vec3<f32>)->vec2<f32> {
 var ring=evidence_clipped(source);
 var green_total=0.;
 if x>0u&&y>0u&&x<p.shape.x-1u&&y<p.shape.y-1u {
  var any_set=false;var any_clear=false;
  for(var dy=-1;dy<=1;dy++){for(var dx=-1;dx<=1;dx++){
   let neighbor=evidence_source(u32(i32(x)+dx),u32(i32(y)+dy));
   let clipped=evidence_clipped(neighbor);
   any_set=any_set||clipped;any_clear=any_clear||!clipped;
   let weight=select(1.,0.25,dx!=0&&dy!=0);
   green_total=fma(weight,f32(neighbor.y>=p.thresholds.y),green_total);
  }}
  ring=any_set&&any_clear;
 }
 if !(x>=2u&&x<p.shape.x-2u&&y>=2u&&y<p.shape.y-1u){green_total=0.;}
 var directional=false;
 if x>=p.rectangle.x&&y>=p.rectangle.y&&x<p.rectangle.z&&y<p.rectangle.w&&source.y<p.thresholds.y {
  let rg=(weighted.x+0.01)/(weighted.y+0.01);let bg=(weighted.z+0.01)/(weighted.y+0.01);
  if rg<NEUTRAL_RATIO_MIN||rg>NEUTRAL_RATIO_MAX||bg<NEUTRAL_RATIO_MIN||bg>NEUTRAL_RATIO_MAX {
   let offsets=array<vec2<i32>,4>(vec2(-1,0),vec2(1,0),vec2(0,-1),vec2(0,1));
   for(var k=0u;k<4u;k++){
    let pos=vec2<i32>(i32(x),i32(y))+offsets[k];
    let other=evidence_source(u32(pos.x),u32(pos.y));
    let other_weighted=evidence_weighted(other);
    directional=directional||(other.y>=p.thresholds.y&&(other_weighted.y-weighted.y)/p.gains.y>p.thresholds.w);
   }
  }
 }
 let converted=color(weighted,0u);let minimum=min(converted.x,min(converted.y,converted.z));
 let low=f32(i32(minimum))<p.thresholds.z;
 return vec2(f32(directional||(low&&ring)),ceil(min(green_total*255.,255.)));
}

@compute @workgroup_size(8,8)
fn highlight_evidence(@builtin(global_invocation_id) id:vec3<u32>) {
 if any(id.xy>=blocks.origin.zw){return;}
 let cell=id.xy;
 let output_first=select(blocks.origin.xy+cell,vec2<u32>(0),cell==vec2<u32>(0));
 let output_end=select(blocks.origin.xy+cell+vec2<u32>(1),blocks.shape.zw,cell+vec2<u32>(1)==blocks.origin.zw);
 let first=cell*blocks.controls.x;let end=min(blocks.shape.xy,first+vec2(blocks.controls.x));
 var peaks=vec3(-3.402823466e38);var total=vec3(0.);var original_total=vec3(0.);
 var coverage=0.;var green_mask=0.;
 for(var y=first.y;y<end.y;y++) {
  var row=vec3(0.);var original_row=vec3(0.);var row_coverage=0.;
  for(var x=first.x;x<end.x;x++) {
   let source=evidence_source(x,y);peaks=max(peaks,source);original_row+=source;
   let weighted=evidence_weighted(source);
   let classification=native_classification(x,y,source,weighted);
   row_coverage+=classification.x;green_mask=max(green_mask,classification.y);
   let amount=neutralization_weight(weighted);let corrected=color(weighted,INVERSE_CAMERA_MATRIX);
   row+=vec3(fma(amount,corrected.x,weighted.x*(1.-amount)),fma(amount,corrected.y,weighted.y*(1.-amount)),fma(amount,corrected.z,weighted.z*(1.-amount)));
  }
  total+=row/f32(end.x-first.x);original_total+=original_row/f32(end.x-first.x);coverage+=row_coverage/f32(end.x-first.x);
 }
 let base=evidence_weighted(original_total/f32(end.y-first.y));
 for(var y=output_first.y;y<output_end.y;y++) {
  for(var x=output_first.x;x<output_end.x;x++) {
   let index=(y*blocks.shape.z+x)*2u;
   output[index]=vec4(peaks,coverage/f32(end.y-first.y));
   output[index+1u]=vec4(total/f32(end.y-first.y)-base,green_mask);
  }
 }
}

// Large cells retain the original ordered row means without assigning an
// entire native image to one invocation.
@compute @workgroup_size(8,8)
fn highlight_evidence_rows(@builtin(global_invocation_id) id:vec3<u32>) {
 if id.x>=blocks.origin.z||id.y>=blocks.shape.y{return;}
 let first=id.x*blocks.controls.x;let end=min(blocks.shape.x,first+blocks.controls.x);
 var peaks=vec3(-3.402823466e38);var row=vec3(0.);var original_row=vec3(0.);
 var coverage=0.;var green_mask=0.;
 for(var x=first;x<end;x++) {
  let source=evidence_source(x,id.y);peaks=max(peaks,source);original_row+=source;
  let weighted=evidence_weighted(source);
  let classification=native_classification(x,id.y,source,weighted);
  coverage+=classification.x;green_mask=max(green_mask,classification.y);
  let amount=neutralization_weight(weighted);let corrected=color(weighted,INVERSE_CAMERA_MATRIX);
  row+=vec3(fma(amount,corrected.x,weighted.x*(1.-amount)),fma(amount,corrected.y,weighted.y*(1.-amount)),fma(amount,corrected.z,weighted.z*(1.-amount)));
 }
 let width=f32(end-first);let index=(id.y*blocks.origin.z+id.x)*3u;
 output[index]=vec4(peaks,coverage/width);
 output[index+1u]=vec4(row/width,green_mask);
 output[index+2u]=vec4(original_row/width,0.);
}

@compute @workgroup_size(8,8)
fn highlight_evidence_columns(@builtin(global_invocation_id) id:vec3<u32>) {
 if any(id.xy>=blocks.origin.zw){return;}
 let cell=id.xy;
 let output_first=select(blocks.origin.xy+cell,vec2<u32>(0),cell==vec2<u32>(0));
 let output_end=select(blocks.origin.xy+cell+vec2<u32>(1),blocks.shape.zw,cell+vec2<u32>(1)==blocks.origin.zw);
 let first=cell.y*blocks.controls.x;let end=min(blocks.shape.y,first+blocks.controls.x);
 var peaks=vec3(-3.402823466e38);var total=vec3(0.);var original_total=vec3(0.);
 var coverage=0.;var green_mask=0.;
 for(var y=first;y<end;y++) {
  let index=(y*blocks.origin.z+cell.x)*3u;
  let a=row_evidence[index];let b=row_evidence[index+1u];let c=row_evidence[index+2u];
  peaks=max(peaks,a.xyz);coverage+=a.w;total+=b.xyz;green_mask=max(green_mask,b.w);original_total+=c.xyz;
 }
 let height=f32(end-first);let base=evidence_weighted(original_total/height);
 for(var y=output_first.y;y<output_end.y;y++) {
  for(var x=output_first.x;x<output_end.x;x++) {
   let index=(y*blocks.shape.z+x)*2u;
   output[index]=vec4(peaks,coverage/height);
   output[index+1u]=vec4(total/height-base,green_mask);
  }
 }
}
