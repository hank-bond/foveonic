// Bounded dark-channel candidate selection on a stable linear RGB overview.
struct Basis {
    to_camera:array<vec4<f32>,3>, from_camera:array<vec4<f32>,3>,
    to_srgb:array<vec4<f32>,3>, from_srgb:array<vec4<f32>,3>, luma:vec4<f32>,
}
struct Estimate {
    shape: vec4<u32>, region: vec4<u32>, selection: vec4<u32>,
    normalization: vec4<f32>, basis:Basis,
}
@group(0) @binding(0) var<storage,read> src: array<f32>;
@group(0) @binding(1) var<uniform> p: Estimate;
@group(0) @binding(2) var<storage,read_write> pixels: array<vec4<u32>>;
@group(0) @binding(3) var<storage,read_write> horizontal: array<u32>;
@group(0) @binding(4) var<storage,read_write> dark: array<u32>;
@group(0) @binding(5) var<storage,read_write> result: array<vec4<f32>>;
var<workgroup> heap: array<vec4<u32>,1000>;
var<workgroup> histogram: array<vec4<u32>,256>;

@compute @workgroup_size(8,8)
fn dehaze_prepare(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy>=p.shape.zw) { return; }
    let start=p.region.xy+id.xy*p.region.zw/p.shape.zw;
    let end=p.region.xy+(id.xy+1u)*p.region.zw/p.shape.zw;
    var sum=vec3(0.); var count=0u;
    for(var y=start.y;y<end.y;y++) { for(var x=start.x;x<end.x;x++) {
        let n=(y*p.shape.x+x)*3u;
        var rgb=vec3(src[n],src[n+1u],src[n+2u])*p.normalization.x;
        if p.selection.w!=0u { rgb=vec3(rgb.y); }
        else { rgb=vec3(dot(rgb,p.basis.to_camera[0].xyz),dot(rgb,p.basis.to_camera[1].xyz),dot(rgb,p.basis.to_camera[2].xyz)); }
        sum+=rgb; count++;
    }}
    let rgb=vec3<u32>(clamp(sum/f32(max(count,1u)),vec3(0.),vec3(1.))*65535.+0.5);
    let intensity=(rgb.x>>2u)+(rgb.y>>1u)+(rgb.z>>2u);
    pixels[id.y*p.shape.z+id.x]=vec4(rgb,intensity);
}
@compute @workgroup_size(8,8)
fn dehaze_horizontal(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy>=p.shape.zw) { return; }
    var value=65535u;
    for(var dx=-i32(p.selection.x);dx<=i32(p.selection.x);dx++) {
        let x=u32(clamp(i32(id.x)+dx,0,i32(p.shape.z)-1));
        let rgb=pixels[id.y*p.shape.z+x].xyz;
        value=min(value,min(rgb.x,min(rgb.y,rgb.z)));
    }
    horizontal[id.y*p.shape.z+id.x]=value;
}
@compute @workgroup_size(8,8)
fn dehaze_vertical(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy>=p.shape.zw) { return; }
    var value=65535u;
    for(var dy=-i32(p.selection.x);dy<=i32(p.selection.x);dy++) {
        let y=u32(clamp(i32(id.y)+dy,0,i32(p.shape.w)-1));
        value=min(value,horizontal[y*p.shape.z+id.x]);
    }
    dark[id.y*p.shape.z+id.x]=value;
}
// Smaller dark minima and brighter ties are worse candidates. Coordinates settle exact ties.
fn worse(a:vec4<u32>,b:vec4<u32>)->bool {
    return a.x<b.x || (a.x==b.x && (a.y>b.y || (a.y==b.y && a.z>b.z)));
}
@compute @workgroup_size(1)
fn dehaze_estimate() {
    for(var b=0u;b<256u;b++) { histogram[b]=vec4(0u); }
    var count=0u;
    let radius=p.selection.x;
    for(var y=0u;y<p.shape.w;y++) { for(var x=0u;x<p.shape.z;x++) {
        let index=y*p.shape.z+x;
        let rgb=pixels[index];
        histogram[rgb.x>>8u].x++;
        histogram[rgb.y>>8u].y++;
        histogram[rgb.z>>8u].z++;
        // Manual color still needs brightness calibration, not neighborhood color selection.
        if p.normalization.y>0.5 { continue; }
        if x<radius || y<radius || x+radius>=p.shape.z || y+radius>=p.shape.w { continue; }
        let score=dark[index];
        if score==0u || rgb.w>p.selection.y { continue; }
        let candidate=vec4(score,rgb.w,index,0u);
        if count<p.selection.z {
            var child=count; count++;
            loop {
                if child==0u { break; }
                let parent=(child-1u)/2u;
                if !worse(candidate,heap[parent]) { break; }
                heap[child]=heap[parent]; child=parent;
            }
            heap[child]=candidate;
        } else if worse(heap[0],candidate) {
            var parent=0u;
            loop {
                var child=parent*2u+1u;
                if child>=count { break; }
                if child+1u<count { if worse(heap[child+1u],heap[child]) { child++; } }
                if !worse(heap[child],candidate) { break; }
                heap[parent]=heap[child]; parent=child;
            }
            heap[parent]=candidate;
        }
    }}
    var sum=vec3(0u);
    for(var i=0u;i<count;i++) { sum+=pixels[heap[i].z].xyz; }
    var color=vec3<f32>(sum)/(65535.*f32(max(count,1u)));
    if p.selection.w==0u {
        color=vec3(dot(color,p.basis.to_srgb[0].xyz),dot(color,p.basis.to_srgb[1].xyz),dot(color,p.basis.to_srgb[2].xyz));
    }
    // Canonical storage and the swatch share this bounded color. Auto and manual
    // both convert it back through the same basis, so freezing Auto is exact.
    color=clamp(color,vec3(0.),vec3(1.));
    result[0]=vec4(color,select(0.,1.,count>0u && any(color>vec3(0.))));
    let percentile_count=max(1u,p.shape.z*p.shape.w/100u);
    var lows=vec3(0.); var highs=vec3(1.);
    var low_count=vec3(0u); var high_count=vec3(0u);
    for(var b=0u;b<256u;b++) {
        let lc=histogram[b].xyz; let hc=histogram[255u-b].xyz;
        lows=select(lows,vec3(f32(b)/255.),low_count<vec3(percentile_count));
        highs=select(highs,vec3(f32(255u-b)/255.),high_count<vec3(percentile_count));
        low_count+=lc; high_count+=hc;
    }
    lows=min(lows,vec3(.95)); highs=max(highs,vec3(.05));
    let center=sqrt(lows*highs);
    let narrow=highs<=4.*lows;
    lows=select(lows,min(vec3(.95),.5*center),narrow);
    highs=select(highs,max(vec3(.05),2.*center),narrow);
    result[1]=vec4(min(lows.x,min(lows.y,lows.z)),max(highs.x,max(highs.y,highs.z)),0.,0.);
}

@group(0) @binding(6) var<storage,read_write> partial_heap: array<vec4<u32>>;
@group(0) @binding(7) var<storage,read_write> partial_hist: array<vec4<u32>>;
@group(0) @binding(8) var<storage,read_write> partial_count: array<u32>;

// Every partition keeps its own top K under the total score/intensity/index order.
// Any omitted record has K better records in its partition, hence cannot be global top K.
@compute @workgroup_size(1)
fn dehaze_partition(@builtin(workgroup_id) group: vec3<u32>) {
    let part=group.x; let base=part*1000u; let hbase=part*256u;
    for(var b=0u;b<256u;b++) { partial_hist[hbase+b]=vec4(0u); }
    var count=0u;
    let n=p.shape.z*p.shape.w;
    let start=n*part/16u; let end=n*(part+1u)/16u;
    let radius=p.selection.x;
    for(var index=start;index<end;index++) {
        let x=index%p.shape.z; let y=index/p.shape.z;
        let rgb=pixels[index];
        partial_hist[hbase+(rgb.x>>8u)].x++;
        partial_hist[hbase+(rgb.y>>8u)].y++;
        partial_hist[hbase+(rgb.z>>8u)].z++;
        if p.normalization.y>0.5 { continue; }
        if x<radius || y<radius || x+radius>=p.shape.z || y+radius>=p.shape.w { continue; }
        let score=dark[index];
        if score==0u || rgb.w>p.selection.y { continue; }
        let candidate=vec4(score,rgb.w,index,0u);
        if count<p.selection.z {
            var child=count; count++;
            loop {
                if child==0u { break; }
                let parent=(child-1u)/2u;
                if !worse(candidate,partial_heap[base+parent]) { break; }
                partial_heap[base+child]=partial_heap[base+parent]; child=parent;
            }
            partial_heap[base+child]=candidate;
        } else if worse(partial_heap[base],candidate) {
            var parent=0u;
            loop {
                var child=parent*2u+1u;
                if child>=count { break; }
                if child+1u<count { if worse(partial_heap[base+child+1u],partial_heap[base+child]) { child++; } }
                if !worse(partial_heap[base+child],candidate) { break; }
                partial_heap[base+parent]=partial_heap[base+child]; parent=child;
            }
            partial_heap[base+parent]=candidate;
        }
    }
    partial_count[part]=count;
}
@compute @workgroup_size(1)
fn dehaze_merge() {
    for(var b=0u;b<256u;b++) {
        var total=vec4(0u);
        for(var part=0u;part<16u;part++) { total+=partial_hist[part*256u+b]; }
        histogram[b]=total;
    }
    var count=0u;
    for(var part=0u;part<16u;part++) { for(var i=0u;i<partial_count[part];i++) {
        let candidate=partial_heap[part*1000u+i];
        if count<p.selection.z {
            var child=count; count++;
            loop {
                if child==0u { break; }
                let parent=(child-1u)/2u;
                if !worse(candidate,heap[parent]) { break; }
                heap[child]=heap[parent]; child=parent;
            }
            heap[child]=candidate;
        } else if worse(heap[0],candidate) {
            var parent=0u;
            loop {
                var child=parent*2u+1u;
                if child>=count { break; }
                if child+1u<count { if worse(heap[child+1u],heap[child]) { child++; } }
                if !worse(heap[child],candidate) { break; }
                heap[parent]=heap[child]; parent=child;
            }
            heap[parent]=candidate;
        }
    }}
    var sum=vec3(0u);
    for(var i=0u;i<count;i++) { sum+=pixels[heap[i].z].xyz; }
    var color=vec3<f32>(sum)/(65535.*f32(max(count,1u)));
    if p.selection.w==0u {
        color=vec3(dot(color,p.basis.to_srgb[0].xyz),dot(color,p.basis.to_srgb[1].xyz),dot(color,p.basis.to_srgb[2].xyz));
    }
    // Canonical storage and the swatch share this bounded color. Auto and manual
    // both convert it back through the same basis, so freezing Auto is exact.
    color=clamp(color,vec3(0.),vec3(1.));
    result[0]=vec4(color,select(0.,1.,count>0u && any(color>vec3(0.))));
    let percentile_count=max(1u,p.shape.z*p.shape.w/100u);
    var lows=vec3(0.); var highs=vec3(1.);
    var low_count=vec3(0u); var high_count=vec3(0u);
    for(var b=0u;b<256u;b++) {
        let lc=histogram[b].xyz; let hc=histogram[255u-b].xyz;
        lows=select(lows,vec3(f32(b)/255.),low_count<vec3(percentile_count));
        highs=select(highs,vec3(f32(255u-b)/255.),high_count<vec3(percentile_count));
        low_count+=lc; high_count+=hc;
    }
    lows=min(lows,vec3(.95)); highs=max(highs,vec3(.05));
    let center=sqrt(lows*highs);
    let narrow=highs<=4.*lows;
    lows=select(lows,min(vec3(.95),.5*center),narrow);
    highs=select(highs,max(vec3(.05),2.*center),narrow);
    result[1]=vec4(min(lows.x,min(lows.y,lows.z)),max(highs.x,max(highs.y,highs.z)),0.,0.);
}
