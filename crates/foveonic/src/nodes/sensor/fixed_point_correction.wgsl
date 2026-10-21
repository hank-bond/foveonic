// Signed Q14 correction with explicit units and ordered gain increments.
struct Params {
    source: vec4<u32>, flare: vec4<f32>, final_gain: vec4<f32>,
    linear: vec4<i32>, quadratic: vec4<i32>, segment_scale: vec4<i32>,
    segment_bias: vec4<i32>, controls: vec4<u32>,
};
@group(0) @binding(0) var<storage, read> input_pixels: array<f32>;
@group(0) @binding(1) var<storage, read> offsets: array<i32>;
@group(0) @binding(2) var<storage, read> knots: array<f32>;
@group(0) @binding(3) var<uniform> params: Params;
@group(0) @binding(4) var<storage, read_write> output_pixels: array<f32>;
@group(0) @binding(5) var<storage, read_write> measured_offsets: array<i32>;

// Narrow by wrapping to signed 16-bit, rather than clamping.
fn signed16(value: i32) -> i32 { return (value << 16u) >> 16u; }
@compute @workgroup_size(8,8)
fn scale(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x>=params.source.x || id.y>=params.source.y {return;}
    let base=(id.y*params.source.x+id.x)*3u;
    for(var c=0u;c<3u;c++) {
        var value=i32(input_pixels[base+c]);
        if params.source.w!=0u {value=signed16(i32((bitcast<u32>(value)*params.source.w)>>14u));}
        output_pixels[base+c]=f32(value);
    }
}
// Cooperatively fetch sparse samples, then preserve each channel's reference
// Float32 accumulation order. A parallel sum would change rounded flare offsets.
// Three independent workgroups share no state. Each channel owns a 1 KiB tile.
const FLARE_TILE_SAMPLES: u32 = 256u;
const FLARE_THREADS: u32 = 256u;
var<workgroup> flare_samples: array<f32, FLARE_TILE_SAMPLES>;
@compute @workgroup_size(FLARE_THREADS)
fn statistics(@builtin(local_invocation_index) lane: u32, @builtin(workgroup_id) group: vec3<u32>) {
    let columns = (params.source.x + 1u) / 4u;
    let rows = (params.source.y + 1u) / 4u;
    let count = columns * rows;
    var sum = 0.0;
    for (var start = 0u; start < count; start += FLARE_TILE_SAMPLES) {
        for (var position = lane; position < min(FLARE_TILE_SAMPLES, count - start); position += FLARE_THREADS) {
            let sample = start + position;
            // Reference lattice: every fourth row/column, starting at two.
            let x = 2u + (sample % columns) * 4u;
            let y = 2u + (sample / columns) * 4u;
            let index = (y * params.source.x + x) * 3u;
            flare_samples[position] = input_pixels[index + group.x];
        }
        workgroupBarrier();
        if lane == 0u {
            let limit = min(FLARE_TILE_SAMPLES, count - start);
            var i = 0u;
            // Prefetch four values before the dependent additions. Their
            // original order still determines every rounded partial sum.
            for (; i + 4u <= limit; i += 4u) {
                let values = vec4(flare_samples[i], flare_samples[i + 1u], flare_samples[i + 2u], flare_samples[i + 3u]);
                sum = sum + values.x;
                sum = sum + values.y;
                sum = sum + values.z;
                sum = sum + values.w;
            }
            for (; i < limit; i++) { sum = sum + flare_samples[i]; }
        }
        workgroupBarrier();
    }
    if lane == 0u {
        let c = group.x;
        let average=sum/f32(count);
        let factor=params.flare[c]*16384.;
        let product=average*factor;
        // Recover the product rounding residual before truncating the Q14 offset.
        let residual=fma(average,factor,-product);
        let integral=i32(product);
        let fraction=(product-f32(integral))+residual;
        var adjustment=0;
        if fraction>=1. || (integral<0 && fraction>0.) {adjustment=1;}
        if fraction<=-1. || (integral>0 && fraction<0.) {adjustment=-1;}
        measured_offsets[c]=integral+adjustment;
    }
}
@compute @workgroup_size(64)
fn correct(@builtin(global_invocation_id) id: vec3<u32>){
    let y=id.x;if y>=params.source.y{return;}
    let width=params.source.x;
    let spacing=f32(width)/f32(params.source.z-1u);
    var segment=0u;var boundary=spacing;
    var current=vec3<i32>(0);var increment=vec3<i32>(0);
    // Accumulate gains in Q22. Dropping eight bits below yields a Q14 multiplier.
    let factors=params.final_gain.xyz*4194304.;
    let row=y*params.source.z*3u;
    for(var c=0u;c<3u;c++){
        let a=knots[row+c];let b=knots[row+3u+c];
        current[c]=i32(a*factors[c]);increment[c]=i32(((b-a)/spacing)*factors[c]);
    }
    var previous=vec3<i32>(0);
    for(var x=0u;x<width;x++){
        if f32(x)>=boundary {
            segment++;boundary+=spacing;
            for(var c=0u;c<3u;c++){
                let a=knots[row+segment*3u+c];let b=knots[row+(segment+1u)*3u+c];
                current[c]=i32(a*factors[c]);increment[c]=i32(((b-a)/spacing)*factors[c]);
            }
        }
        let base=(y*width+x)*3u;
        for(var c=0u;c<3u;c++){
            let s=i32(input_pixels[base+c]);let p=select(previous[c],s,x==0u);let delta=s-p;
            var z=s*16384+delta*params.linear[c]+i32((bitcast<u32>(delta*delta)>>14u))*params.quadratic[c];
            if params.controls.x>1u && x%params.controls.x==0u {z+=p*params.segment_scale[c]+params.segment_bias[c];}
            let corrected=(z-offsets[c])>>14u;
            let gained=(corrected*(current[c]>>8u))>>14u;
            // Reference headroom limit precedes signed-16 wrapping, including negatives.
            output_pixels[base+c]=f32(signed16(min(32000,gained)));
            previous[c]=s;current[c]+=increment[c];
        }
    }
}
