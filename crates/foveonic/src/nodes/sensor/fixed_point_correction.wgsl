// Signed Q14 correction with explicit units and ordered gain increments.
struct Params {
    source: vec4<u32>, flare: vec4<f32>, final_gain: vec4<f32>,
    linear: vec4<i32>, quadratic: vec4<i32>, segment_scale: vec4<i32>,
    segment_bias: vec4<i32>, controls: vec4<u32>,
    drift: array<vec4<f32>,6>, shields: array<vec4<u32>,2>,
    row_spans: vec4<u32>, drift_controls: vec4<u32>, row_filter: vec4<f32>,
    polynomial: array<vec4<i32>,9>,
};
@group(0) @binding(0) var<storage, read> input_pixels: array<f32>;
@group(0) @binding(1) var<storage, read> offsets: array<i32>;
@group(0) @binding(2) var<storage, read> knots: array<f32>;
@group(0) @binding(3) var<uniform> params: Params;
@group(0) @binding(4) var<storage, read_write> output_pixels: array<f32>;
@group(0) @binding(5) var<storage, read_write> measured_offsets: array<i32>;

// Paired Float32 controls defer integer truncation until after interpolation.
// GPU arithmetic can differ from the reference double path at integer boundaries.
fn ds(v: f32) -> vec2<f32> { return vec2(v,0.); }
fn add(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let hi=a.x+b.x; let v=hi-a.x;
    let lo=((b.x-v)+(a.x-(hi-v)))+a.y+b.y;
    let result=hi+lo; return vec2(result,lo-(result-hi));
}
fn sub(a: vec2<f32>,b: vec2<f32>) -> vec2<f32> { return add(a,-b); }
fn mul(a: vec2<f32>,b: vec2<f32>) -> vec2<f32> {
    let hi=a.x*b.x; let lo=fma(a.x,b.x,-hi)+a.x*b.y+a.y*b.x;
    let result=hi+lo; return vec2(result,lo-(result-hi));
}
fn div_pair(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let q=a.x/b.x; return add(ds(q),ds(sub(a,mul(ds(q),b)).x/b.x));
}
fn trunc_pair(a: vec2<f32>) -> i32 {
    let hi=i32(a.x); let remainder=(a.x-f32(hi))+a.y;
    var adjust=0;
    if remainder>=1. || (hi<0 && remainder>0.) {adjust=1;}
    if remainder<=-1. || (hi>0 && remainder<0.) {adjust=-1;}
    return hi+adjust;
}
// Drift slopes and intercepts use paired Float32 camera-code values.
// Row-offset scratch retains both components until final integer truncation.
fn parameter_drift(index: u32)  ->  vec2<f32> {
    let v=params.drift[index/2u];
    return vec2(v[(index%2u)*2u], v[(index%2u)*2u+1u]);
}

fn load_drift(index: u32)  ->  vec2<f32> {
    let at=3u+index*2u;
    return vec2(bitcast<f32>(offsets[at]), bitcast<f32>(offsets[at+1u]));
}

fn measured_drift(index: u32)  ->  vec2<f32> {
    let at=3u+index*2u;
    return vec2(bitcast<f32>(measured_offsets[at]), bitcast<f32>(measured_offsets[at+1u]));
}

fn store_drift(index: u32, v: vec2<f32>) {
    let at=3u+index*2u;
    measured_offsets[at]=bitcast<i32>(v.x);
    measured_offsets[at+1u]=bitcast<i32>(v.y);
}

fn shield_line(rect: vec4<u32>, c: u32)  ->  vec4<f32> {
    if rect.z<=rect.x || rect.w<=rect.y {
        return vec4(0.);
    }
    var sum=ds(0.);
    let count=f32((rect.z-rect.x)*(rect.w-rect.y));
    for(var y=rect.y; y<rect.w; y++) {
        for(var x=rect.x; x<rect.z; x++) {
            sum=add(sum, ds(input_pixels[(y*params.source.x+x)*3u+c]));
        }
    }
    let mean=div_pair(sum, ds(count));
    var covariance=ds(0.);
    let center=i32((rect.x+rect.z)/2u);
    for(var y=rect.y; y<rect.w; y++) {
        for(var x=rect.x; x<rect.z; x++) {
            covariance=add(covariance, mul(sub(ds(input_pixels[(y*params.source.x+x)*3u+c]), mean), ds(f32(i32(x)-center))));
        }
    }
    let slope=mul(div_pair(ds(12.), ds(count)), div_pair(covariance, ds(f32(params.source.x))));
    return vec4(slope, mean);
}
// Scratch layout in 32-bit words: a 40-word prefix stores RGB flare, twelve
// paired drift components, and paired flare values. Each row then owns nine
// Q14 correction words and two six-word RGB paired-float smoothing buffers.

fn scratch_index(y: u32, c: u32, second: bool) -> u32 {
    return 40u+params.source.y*9u+select(0u, params.source.y*6u, second)+y*6u+c*2u;
}

fn row_value(y: u32, c: u32, second: bool) -> vec2<f32> {
    let at=scratch_index(y, c, second);
    return vec2(bitcast<f32>(measured_offsets[at]), bitcast<f32>(measured_offsets[at+1u]));
}

fn store_row(y: u32, c: u32, second: bool, v: vec2<f32>) {
    let at=scratch_index(y, c, second);
    measured_offsets[at]=bitcast<i32>(v.x);
    measured_offsets[at+1u]=bitcast<i32>(v.y);
}

fn interpolated_drift(index: u32, y: u32) -> vec2<f32> {
    let a=measured_drift(index);
    let b=measured_drift(index+6u);
    return add(a, mul(sub(b, a), ds(f32(y)/f32(params.source.y-1u))));
}

fn estimate_row(y: u32, c: u32) -> vec4<f32> {
    let mean=trunc_pair(mul(interpolated_drift(c+3u, y), ds(2048.)));
    let slope=trunc_pair(mul(interpolated_drift(c, y), ds(1024.)));
    var estimates: array<vec2<f32>, 2>;
    for(var side=0u; side<2u; side++) {
        let start=params.row_spans[side*2u];
        let end=params.row_spans[side*2u+1u];
        var sum=ds(0.);
        var least=2147483647;
        var greatest=-2147483647;
        // The row estimator uses Q11 coefficients, not rounded-down Q14 words.
        let q=params.linear[c]/8;
        for(var x=start; x<=end; x++) {
            let at=(y*params.source.x+x)*3u+c;
            let sample=i32(input_pixels[at]);
            let previous=i32(input_pixels[at-3u]);
            let value=sample*2048+(sample-previous)*q-mean+select(-slope, slope, side==0u);
            sum=add(sum, ds(f32(value)));
            least=min(least, value);
            greatest=max(greatest, value);
        }
        estimates[side]=div_pair(sub(sub(sum, ds(f32(least))), ds(f32(greatest))), ds(f32((end-start-1u)*2048u)));
    }
    // Validity is shared across RGB, so calculate the other channels as well in
    // the caller. The ordered pass writes all three channels for row estimation.
    return vec4(estimates[0], estimates[1]);
}
// One ordered row-estimation pass follows the independent flare workgroups.
// Only shield columns are read. Paired Float32 state avoids a CPU image copy.

fn estimate_rows() {
    let height=params.source.y;
    for(var y=0u; y<height; y++) {
        var sides: array<vec4<f32>, 3>;
        var left_valid=true;
        var right_valid=true;
        for(var c=0u; c<3u; c++) {
            sides[c]=estimate_row(y, c);
            left_valid=left_valid && sides[c].x<100.;
            right_valid=right_valid && sides[c].z<100.;
        }
        for(var c=0u; c<3u; c++) {
            var value=ds(0.);
            if left_valid && right_valid {
                value=mul(add(sides[c].xy, mul(sides[c].zw, ds(3.))), ds(0.25));
            }
            else if left_valid {
                value=sides[c].xy;
            }
            else if right_valid {
                value=sides[c].zw;
            }
            store_row(y, c, false, value);
        }
    }
    let top=params.drift_controls.x;
    let bottom=params.drift_controls.y;
    for(var y=0u; y<top; y++) {
        for(var c=0u; c<3u; c++) {
            store_row(y, c, false, row_value(top+y, c, false));
        }
    }
    for(var y=bottom; y<height; y++) {
        for(var c=0u; c<3u; c++) {
            store_row(y, c, false, row_value(2u*bottom-height+y-bottom, c, false));
        }
    }
    let alpha=params.row_filter.zw;
    let scale=params.row_filter.xy;
    for(var c=0u; c<3u; c++) {
        var previous=row_value(0u, c, false);
        var center=row_value(1u, c, false);
        for(var y=1u; y<height-1u; y++) {
            let following=row_value(max(2u, y), c, false);
            var value=row_value(y, c, false);
            if (center.x<=previous.x && center.x<following.x) || (center.x>previous.x && center.x>following.x) {
                // Select the median as an original pair, preserving its low word.
                if (previous.x<=center.x && center.x<=following.x)||(following.x<=center.x && center.x<=previous.x) {
                    value=center;
                }
                else if (center.x<=previous.x && previous.x<=following.x)||(following.x<=previous.x && previous.x<=center.x) {
                    value=previous;
                }
                else {
                    value=following;
                }
            }
            store_row(y, c, true, value);
            previous=center;
            center=following;
        }
        store_row(height-1u, c, true, mul(add(previous, center), ds(0.5)));
        store_row(0u, c, true, mul(add(row_value(1u, c, true), row_value(3u, c, true)), ds(0.5)));
        var state=row_value(0u, c, true);
        var sum=ds(0.);
        for(var y=0u; y<height; y++) {
            state=add(state, mul(sub(row_value(y, c, true), state), alpha));
            store_row(y, c, true, state);
            sum=add(sum, state);
        }
        let mean=div_pair(sum, ds(f32(height)));
        for(var i=0u; i<height; i++) {
            let y=height-1u-i;
            state=add(state, mul(sub(sub(row_value(y, c, true), mean), state), alpha));
            store_row(y, c, true, mul(state, scale));
        }
    }
    if (params.controls.y & 4u)!=0u {
        var denominator=ds(0.);
        var numerators=array<vec2<f32>, 3>(ds(0.), ds(0.), ds(0.));
        for(var y=0u; y<height; y++) {
            let weight=mul(add(add(row_value(y, 0u, true), mul(row_value(y, 1u, true), ds(2.))), row_value(y, 2u, true)), ds(0.25));
            store_row(y, 0u, false, weight);
            denominator=add(denominator, mul(weight, weight));
            for(var c=0u; c<3u; c++) {
                numerators[c]=add(numerators[c], mul(row_value(y, c, true), weight));
            }
        }
        for(var c=0u; c<3u; c++) {
            let factor=div_pair(numerators[c], denominator);
            for(var y=0u; y<height; y++) {
                store_row(y, c, true, mul(row_value(y, 0u, false), factor));
            }
        }
    }
    for(var y=0u; y<height; y++) {
        for(var c=0u; c<3u; c++) {
            // Add flare before the Q14 truncation. Recover it from the same sparse
            // image mean, without using the already truncated integer flare word.
            // Flare pairs occupy the fixed scratch prefix.
            let flare=vec2(bitcast<f32>(measured_offsets[27u+c]), bitcast<f32>(measured_offsets[30u+c]));
            measured_offsets[40u+y*9u+6u+c]=trunc_pair(mul(add(flare, row_value(y, c, true)), ds(16384.)));
        }
    }
}
// Narrow by wrapping to signed 16-bit, rather than clamping.
fn signed16(value: i32) -> i32 { return (value << 16u) >> 16u; }
@compute @workgroup_size(8,8)
fn scale(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x>=params.source.x || id.y>=params.source.y {return;}
    let base=(id.y*params.source.x+id.x)*3u;
    for(var c=0u;c<3u;c++) {
        var value=i32(input_pixels[base+c]);
        if params.source.w!=0u {
            if (params.controls.y & 1u)!=0u {
                value-=trunc_pair(mul(add(load_drift(c+3u),load_drift(c+9u)),ds(0.5)));
            }
            value=signed16(i32((bitcast<u32>(value)*params.source.w)>>14u));
        }
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
    if params.controls.w==2u {
        if lane==0u {estimate_rows();}
        return;
    }
    if params.controls.w==1u {
        if lane==0u {
            let c=group.x;
            var top=vec4(parameter_drift(c),parameter_drift(c+3u));
            var bottom=vec4(parameter_drift(c+6u),parameter_drift(c+9u));
            if params.drift_controls.z!=0u {
                top=shield_line(params.shields[0],c);bottom=shield_line(params.shields[1],c);
                let red=shield_line(params.shields[0],0u);
                if abs(red.z)>4000. {top=vec4(0.);bottom=vec4(0.);}
            }
            if params.drift_controls.w==0u {top=vec4(0.,0.,top.zw);bottom=vec4(0.,0.,bottom.zw);}
            store_drift(c,top.xy);store_drift(c+3u,top.zw);
            store_drift(c+6u,bottom.xy);store_drift(c+9u,bottom.zw);
        }
        return;
    }
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
        if (params.controls.y & 1u)!=0u {
            let extra=mul(ds(average),ds(params.flare[c]));
            measured_offsets[27u+c]=bitcast<i32>(extra.x);measured_offsets[30u+c]=bitcast<i32>(extra.y);
            for(var y=0u;y<params.source.y;y++) {
                let at=40u+y*9u;
                let slope=trunc_pair(mul(div_pair(interpolated_drift(c,y),ds(f32(params.source.x))),ds(16384.)));
                measured_offsets[at+c]=trunc_pair(mul(interpolated_drift(c+3u,y),ds(16384.)))-i32(params.source.x/2u)*slope;
                measured_offsets[at+3u+c]=slope;
                measured_offsets[at+6u+c]=trunc_pair(mul(extra,ds(16384.)));
            }

        }
    }
}
@compute @workgroup_size(64)
fn correct(@builtin(global_invocation_id) id: vec3<u32>){
    let y=id.x;if y>=params.source.y{return;}
    let width=params.source.x;
    let spacing=f32(width)/f32(params.source.z-1u);
    var segment=0u;var boundary=ds(spacing);
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
        if f32(x)>boundary.x || (f32(x)==boundary.x && boundary.y<=0.) {
            segment++;boundary=add(boundary,ds(spacing));
            for(var c=0u;c<3u;c++){
                let a=knots[row+segment*3u+c];let b=knots[row+(segment+1u)*3u+c];
                current[c]=i32(a*factors[c]);increment[c]=i32(((b-a)/spacing)*factors[c]);
            }
        }
        let base=(y*width+x)*3u;
        var values=vec3<i32>(0);
        for(var c=0u;c<3u;c++){
            let s=i32(input_pixels[base+c]);let p=select(previous[c],s,x==0u);let delta=s-p;
            var z=s*16384+delta*params.linear[c]+i32((bitcast<u32>(delta*delta)>>14u))*params.quadratic[c];
            if params.controls.x>1u && x%params.controls.x==0u {z+=p*params.segment_scale[c]+params.segment_bias[c];}
            var offset=offsets[c];
            if (params.controls.y & 1u)!=0u {
                let at=40u+y*9u;offset=offsets[at+c]+i32(x)*offsets[at+3u+c]+offsets[at+6u+c];
            }
            values[c]=(z-offset)>>14u;
            previous[c]=s;
        }
        if (params.controls.y & 8u)!=0u {
            let r=values.x;let g=values.y;let b=values.z;
            let squares=bitcast<vec3<u32>>(values*values)>>vec3(14u);
            let terms=array<i32,9>(i32(squares.x),i32(squares.y),i32(squares.z),
                (r*g)>>14u,(b*r)>>14u,(b*g)>>14u,
                (i32(squares.x)*r)>>14u,(i32(squares.y)*g)>>14u,(i32(squares.z)*b)>>14u);
            var corrected=values;
            for(var c=0u;c<3u;c++) {
                var sum=0;
                for(var t=0u;t<9u;t++) {sum+=terms[t]*params.polynomial[c*3u+t/3u][t%3u];}
                corrected[c]+=sum>>14u;
            }
            values=corrected;
        }
        for(var c=0u;c<3u;c++) {
            let gained=(values[c]*(current[c]>>8u))>>14u;
            // Reference headroom limit precedes signed-16 wrapping, including negatives.
            output_pixels[base+c]=f32(signed16(min(32000,gained)));
            current[c]+=increment[c];
        }
    }
}
