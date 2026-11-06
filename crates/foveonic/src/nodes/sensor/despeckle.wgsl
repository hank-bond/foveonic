// Reference extrema detector, deterministic bank compaction, and dependency-ordered
// trimmed/chroma-anchored repair. Adapted from Foveonic despeckle.rs / repair.rs.
struct Params { source: vec4<u32>, area: vec4<u32>, detector: vec4<u32> };
@group(0) @binding(0) var<storage, read> input_pixels: array<f32>;
@group(0) @binding(1) var<storage, read> thresholds: array<f32>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read_write> pixels: array<f32>;
@group(0) @binding(4) var<storage, read_write> marks: array<u32>;
@group(0) @binding(5) var<storage, read_write> counts: array<u32>;
@group(0) @binding(6) var<storage, read_write> records: array<vec2<u32>>;
@group(0) @binding(7) var<storage, read> original_pixels: array<f32>;
// Marks hold layer flags and a temporary block rank above bit 7. Scatter keeps
// only retained layer flags, then completed repairs clear them. Record flags
// additionally carry readiness between the classification and repair dispatches.
const LAYER_MASK: u32 = 7u;
const READY_BIT: u32 = 8u;
const RANK_SHIFT: u32 = 8u;
// Match the conservative native-code highlight guard used by layer filtering.
const NATIVE_HIGHLIGHT_CODE: f32 = 3900.;
const NATIVE_DARK_CODE: f32 = NATIVE_HIGHLIGHT_CODE / 8.;
var<workgroup> ranks: array<u32, 256>;
const offsets = array<vec2<i32>, 8>(vec2(-1,-1), vec2(0,-1), vec2(1,-1), vec2(-1,0), vec2(1,0), vec2(-1,1), vec2(0,1), vec2(1,1));
fn sample(p: u32, c: u32) -> f32 { return input_pixels[p * 3u + c]; }
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
    if x >= params.area.x && x <= params.area.z && y >= params.area.y && y <= params.area.w {
        for (var c=0u; c<3u; c++) { if extremum(p,c) { layers |= 4u >> c; } }
    }
    for (var c=0u; c<3u; c++) { pixels[p*3u+c] = input_pixels[p*3u+c]; }
    rank_block(layers, p, local, group.x);
}
fn rank_block(layers: u32, p: u32, local: u32, block: u32) {
    ranks[local] = select(0u,1u,layers!=0u);
    workgroupBarrier();
    for (var step=1u; step<256u; step*=2u) {
        var previous=0u;
        if local >= step { previous=ranks[local-step]; }
        workgroupBarrier();
        ranks[local] += previous;
        workgroupBarrier();
    }
    marks[p] = layers | (ranks[local] << RANK_SHIFT);
    if local==255u { counts[block]=ranks[local]; }
}
@compute @workgroup_size(1)
fn prefix(@builtin(workgroup_id)group:vec3<u32>) {
    let first = group.x * params.source.z / 8u;
    let end = (group.x + 1u) * params.source.z / 8u;
    let bank=group.x;
        var sum=0u;
        for (var block=first; block<end; block++) {
            let count=counts[block]; counts[block]=sum; sum+=count;
        }
        counts[params.source.z+bank]=min(sum,params.source.w);
}
@compute @workgroup_size(256)
fn scatter(@builtin(global_invocation_id) id: vec3<u32>, @builtin(workgroup_id) group: vec3<u32>) {
    let mark=marks[id.x];
    marks[id.x]=0u;
    if (mark & LAYER_MASK)==0u { return; }
    let rank=counts[group.x]+(mark>>RANK_SHIFT)-1u;
    // Dropped detections must not block retained repairs, including across banks.
    if rank >= params.source.w { return; }
    let bank=((group.x+1u)*8u-1u)/params.source.z;
    records[bank*params.source.w+rank]=vec2(id.x,mark&LAYER_MASK);
    marks[id.x]=mark&LAYER_MASK;
}
fn color(p: u32) -> vec3<f32> { return vec3(pixels[p*3u],pixels[p*3u+1u],pixels[p*3u+2u]); }
fn anchor(original: vec3<f32>, sum: vec3<f32>, c: u32, a: u32) -> f32 { return original[a]+(sum[c]-sum[a])/6.0; }
// A ready record has no earlier pending neighbor. Its later neighbors cannot
// be ready in the same round, so all pixel reads and writes in a round are disjoint
// from other repairs' writes. Classification never changes the pending mask.
@compute @workgroup_size(256)
fn classify_ready(@builtin(global_invocation_id) id: vec3<u32>) {
    let bank = id.y;
    if id.x >= counts[params.source.z + bank] { return; }
    let index = bank * params.source.w + id.x;
    let record = records[index];
    records[index].y = record.y & LAYER_MASK;
    let p = record.x;
    if marks[p] == 0u { return; }
    let w = params.source.x;
    let xy = vec2(p % w, p / w);
    // Serial replay skips these centers. Clear them in the next repair dispatch.
    if xy.x < 2u || xy.y < 2u || xy.x >= w - 1u || xy.y >= params.source.y - 1u {
        records[index].y |= READY_BIT;
        return;
    }
    // Only the left pixel and three pixels in the preceding row come earlier.
    // The shared mask includes dependencies across bank boundaries.
    if marks[p - 1u] != 0u || marks[p - w - 1u] != 0u ||
       marks[p - w] != 0u || marks[p - w + 1u] != 0u {
        return;
    }
    records[index].y |= READY_BIT;
}
@compute @workgroup_size(256)
fn repair_ready(@builtin(global_invocation_id) id: vec3<u32>) {
    let bank = id.y;
    if id.x >= counts[params.source.z + bank] { return; }
    let record = records[bank * params.source.w + id.x];
    if (record.y & READY_BIT) == 0u { return; }
    repair_record(record.x, record.y & LAYER_MASK);
    marks[record.x] = 0u;
}
// Completed repairs have cleared their layer bits in marks. Reuse the detector's
// rank storage to compact only the remaining records before ordered replay.
@compute @workgroup_size(256)
fn remaining(
    @builtin(global_invocation_id) id: vec3<u32>,
    @builtin(local_invocation_index) local: u32,
    @builtin(workgroup_id) group: vec3<u32>,
) {
    rank_block(marks[id.x] & LAYER_MASK, id.x, local, group.x);
}
fn repair_record(p: u32, layers: u32) {
    let xy=vec2<i32>(i32(p%params.source.x),i32(p/params.source.x));
    if xy.x<2 || xy.y<2 || xy.x>=i32(params.source.x)-1 || xy.y>=i32(params.source.y)-1 { return; }
    var sum=vec3(0.0); var low=vec3(0.0); var high=vec3(0.0);
    for (var n=0u; n<8u; n++) {
        let q=xy+offsets[n]; let v=color(u32(q.y)*params.source.x+u32(q.x));
        low=select(min(low,v),v,all(low==vec3(0.0)));
        high=select(max(high,v),v,all(high==vec3(0.0)));
        sum+=v;
    }
    sum-=high+low;
    let original=color(p); var result=original;
    switch layers {
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
@compute @workgroup_size(1)
fn repair() {
    // Long chains retain native bank/row order after the bounded parallel rounds.
    for (var bank=0u; bank<8u; bank++) {
        let count=counts[params.source.z+bank];
        for (var index=0u; index<count; index++) {
            let record=records[bank*params.source.w+index];
            repair_record(record.x,record.y);
        }
    }
}


fn protected_highlight(p: u32, c: u32, xy: vec2<i32>) -> bool {
    if original_pixels[p*3u+c] < NATIVE_HIGHLIGHT_CODE { return false; }
    // Override clipping protection only for a lone layer spike. Require an
    // eightfold separation from all adjacent samples and both other layers.
    for (var layer = 0u; layer < 3u; layer++) {
        if layer != c && original_pixels[p*3u+layer] >= NATIVE_DARK_CODE { return true; }
    }
    for (var n = 0u; n < 8u; n++) {
        let q = xy + offsets[n];
        let neighbor = u32(q.y)*params.source.x+u32(q.x);
        for (var layer = 0u; layer < 3u; layer++) {
            if original_pixels[neighbor*3u+layer] >= NATIVE_DARK_CODE { return true; }
        }
    }
    return false;
}

// Optional positive compact-outlier filter. Each radius reads the unchanged
// image. A straight edge or line retains at least one high ring neighbor.
@compute @workgroup_size(8,8)
fn compact(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.source.x || id.y >= params.source.y { return; }
    let p = id.y * params.source.x + id.x;
    for (var c = 0u; c < 3u; c++) { pixels[p*3u+c] = sample(p,c); }
    if id.x < params.area.x || id.x > params.area.z || id.y < params.area.y || id.y > params.area.w { return; }
    for (var c = 0u; c < 3u; c++) {
        if protected_highlight(p, c, vec2<i32>(id.xy)) { return; }
    }
    // Use one radius and one set of donor pixels for all layers so unequal
    // thresholds cannot leave a colored residual from a white speckle.
    for (var radius = 1u; radius <= params.detector.y; radius++) {
        var ring: array<vec3<f32>, 8>;
        var sum = vec3(0.);
        for (var n = 0u; n < 8u; n++) {
            let q = vec2<i32>(id.xy) + offsets[n] * i32(radius);
            let neighbor = u32(q.y)*params.source.x+u32(q.x);
            ring[n] = vec3(sample(neighbor,0u),sample(neighbor,1u),sample(neighbor,2u));
            sum += ring[n];
        }
        for (var c = 0u; c < 3u; c++) {
            var low = 0u; var high = 0u;
            for (var n = 1u; n < 8u; n++) {
                if ring[n][c] < ring[low][c] { low = n; }
                // Choose the last maximum so a flat ring excludes two donors.
                if ring[n][c] >= ring[high][c] { high = n; }
            }
            let center = sample(p,c);
            let threshold = thresholds[c*4096u+u32(clamp(center,0.,4095.))];
            if center > ring[high][c] + threshold {
                let result = (sum-ring[high]-ring[low])/6.;
                for (var layer = 0u; layer < 3u; layer++) { pixels[p*3u+layer] = result[layer]; }
                return;
            }
        }
    }
}
