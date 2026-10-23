// Ordered color-cell inference follows the independently checked reference equations.
const CELL_SIDE=16u;
const CELL_MEAN_WEIGHT=1./256.;
const VALID_CELL_LOW=0.5;
const VALID_CELL_HIGH=1.;
const COLOR_COMPATIBILITY=0.015;
const MIN_FIT_CELLS=101u;
const MIN_STANDARD_DEVIATION=0.001;
const RECOVERY_START=0.95;
const RECOVERY_SPAN=0.050000012;
const BRIGHTNESS_OFFSET=5.;
const DEFAULT_SLOPES=vec3(0.4909, 2.4285, 0.7424);
const DEFAULT_INTERCEPTS=vec3(0.4948, -1.4353, 0.2762);
// Only a well-conditioned fitted G/B slope replaces the fallback regression.
const GB_SLOPE_MIN=0.3712;
const GB_SLOPE_MAX=2.9696;
const ORIGINAL_CELL=1.;
const PROPAGATED_CELL=2.;
@group(0) @binding(4) var<storage, read_write> grid: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read_write> recovery_mask: array<u32>;
@group(0) @binding(7) var<storage, read_write> coefficients: array<f32>;



fn plus(a: f32, b: f32)->f32 {
    return fma(1., a, b);
}
fn times(a: f32, b: f32)->f32 {
    return fma(a, b, 0.);
}
fn divide(a: f32, b: f32)->f32 {
    let q=a/b;
    if a==0.||b==0.||q!=q||abs(q)>3.402823e38 {
        return q;
    }
    return plus(q, fma(-q, b, a)/b);
}
fn ratios(value: vec3<f32>)->vec3<f32> {
    return vec3(divide(value.x, max(1., value.y)), divide(value.x, max(1., value.z)), divide(value.y, max(1., value.z)));
}
fn median(v: vec3<f32>)->f32 {
    return max(min(v.x, v.y), min(max(v.x, v.y), v.z));
}
@compute @workgroup_size(8, 8)
fn dynamic_range_prepare(@builtin(global_invocation_id) id: vec3<u32>) {
    let x=id.x;
    let y=id.y;
    if x>=p.shape.x||y>=p.shape.y {
        return;
    }
    let n=y*p.shape.x+x;
    let i=n*3u;
    let source=vec3(pixels[i], pixels[i+1u], pixels[i+2u]);
    let limits=p.ceilings.xyz*p.gains.xyz;
    var value=min(source, p.ceilings.xyz)*p.gains.xyz;
    let flags=vec3<u32>(value>=limits);
    let count=flags.x+flags.y+flags.z;
    work[p.shape.z*GREEN_MARKER+n]=f32(source.y>=p.thresholds.y);
    work[p.shape.z*CLIPPED_MARKER+n]=f32(count>=1u);
    if count>=2u {
        value=vec3(median(limits));
    }
    store_rgb(0u, n, value);
    var total=0.;
    if x>=2u&&x<p.shape.x-2u&&y>=2u&&y<p.shape.y-1u {
        for(var dy=-1;dy<=1;dy++) {
            for(var dx=-1;dx<=1;dx++) {
                let weight=select(1., 0.25, dx!=0&&dy!=0);
                let index=(u32(i32(y)+dy)*p.shape.x+u32(i32(x)+dx))*3u;
                total=fma(weight, f32(pixels[index+1u]>=p.thresholds.y), total);
            }
        }
    }
    green_mask[n]=u32(ceil(min(total*255., 255.)));
}
@compute @workgroup_size(8, 8)
fn dynamic_range_cells(@builtin(global_invocation_id) id: vec3<u32>) {
    let width=(p.shape.x+CELL_SIDE-1u)/CELL_SIDE;
    let height=(p.shape.y+CELL_SIDE-1u)/CELL_SIDE;
    if id.x>=width||id.y>=height {
        return;
    }
    let index=id.y*width+id.x;
    var sum=vec3(0.);
    var maximum=0.;
    let limits=p.ceilings.xyz*p.gains.xyz;
    // Only complete cells inside the active rectangle supply color evidence.
    // Partial edge cells remain invalid but still give every pixel a safe grid slot.
    if id.x>=p.rectangle.x/CELL_SIDE&&id.x<p.rectangle.z/CELL_SIDE&&id.y>=p.rectangle.y/CELL_SIDE&&id.y<p.rectangle.w/CELL_SIDE {
        for(var y=0u;y<CELL_SIDE;y++) {
            for(var x=0u;x<CELL_SIDE;x++) {
                let n=(id.y*CELL_SIDE+y)*p.shape.x+id.x*CELL_SIDE+x;
                let value=load_rgb(0u, n);
                sum=vec3(fma(value.x, CELL_MEAN_WEIGHT, sum.x), fma(value.y, CELL_MEAN_WEIGHT, sum.y), fma(value.z, CELL_MEAN_WEIGHT, sum.z));
                maximum=max(maximum, max(divide(value.x, limits.x), max(divide(value.y, limits.y), divide(value.z, limits.z))));
            }
        }
    }
    grid[index]=vec4(sum, f32(maximum>VALID_CELL_LOW&&maximum<VALID_CELL_HIGH));
}
fn relative(a: f32, b: f32)->f32 {
    return abs(divide(plus(a, -b), plus(a, b)));
}
fn propagate(destination_index: u32, a: u32, b: u32) {
    if grid[destination_index].w!=0.||grid[a].w==0.||grid[b].w==0. {
        return;
    }
    let av=grid[a].xyz;
    let bv=grid[b].xyz;
    let tv=grid[destination_index].xyz;
    let ar=vec2(divide(av.x, av.y), divide(av.y, av.z));
    let br=vec2(divide(bv.x, bv.y), divide(bv.y, bv.z));
    let tr=vec2(divide(tv.x, tv.y), divide(tv.y, tv.z));
    if !(relative(ar.x, br.x)<COLOR_COMPATIBILITY&&relative(ar.y, br.y)<COLOR_COMPATIBILITY) {
        return;
    }
    if !((relative(ar.x, tr.x)<COLOR_COMPATIBILITY&&relative(br.x, tr.x)<COLOR_COMPATIBILITY)||(relative(ar.y, tr.y)<COLOR_COMPATIBILITY&&relative(br.y, tr.y)<COLOR_COMPATIBILITY)) {
        return;
    }
    let am=divide(plus(plus(av.x, av.y), av.z), 3.);
    let bm=divide(plus(plus(bv.x, bv.y), bv.z), 3.);
    grid[destination_index]=vec4(select(bv, av, am>=bm), PROPAGATED_CELL);
}
@compute @workgroup_size(1)
fn dynamic_range_fit() {
    let width=(p.shape.x+CELL_SIDE-1u)/CELL_SIDE;
    let height=(p.shape.y+CELL_SIDE-1u)/CELL_SIDE;
    let cells=width*height;
    for(var y=1u;y<height;y++) {
        for(var x=1u;x<width;x++) {
            let i=y*width+x;
            propagate(i, i-1u, i-width);
        }
    }
    for(var y=i32(height)-2;y>0;y--) {
        for(var x=i32(width)-2;x>0;x--) {
            let i=u32(y)*width+u32(x);
            propagate(i, i+1u, i+width);
        }
    }
    var count=0u;
    var sum=vec3(0.);
    for(var i=0u;i<cells;i++) {
        if grid[i].w==ORIGINAL_CELL {
            let v=ratios(grid[i].xyz);
            sum=vec3(plus(sum.x, v.x), plus(sum.y, v.y), plus(sum.z, v.z));
            count++;
        }
    }
    var slopes=DEFAULT_SLOPES;
    var intercepts=DEFAULT_INTERCEPTS;
    if count>=MIN_FIT_CELLS {
        let denominator=f32(count+1u);
        let means=vec3(divide(plus(sum.x, 1.), denominator), divide(plus(sum.y, 1.), denominator), divide(plus(sum.z, 1.), denominator));
        var variance=vec3(0.);
        var covariance=vec3(0.);
        for(var i=0u;i<cells;i++) {
            if grid[i].w==ORIGINAL_CELL {
                let delta=means-ratios(grid[i].xyz);
                variance=vec3(fma(delta.x, delta.x, variance.x), fma(delta.y, delta.y, variance.y), fma(delta.z, delta.z, variance.z));
                covariance=vec3(fma(delta.x, delta.y, covariance.x), fma(delta.y, delta.z, covariance.y), fma(delta.z, delta.x, covariance.z));
            }
        }
        let delta=means-vec3(1.);
        let sd=sqrt(vec3(divide(fma(delta.x, delta.x, variance.x), denominator), divide(fma(delta.y, delta.y, variance.y), denominator), divide(fma(delta.z, delta.z, variance.z), denominator)));
        let bounded=max(sd, vec3(MIN_STANDARD_DEVIATION));
        covariance=vec3(divide(fma(delta.x, delta.y, covariance.x), denominator), divide(fma(delta.y, delta.z, covariance.y), denominator), divide(fma(delta.z, delta.x, covariance.z), denominator));
        let correlation=vec3(divide(divide(covariance.x, bounded.y), bounded.x), divide(divide(covariance.y, bounded.z), bounded.y), divide(divide(covariance.z, bounded.x), bounded.z));
        slopes=vec3(divide(times(sd.x, correlation.x), bounded.y), divide(times(sd.y, correlation.y), bounded.z), divide(times(sd.z, correlation.z), bounded.x));
        intercepts=vec3(fma(-slopes.x, means.y, means.x), fma(-slopes.y, means.z, means.y), fma(-slopes.z, means.x, means.z));
    }
    coefficients[0]=slopes.x;
    coefficients[1]=intercepts.x;
    coefficients[2]=slopes.y;
    coefficients[3]=intercepts.y;
    coefficients[4]=slopes.z;
    coefficients[5]=intercepts.z;
    coefficients[6]=f32(count);
}
@compute @workgroup_size(8, 8)
fn dynamic_range_apply(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x>=p.shape.x||id.y>=p.shape.y {
        return;
    }
    let n=id.y*p.shape.x+id.x;
    let source=load_rgb(0u, n);
    let limits=p.ceilings.xyz*p.gains.xyz;
    var result=source;
    var mask=0u;
    if p.shape.w==CHANNEL_EXTENSION {
        let normalized=vec3(divide(source.x, limits.x), divide(source.y, limits.y), divide(source.z, limits.z));
        let maximum=max(normalized.x, max(normalized.y, normalized.z));
        let cell=grid[(id.y/CELL_SIDE)*((p.shape.x+CELL_SIDE-1u)/CELL_SIDE)+id.x/CELL_SIDE];
        let cell_ratios=vec3(divide(cell.x, cell.z), divide(cell.x, cell.y), divide(cell.y, cell.z));
        var gb_slope=coefficients[4];
        var gb_intercept=coefficients[5];
        if !(gb_slope>=GB_SLOPE_MIN&&gb_slope<=GB_SLOPE_MAX) {
            gb_slope=DEFAULT_SLOPES.z;
            gb_intercept=DEFAULT_INTERCEPTS.z;
        }
        for(var channel=0u;channel<3u;channel++) {
            let a=(channel+1u)%3u;
            let b=(channel+2u)%3u;
            if !(normalized[channel]>RECOVERY_START&&normalized[channel]==maximum&&normalized[a]<1.&&normalized[b]<1.) {
                continue;
            }
            let amount=clamp(divide(plus(normalized[channel], -RECOVERY_START), RECOVERY_SPAN), 0., 1.);
            var reconstructed=0.;
            if channel==0u {
                reconstructed=times(source.z, fma(divide(source.y, max(1., source.z)), coefficients[2], coefficients[3]));
            }
            else if channel==1u {
                reconstructed=divide(source.x, max(MIN_STANDARD_DEVIATION, fma(divide(source.x, max(1., source.z)), coefficients[0], coefficients[1])));
            }
            else {
                reconstructed=divide(source.y, max(MIN_STANDARD_DEVIATION, fma(divide(source.x, max(1., source.y)), gb_slope, gb_intercept)));
            }
            var ratio=divide(reconstructed, source.z);
            if channel==1u {
                ratio=divide(source.x, reconstructed);
            }
            else if channel==2u {
                ratio=divide(source.y, reconstructed);
            }
            if cell.w!=0.&&times(plus(cell_ratios.z, -1.), plus(ratio, -1.))<0. {
                if channel==0u {
                    reconstructed=times(cell_ratios.x, source.z);
                }
                else if channel==1u {
                    reconstructed=divide(source.x, cell_ratios.y);
                }
                else {
                    reconstructed=divide(source.y, cell_ratios.z);
                }
            }
            var candidate=result;
            candidate[channel]=fma(reconstructed, amount, times(plus(1., -amount), source[channel]));
            let old_sum=plus(plus(plus(source.x, source.y), source.z), BRIGHTNESS_OFFSET);
            let new_sum=plus(plus(plus(candidate.x, candidate.y), candidate.z), BRIGHTNESS_OFFSET);
            result=vec3(divide(times(candidate.x, old_sum), new_sum), divide(times(candidate.y, old_sum), new_sum), divide(times(candidate.z, old_sum), new_sum));
            mask=mask|(1u<<channel);
        }
        let clipped=vec3<u32>(normalized>=vec3(1.));
        if clipped.x+clipped.y+clipped.z==2u {
            result=vec3(median(limits));
        }
    }
    store_rgb(0u, n, result);
    recovery_mask[n]=mask;
    let converted=color(result, 0u);
    let minimum=min(converted.x, min(converted.y, converted.z));
    work[p.shape.z*LOW_CAMERA_MARKER+n]=f32(f32(i32(minimum))<p.thresholds.z);
}
