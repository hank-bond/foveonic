// Reference extrema detector, deterministic bank compaction, and sequential
// trimmed/chroma-anchored repair. Adapted from Foveonic despeckle.rs / repair.rs.
struct Params { source: vec4<u32>, area: vec4<u32>, gain: vec4<f32> };
@group(0) @binding(0) var<storage, read> input_pixels: array<f32>;
@group(0) @binding(1) var<storage, read> thresholds: array<f32>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read_write> pixels: array<f32>;
@group(0) @binding(4) var<storage, read_write> marks: array<u32>;
@group(0) @binding(5) var<storage, read_write> counts: array<u32>;
@group(0) @binding(6) var<storage, read_write> records: array<vec2<u32>>;
var<workgroup> ranks: array<u32, 256>;
const offsets = array<vec2<i32>, 8>(vec2(-1,-1), vec2(0,-1), vec2(1,-1), vec2(-1,0), vec2(1,0), vec2(-1,1), vec2(0,1), vec2(1,1));
fn sample(p: u32, c: u32) -> f32 { return input_pixels[p * 3u + c] / params.gain.x; }
fn extremum(p: u32, c: u32) -> bool {
    let w = params.source.x;
    let center = sample(p, c);
    let left = sample(p - 1u, c); let right = sample(p + 1u, c);
    let top = sample(p - w, c); let bottom = sample(p + w, c);
    if center > left && center > right {
        let peak = min(center, 4095.0);
        let threshold = thresholds[c * 4096u + u32(max(peak, 0.0))];
        if peak <= max(max(top,bottom),max(left,right)) + threshold { return false; }
        let diagonal = max(max(sample(p-w-1u,c),sample(p-w+1u,c)),max(sample(p+w-1u,c),sample(p+w+1u,c)));
        return peak > diagonal + threshold;
    }
    if center < left && center < right {
        let threshold = thresholds[c * 4096u + u32(clamp(center, 0.0, 4095.0))];
        if center >= min(min(top,bottom),min(left,right)) - threshold { return false; }
        let diagonal = min(min(sample(p-w-1u,c),sample(p-w+1u,c)),min(sample(p+w-1u,c),sample(p+w+1u,c)));
        return center < diagonal - threshold;
    }
    return false;
}
@compute @workgroup_size(256)
fn detect(@builtin(global_invocation_id) id: vec3<u32>, @builtin(local_invocation_index) local: u32, @builtin(workgroup_id) group: vec3<u32>) {
    let p = id.x;
    let x = p % params.source.x; let y = p / params.source.x;
    var layers = 0u;
    if params.gain.x > 0.0 && x >= params.area.x && x <= params.area.z && y >= params.area.y && y <= params.area.w {
        for (var c=0u; c<3u; c++) { if extremum(p,c) { layers |= 4u >> c; } }
    }
    for (var c=0u; c<3u; c++) { pixels[p*3u+c] = input_pixels[p*3u+c]; }
    ranks[local] = select(0u,1u,layers!=0u);
    workgroupBarrier();
    for (var step=1u; step<256u; step*=2u) {
        var previous=0u;
        if local >= step { previous=ranks[local-step]; }
        workgroupBarrier();
        ranks[local] += previous;
        workgroupBarrier();
    }
    marks[p] = layers | (ranks[local] << 8u);
    if local==255u { counts[group.x]=ranks[local]; }
}
@compute @workgroup_size(1)
fn prefix(@builtin(workgroup_id)group:vec3<u32>) {
    let blocks = params.source.z / 8u;
    let bank=group.x;
        var sum=0u;
        for (var block=bank*blocks; block<(bank+1u)*blocks; block++) {
            let count=counts[block]; counts[block]=sum; sum+=count;
        }
        counts[params.source.z+bank]=min(sum,params.source.w);
}
@compute @workgroup_size(256)
fn scatter(@builtin(global_invocation_id) id: vec3<u32>, @builtin(workgroup_id) group: vec3<u32>) {
    let mark=marks[id.x];
    if (mark & 7u)==0u { return; }
    let rank=counts[group.x]+(mark>>8u)-1u;
    if rank >= params.source.w { return; }
    let bank=group.x/(params.source.z/8u);
    records[bank*params.source.w+rank]=vec2(id.x,mark&7u);
}
fn color(p: u32) -> vec3<f32> { return vec3(pixels[p*3u],pixels[p*3u+1u],pixels[p*3u+2u]); }
fn anchor(original: vec3<f32>, sum: vec3<f32>, c: u32, a: u32) -> f32 { return original[a]+(sum[c]-sum[a])/6.0; }
@compute @workgroup_size(1)
fn repair() {
    // A single invocation preserves the native read-after-write record order,
    // including neighboring detections. Detection never reads these writes.
    for (var bank=0u; bank<8u; bank++) {
        let count=counts[params.source.z+bank];
        for (var index=0u; index<count; index++) {
            let record=records[bank*params.source.w+index];
            let p=record.x; let xy=vec2<i32>(i32(p%params.source.x),i32(p/params.source.x));
            if xy.x<2 || xy.y<2 || xy.x>=i32(params.source.x)-1 || xy.y>=i32(params.source.y)-1 { continue; }
            var sum=vec3(0.0); var low=vec3(0.0); var high=vec3(0.0);
            for (var n=0u; n<8u; n++) {
                let q=xy+offsets[n]; let v=color(u32(q.y)*params.source.x+u32(q.x));
                low=select(min(low,v),v,all(low==vec3(0.0)));
                high=select(max(high,v),v,all(high==vec3(0.0)));
                sum+=v;
            }
            sum-=high+low;
            let original=color(p); var result=original;
            switch record.y {
                case 1u: { result.z=(anchor(original,sum,2u,0u)+anchor(original,sum,2u,1u))*0.5; }
                case 2u: { result.y=(anchor(original,sum,1u,0u)+anchor(original,sum,1u,2u))*0.5; }
                case 3u: { result.y=anchor(original,sum,1u,0u); result.z=anchor(original,sum,2u,0u); }
                case 4u: { result.x=(anchor(original,sum,0u,1u)+anchor(original,sum,0u,2u))*0.5; }
                case 5u: { result.x=anchor(original,sum,0u,1u); result.z=anchor(original,sum,2u,1u); }
                case 6u: { result.x=anchor(original,sum,0u,2u); result.y=anchor(original,sum,1u,2u); }
                case 7u: { result=sum/6.0; }
                default: {}
            }
            if all(abs(result)<=vec3(3.402823e38)) { pixels[p*3u]=result.x; pixels[p*3u+1u]=result.y; pixels[p*3u+2u]=result.z; }
        }
    }
}
