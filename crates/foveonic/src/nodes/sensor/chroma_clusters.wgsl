// Candidate patches are independent within each ordered repair point.
const CANDIDATE_LANES=256u;
struct Region{rect:vec4<u32>,ranges:vec4<u32>,weights:vec4<u32>}
@group(0) @binding(0) var<storage,read_write> pixels:array<f32>;
@group(0) @binding(1) var<storage,read> mask:array<u32>;
@group(0) @binding(2) var<uniform> p:vec4<u32>;
@group(0) @binding(3) var<storage,read> regions:array<Region>;
@group(0) @binding(4) var<storage,read> points:array<vec2<u32>>;
@group(0) @binding(5) var<storage,read> candidates:array<vec2<u32>>;
@group(0) @binding(6) var<storage,read_write> weights:array<u32>;
const ORIENTATIONS=array<vec4<i32>,6>(vec4(1,0,0,1),vec4(0,-1,1,0),vec4(1,0,0,-1),vec4(0,1,-1,0),vec4(-1,0,0,1),vec4(-1,0,0,-1));
fn load(x:u32,y:u32)->vec3<f32>{let i=(y*p.x+x)*3u;return vec3(pixels[i],pixels[i+1u],pixels[i+2u]);}
var<workgroup> score_minimum:array<f32,CANDIDATE_LANES>;
var<workgroup> score_index:array<u32,CANDIDATE_LANES>;
@compute @workgroup_size(CANDIDATE_LANES)
fn chroma_clusters(@builtin(local_invocation_index) lane:u32,@builtin(workgroup_id) group:vec3<u32>){
 let region_index=p.w+group.x;
  let r=regions[region_index];let width=r.rect.z;
  for(var point_index=0u;point_index<r.ranges.y;point_index++){
   let repair_point=points[r.ranges.x+point_index];let absolute=r.rect.xy+repair_point;
   if repair_point.x==0u||repair_point.y==0u||repair_point.x>=r.rect.z-1u||repair_point.y>=r.rect.w-1u||absolute.x>=p.x-1u||absolute.y>=p.y-1u||mask[absolute.y*p.x+absolute.x]!=0u{continue;}
   var best=3.402823466e38;var best_index=4294967295u;
   for(var candidate_index=lane;candidate_index<r.ranges.w;candidate_index+=CANDIDATE_LANES){
    let candidate=candidates[r.ranges.z+candidate_index];var minimum=3.402823466e38;
    for(var orientation=0u;orientation<6u;orientation++){
     let transform=ORIENTATIONS[orientation];var errors:array<vec3<f32>,9>;
     for(var dy=-1;dy<=1;dy++){for(var dx=-1;dx<=1;dx++){
      let sx=transform.x*dx+transform.y*dy;let sy=transform.z*dx+transform.w*dy;
      // Native top-right and middle-right weight rows are exchanged.
      var wy=dy;if dx==1&&dy == -1{wy=0;}else if dx==1&&dy==0{wy = -1;}
      let bits=weights[r.weights.x+u32(i32(repair_point.y)+wy)*width+u32(i32(repair_point.x)+dx)];
      let weight=vec3(f32((bits>>2u)&1u),f32((bits>>1u)&1u),f32(bits&1u));
      let a=load(u32(i32(absolute.x)+dx),u32(i32(absolute.y)+dy));let b=load(r.rect.x+u32(i32(candidate.x)+sx),r.rect.y+u32(i32(candidate.y)+sy));
      let weighted=vec3(fma(weight.x,a.x,0.),fma(weight.y,a.y,0.),fma(weight.z,a.z,0.));
      errors[u32((sy+1)*3+sx+1)]=abs(vec3(fma(weight.x,b.x,-weighted.x),fma(weight.y,b.y,-weighted.y),fma(weight.z,b.z,-weighted.z)));
     }}
     var score=0.;var first=true;
     for(var dy=-1;dy<=1;dy++){for(var dx=-1;dx<=1;dx++){
      let triple=errors[u32((dy+1)*3+dx+1)];let corner=dx!=0&&dy!=0;let center=dx==0&&dy==0;
      let grouped=center||orientation==2u||orientation==4u||select(!corner,corner,orientation==1u||orientation==3u);
      if grouped{let term=fma(1.,fma(1.,triple.x,triple.y),triple.z);if first{score=term;first=false;}else{score=fma(1.,score,term);}}
      else{for(var c=0u;c<3u;c++){if first{score=triple[c];first=false;}else{score=fma(1.,score,triple[c]);}}}
     }}
     minimum=min(minimum,score);
    }
    if minimum<best{best=minimum;best_index=candidate_index;}
   }
   score_minimum[lane]=best;score_index[lane]=best_index;
   workgroupBarrier();
   for(var stride=CANDIDATE_LANES/2u;stride>0u;stride/=2u){
    if lane<stride{
     let other=lane+stride;let less=score_minimum[other]<score_minimum[lane];let tie=score_minimum[other]==score_minimum[lane]&&score_index[other]<score_index[lane];
     if less||tie{score_minimum[lane]=score_minimum[other];score_index[lane]=score_index[other];}
    }
    workgroupBarrier();
   }
   if lane==0u{
    var chosen=vec3(0.);if score_index[0]!=4294967295u{let candidate=candidates[r.ranges.z+score_index[0]];chosen=load(r.rect.x+candidate.x,r.rect.y+candidate.y);}
    let index=(absolute.y*p.x+absolute.x)*3u;pixels[index]=chosen.x;pixels[index+1u]=chosen.y;pixels[index+2u]=chosen.z;
    weights[r.weights.x+repair_point.y*width+repair_point.x]=7u;
   }
   // Targets stay ordered; candidate scores observe all previous pixel/weight writes.
   storageBarrier();workgroupBarrier();
  }
}
