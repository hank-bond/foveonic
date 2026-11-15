// Regional scoring in calibrated, color-mode RGB.
struct Region {
    all: vec4<f32>, // mean log light, mean base, coherent structure, log standard deviation
    bright: vec4<f32>, // mean light, base, peak offset, log standard deviation
    dark: vec4<f32>, // mean light, base, coherent structure, log standard deviation
    support: vec4<f32>, // bright count, dark count, valid count, reserved
}
@group(0) @binding(8) var<storage, read_write> samples: array<vec4<f32>>;
@group(0) @binding(9) var<storage, read_write> regions: array<Region>;
const REGION_SAMPLE_LIMIT: u32 = 128u;
const REGION_COUNT: u32 = 1235u; // 31² + 15² + 7² overlapping windows

@compute @workgroup_size(8,8)
fn sample_regions(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= context.shape.zw) { return; }
    let point = analysis_point(id.xy);
    let rgb = pixel(point);
    let y = dot(rgb,context.luma.xyz)/context.values.x;
    let index = id.y*context.shape.z+id.x;
    if y<=exp2(-14.0) { samples[index]=vec4<f32>(-32.0,0.0,0.0,0.0); return; }
    let p=vec2<i32>(point);
    let g2 = light_gradient(p, i32(context.toe.z));
    let g8 = light_gradient(p, i32(context.toe.w));
    let coherence=clamp(sqrt(max(dot(g2,g8),0.0))/0.3,0.0,1.0);
    let peak=max(max(rgb.x,rgb.y),rgb.z)/context.values.x;
    samples[index]=vec4<f32>(log2(y),encoded_guide(point)*8.0-8.0,max(0.0,log2(max(peak/y,1.0))),coherence);
}
fn window(index:u32)->vec3<u32> {
    if index<961u { return vec3<u32>((index%31u)*8u,(index/31u)*8u,16u); }
    let middle=index-961u;
    if middle<225u { return vec3<u32>((middle%15u)*16u,(middle/15u)*16u,32u); }
    let large=middle-225u;
    return vec3<u32>((large%7u)*32u,(large/7u)*32u,64u);
}
fn add_region(left: Region, right: Region) -> Region {
    return Region(left.all+right.all, left.bright+right.bright, left.dark+right.dark, left.support+right.support);
}
// Integer midpoint selection also covers narrow images without overflowing
// the product of a sample index and the physical window extent.
fn region_midpoint(index: u32, extent: u32, count: u32) -> u32 {
    let divisor=2u*count;
    let phase=2u*index+1u;
    return phase*(extent/divisor)+(phase*(extent%divisor))/divisor;
}
fn summarize_quality_region(index: u32) {
    let w=window(index);
    let window_low=vec2<u32>(floor(vec2<f32>(w.xy)*vec2<f32>(context.shape.zw)/256.0));
    let window_high=min(vec2<u32>(ceil(vec2<f32>(w.xy+w.z)*vec2<f32>(context.shape.zw)/256.0)),context.shape.zw);
    let window_extent=window_high-window_low;
    let sample_extent=min(window_extent,vec2<u32>(REGION_SAMPLE_LIMIT));
    // Bound each scene summary while preserving its Whole-normalized windows.
    // Small windows retain every pixel. Larger windows use cell midpoints.
    // Binary partial sums bound rounding error by the tree depth, even when
    // GPU fast-math reassociates compensated-addition expressions. Each valid
    // sample participates once. The input geometry contains fewer than 2^31
    // RGB pixels, so 31 levels cover even the largest supported population.
    var partial: array<Region,31>;
    var count=0u;
    var origin=vec3<f32>(0.0);
    var has_origin=vec3<bool>(false);
    for(var row=0u;row<sample_extent.y;row++) { for(var column=0u;column<sample_extent.x;column++) {
        let y=window_low.y+region_midpoint(row,window_extent.y,sample_extent.y);
        let x=window_low.x+region_midpoint(column,window_extent.x,sample_extent.x);
        let v=samples[y*context.shape.z+x];
        if v.x < -20.0 { continue; }
        let bright_weight=smoothstep(0.80,0.90,tone(v.x+v.z));
        let dark_weight=(1.0-smoothstep(0.15,0.25,tone(v.x)))*smoothstep(0.03,0.08,v.w);
        // Separate nearby origins protect weak variation within a bright or
        // dark population even when the complete window spans many stops.
        if !has_origin.x { origin.x=v.x; has_origin.x=true; }
        if !has_origin.y && bright_weight>0.0 { origin.y=v.x; has_origin.y=true; }
        if !has_origin.z && dark_weight>0.0 { origin.z=v.x; has_origin.z=true; }
        let light=vec3<f32>(v.x)-origin;
        var merged=Region(
            vec4<f32>(light.x,v.y,v.w,light.x*light.x),
            bright_weight*vec4<f32>(light.y,v.y,v.z,light.y*light.y),
            dark_weight*vec4<f32>(light.z,v.y,v.w,light.z*light.z),
            vec4<f32>(bright_weight,dark_weight,1.0,0.0));
        var occupied=count;
        var level=0u;
        loop {
            if (occupied & 1u)==0u { break; }
            merged=add_region(partial[level],merged);
            occupied=occupied>>1u;
            level++;
        }
        partial[level]=merged;
        count++;
    } }
    var total=Region(vec4<f32>(0.0),vec4<f32>(0.0),vec4<f32>(0.0),vec4<f32>(0.0));
    for(var level=0u;level<31u;level++) {
        if (count & (1u<<level))!=0u { total=add_region(total,partial[level]); }
    }
    var all=total.all/max(total.support.z,1.0);
    // Fractional physical populations can have useful normalized support on
    // small grids. Their means divide by their actual positive weight.
    var bright=total.bright/max(total.support.x,1e-30);
    var dark=total.dark/max(total.support.y,1e-30);
    all.w=sqrt(max(all.w-all.x*all.x,0.0));
    bright.w=sqrt(max(bright.w-bright.x*bright.x,0.0));
    dark.w=sqrt(max(dark.w-dark.x*dark.x,0.0));
    all.x+=origin.x;
    bright.x+=origin.y;
    dark.x+=origin.z;
    let density=65536.0/(f32(context.shape.z)*f32(context.shape.w));
    // Rejected samples still occupy lattice cells. Rescale by scheduled points
    // so near-black rejection does not inflate a remaining population's support.
    let represented_area=(f32(window_extent.x)*f32(window_extent.y))/(f32(sample_extent.x)*f32(sample_extent.y));
    regions[index]=Region(all,bright,dark,total.support*represented_area*density);
}
@compute @workgroup_size(64)
fn summarize_regions(@builtin(global_invocation_id) id:vec3<u32>) {
    if id.x>=REGION_COUNT { return; }
    if context.sampling.z!=0u { summarize_quality_region(id.x); return; }
    let w=window(id.x);
    var sum=vec4<f32>(0.0);
    var high=vec4<f32>(0.0);
    var low=vec4<f32>(0.0);
    var support=vec3<f32>(0.0);
    for(var y=0u;y<w.z;y++) { for(var x=0u;x<w.z;x++) {
        let v=samples[(w.y+y)*context.shape.z+w.x+x];
        if v.x < -20.0 { continue; }
        sum+=vec4<f32>(v.x,v.y,v.w,v.x*v.x);
        support.z+=1.0;
        let bright_weight=smoothstep(0.80,0.90,tone(v.x+v.z));
        let dark_weight=(1.0-smoothstep(0.15,0.25,tone(v.x)))*smoothstep(0.03,0.08,v.w);
        high+=bright_weight*vec4<f32>(v.x,v.y,v.z,v.x*v.x);
        low+=dark_weight*vec4<f32>(v.x,v.y,v.w,v.x*v.x);
        support.x+=bright_weight;
        support.y+=dark_weight;
    } }
    sum/=max(support.z,1.0);
    high/=max(support.x,1.0);
    low/=max(support.y,1.0);
    sum.w=sqrt(max(sum.w-sum.x*sum.x,0.0));
    high.w=sqrt(max(high.w-high.x*high.x,0.0));
    low.w=sqrt(max(low.w-low.x*low.x,0.0));
    regions[id.x]=Region(sum,high,low,vec4<f32>(support,0.0));
}
fn displayed_peak(light:f32,base:f32,offset:f32,amounts:vec4<f32>)->f32 {
    let gain=log2(max(hdr_gain(light-context.values.y,base-context.values.y,amounts),1e-8));
    return tone(light+offset+gain);
}
fn regional_score(amounts:vec4<f32>,strength:f32)->f32 {
    var bright_sum=0.0; var bright_max=0.0; var bright_count=0.0;
    var dark_sum=0.0; var dark_count=0.0; var guard=0.0;
    let anchor=reference_amounts();
    for(var i=0u;i<REGION_COUNT;i++) {
        let r=regions[i];
        // Fade small supports in smoothly as exposure moves samples between ranges.
        let bright_weight=smoothstep(2.0,6.0,r.support.x);
        let dark_weight=smoothstep(2.0,6.0,r.support.y);
        if bright_weight>0.0 {
            let peak=r.bright.x+r.bright.z;
            let prominence=max(0.0,tone(peak)-tone(r.all.x));
            let ceiling_target=0.98-0.10*smoothstep(0.10,0.50,prominence)-0.03*strength;
            let gain=log2(max(hdr_gain(r.bright.x-context.values.y,r.bright.y-context.values.y,amounts),1e-8));
            let rendered_peak=tone(peak+gain);
            let glare=pow(max(0.0,(rendered_peak-ceiling_target)/(1.0-ceiling_target)),2.0);
            let delta=clamp(r.bright.w,0.02,0.4);
            let contrast=max(0.0,displayed_peak(r.bright.x+delta,r.bright.y,r.bright.z,amounts)-displayed_peak(r.bright.x-delta,r.bright.y,r.bright.z,amounts));
            let desired=min(0.06,r.bright.w*0.10);
            let detail=pow(max(0.0,1.0-contrast/max(desired,0.001)),2.0)*smoothstep(0.02,0.20,r.bright.w)*r.all.z;
            let loss=glare+0.25*detail;
            bright_sum+=bright_weight*loss; bright_max=max(bright_max,bright_weight*loss); bright_count+=bright_weight;
        }
        if dark_weight>0.0 {
            let after=displayed(r.dark.x,r.dark.y,amounts);
            let delta=clamp(r.dark.w,0.02,0.4);
            let contrast=max(0.0,displayed(r.dark.x+delta,r.dark.y,amounts)-displayed(r.dark.x-delta,r.dark.y,amounts));
            let desired=mix(0.008,0.012,strength);
            dark_sum+=dark_weight*pow(max(0.0,1.0-contrast/desired),2.0)*r.dark.z;
            dark_count+=dark_weight;
            let natural=displayed(r.dark.x,r.dark.y,anchor);
            let natural_contrast=max(0.0,displayed(r.dark.x+delta,r.dark.y,anchor)-displayed(r.dark.x-delta,r.dark.y,anchor));
            guard=max(guard,dark_weight*200.0*(pow(max(0.0,natural-after-0.002),2.0)+pow(max(0.0,natural_contrast-contrast-0.002),2.0)));
        }
    }
    return guard+0.02*bright_sum/max(bright_count,1.0)+0.02*bright_max+0.015*dark_sum/max(dark_count,1.0);
}
@compute @workgroup_size(4)
fn fit_regions(@builtin(global_invocation_id) id:vec3<u32>) {
    if id.x>=CANDIDATES { return; }
    let amounts=adjustment(id.x);
    let score=global_score(amounts,context.values.z)+regional_score(amounts,context.values.z);
    candidates[id.x]=vec4<f32>(score,amounts.yzw);
}
