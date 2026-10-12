// Reference clipped-layer restoration with immutable original saturation
// provenance and stable row-major donor accumulation. Adapted with permission
// from Foveonic highlight.rs at e47af35, including the owner's existing changes.
struct Params {source:vec4<u32>,area:vec4<u32>,value:vec4<f32>};
@group(0) @binding(0) var<storage,read> original:array<u32>;
@group(0) @binding(1) var<storage,read_write> flags:array<u32>;
@group(0) @binding(2) var<uniform> params:Params;
@group(0) @binding(3) var<storage,read_write> map:array<f32>;
@group(0) @binding(4) var<storage,read_write> output:array<f32>;
@group(0) @binding(5) var<storage,read> pixels:array<f32>;
@group(0) @binding(6) var<storage,read_write> marks:array<u32>;
@group(0) @binding(7) var<storage,read_write> counts:array<u32>;
@group(0) @binding(8) var<storage,read_write> donors:array<u32>;
@group(0) @binding(9) var<storage,read_write> lookup:array<f32>;
@group(0) @binding(10) var<storage,read_write> sorted_donors:array<u32>;
@group(0) @binding(11) var<storage,read_write> bin_counts:array<u32>;
@group(0) @binding(12) var<storage,read_write> bin_starts:array<u32>;
var<workgroup> ranks:array<u32,256>;
var<workgroup> histogram:array<atomic<u32>,256>;
var<workgroup> donor_range:vec2<u32>;
var<workgroup> donor_rgb:array<vec3<f32>,64>;
fn in_image(xy:vec2<u32>)->bool{return xy.x<params.source.x && xy.y<params.source.y;}
fn work(xy:vec2<u32>)->bool{return all(xy>=params.area.xy) && all(xy<=params.area.zw);}
fn color(p:u32)->vec3<f32>{return vec3(pixels[p*3u],pixels[p*3u+1u],pixels[p*3u+2u])/params.value.y;}
@compute @workgroup_size(8,8)
fn connected_flags(@builtin(global_invocation_id)id:vec3<u32>){
    let xy=id.xy;if !in_image(xy){return;}let p=xy.y*params.source.x+xy.x;var connected=original[p];
    if all(xy>=vec2(1u)) && xy.x+1u<params.source.x && xy.y+1u<params.source.y{
        var neighbors=0u;
        for(var dy=-1;dy<=1;dy++){for(var dx=-1;dx<=1;dx++){
            if dx!=0 || dy!=0{let q=vec2<i32>(xy)+vec2(dx,dy);neighbors|=original[u32(q.y)*params.source.x+u32(q.x)];}
        }}
        // In-place native removal and immutable parallel removal are equivalent:
        // every connected pair keeps each other, so only original isolated flags
        // can be removed. Border flags are copied, not reclassified.
        connected&=neighbors;
    }
    flags[p]=connected;
}
@compute @workgroup_size(8,8)
fn neighborhood_map(@builtin(global_invocation_id)id:vec3<u32>){
    let xy=id.xy;if !in_image(xy){return;}var marked=false;
    for(var dy=-2;dy<=2;dy++){for(var dx=-2;dx<=2;dx++){
        let q=vec2<i32>(xy)+vec2(dx,dy);
        if all(q>=vec2(2)) && q.x<i32(params.source.x)-2 && q.y<i32(params.source.y)-2{
            if countOneBits(flags[u32(q.y)*params.source.x+u32(q.x)])>=2u{marked=true;}
        }
    }}
    map[xy.y*params.source.x+xy.x]=select(0.0,1.0,marked);
}
fn density(xy:vec2<u32>)->f32{
    var sum=0.0;
    for(var dy=-2;dy<=2;dy++){for(var dx=-2;dx<=2;dx++){
        let q=vec2<i32>(xy)+vec2(dx,dy);sum+=map[u32(q.y)*params.source.x+u32(q.x)];
    }}
    return sum*0.04;
}
@compute @workgroup_size(8,8)
fn density_field(@builtin(global_invocation_id)id:vec3<u32>){
    let xy=id.xy;if !in_image(xy){return;}var value=0.0;
    if all(xy>=vec2(2u)) && xy.x+2u<params.source.x && xy.y+2u<params.source.y{value=density(xy);}
    output[xy.y*params.source.x+xy.x]=value;
}
@compute @workgroup_size(256)
fn donor_marks(@builtin(global_invocation_id)id:vec3<u32>,@builtin(local_invocation_index)local:u32,@builtin(workgroup_id)group:vec3<u32>){
    let p=id.x;var donor=false;
    if p<params.source.z && params.value.y>0.0{
        let xy=vec2(p%params.source.x,p/params.source.x);
        if work(xy) && (flags[p]&1u)==0u{let rgb=color(p);donor=rgb.x+rgb.y>4000.0;}
    }
    ranks[local]=select(0u,1u,donor);workgroupBarrier();
    for(var step=1u;step<256u;step*=2u){var previous=0u;if local>=step{previous=ranks[local-step];}workgroupBarrier();ranks[local]+=previous;workgroupBarrier();}
    if p<params.source.z{marks[p]=select(0u,ranks[local],donor);}
    if local==255u{counts[group.x]=ranks[local];}
}
@compute @workgroup_size(1)
fn donor_prefix(){var sum=0u;for(var i=0u;i<params.source.w;i++){let count=counts[i];counts[i]=sum;sum+=count;}counts[params.source.w]=sum;}
@compute @workgroup_size(256)
fn donor_scatter(@builtin(global_invocation_id)id:vec3<u32>,@builtin(workgroup_id)group:vec3<u32>){
    let p=id.x;if p>=params.source.z{return;}let rank=marks[p];if rank!=0u{donors[counts[group.x]+rank-1u]=p;}
}
@compute @workgroup_size(256)
fn bin_histogram(@builtin(global_invocation_id)id:vec3<u32>,@builtin(local_invocation_index)local:u32,@builtin(workgroup_id)group:vec3<u32>){
    atomicStore(&histogram[local],0u);workgroupBarrier();
    if id.x<counts[params.source.w]{
        let rgb=max(color(donors[id.x]),vec3(1.0));let bucket=u32(rgb.x*255.0/(rgb.x+rgb.y));
        // Original rank scratch is no longer needed after donor_scatter.
        marks[id.x]=bucket;atomicAdd(&histogram[bucket],1u);
    }
    workgroupBarrier();bin_counts[group.x*256u+local]=atomicLoad(&histogram[local]);
}
@compute @workgroup_size(256)
fn bin_prefix(@builtin(local_invocation_index)bucket:u32){
    let blocks=(counts[params.source.w]+255u)/256u;var sum=0u;
    for(var block=0u;block<blocks;block++){
        let index=block*256u+bucket;let count=bin_counts[index];bin_counts[index]=sum;sum+=count;
    }
    ranks[bucket]=sum;workgroupBarrier();
    for(var step=1u;step<256u;step*=2u){var previous=0u;if bucket>=step{previous=ranks[bucket-step];}workgroupBarrier();ranks[bucket]+=previous;workgroupBarrier();}
    bin_starts[bucket]=ranks[bucket]-sum;if bucket==255u{bin_starts[256]=ranks[bucket];}
}
@compute @workgroup_size(256)
fn bin_scatter(@builtin(global_invocation_id)id:vec3<u32>,@builtin(local_invocation_index)local:u32,@builtin(workgroup_id)group:vec3<u32>){
    let valid=id.x<counts[params.source.w];var bucket=256u;if valid{bucket=marks[id.x];}
    ranks[local]=bucket;workgroupBarrier();
    if valid{
        var prior=0u;for(var i=0u;i<local;i++){if ranks[i]==bucket{prior+=1u;}}
        let index=bin_starts[bucket]+bin_counts[group.x*256u+bucket]+prior;
        sorted_donors[index]=donors[id.x];
    }
}
@compute @workgroup_size(64)
fn correction_table(@builtin(local_invocation_index)lane:u32,@builtin(workgroup_id)group:vec3<u32>){
    let bucket=group.x;
    if lane==0u{donor_range=vec2(bin_starts[bucket],bin_starts[bucket+1u]);}
    let range=workgroupUniformLoad(&donor_range);var sum=0.0;var count=0.0;
    // Parallel RGB loading; the original bin recurrence remains sequential.
    // Ratio arithmetic stays inline rather than materializing rounded ratios.
    for(var base=range.x;base<range.y;base+=64u){
        if base+lane<range.y{donor_rgb[lane]=max(color(sorted_donors[base+lane]),vec3(1.0));}
        workgroupBarrier();
        if lane==0u{
            for(var i=0u;i<min(64u,range.y-base);i++){
                let rgb=donor_rgb[i];let bin=u32(rgb.x*255.0/(rgb.x+rgb.y));
                if bin==bucket{sum+=rgb.z/rgb.y;count+=1.0;}
            }
        }
        workgroupBarrier();
    }
    if lane==0u{lookup[bucket]=sum;lookup[256u+bucket]=count;}
}
@compute @workgroup_size(1)
fn table_interpolation(){
    var first=256u;
    for(var i=0u;i<256u;i++){if lookup[i]>0.0{lookup[i]/=lookup[256u+i];if first==256u{first=i;}}}
    if first<256u{var previous=lookup[first];for(var i=0u;i<256u;i++){if lookup[i]==0.0{lookup[i]=previous;}previous=lookup[i];}}
}
@compute @workgroup_size(8,8)
fn restore(@builtin(global_invocation_id)id:vec3<u32>){
    let xy=id.xy;if !in_image(xy){return;}let p=xy.y*params.source.x+xy.x;
    if !work(xy) || params.value.y==0.0{for(var c=0u;c<3u;c++){output[p*3u+c]=pixels[p*3u+c];}return;}
    let rgb=color(p);var blue=rgb.z;
    if lookup[0]>0.0 && (flags[p]&1u)!=0u{
        var ratio=0.0;let total=rgb.x+rgb.y;
        if total!=0.0{ratio=clamp(rgb.x*255.0/total,0.0,255.0);}else if rgb.x>0.0{ratio=255.0;}
        blue=min(rgb.y*lookup[u32(ratio)],32767.0);
    }
    let brightest=max(max(rgb.x,rgb.y),blue);let middle=max(min(rgb.x,rgb.y),min(max(rgb.x,rgb.y),blue));
    let brightness=brightest*(2.0/3.0)+middle*(1.0/3.0);
    let bright=clamp((brightness-params.value.x)/1000.0,0.0,1.0);let den=density(xy);
    let weight=sqrt((bright*bright+den*den)*0.5);let inverse=1.0-weight;let neutral=weight*rgb.x;
    output[p*3u]=rgb.x*params.value.y;
    output[p*3u+1u]=fma(inverse,rgb.y,neutral)*params.value.y;
    output[p*3u+2u]=fma(inverse,blue,neutral)*params.value.y;
}
