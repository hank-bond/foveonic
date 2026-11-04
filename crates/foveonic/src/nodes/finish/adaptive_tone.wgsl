// Regional visibility fit using the shared coordinated HDR response.
struct Context {
    shape: vec4<u32>,
    bounds: vec4<u32>,
    luma: vec4<f32>,
    values: vec4<f32>, // nominal white, HDR pivot, profile strength, Exposure gain
    analysis: vec4<f32>, // reserved, reserved, refinement radius, regional phase
    limits: vec4<f32>, // applied caps, independent of scene fitting
    natural_limits: vec4<f32>, // Natural caps for feasible shadow protection
    toe: vec4<f32>, // near-black input EV including Exposure, toe slope, working/native scale, reserved
};
@group(0) @binding(0) var<storage, read> source_rgb: array<f32>;
@group(0) @binding(1) var<storage, read> base_mask: array<f32>;
@group(0) @binding(2) var<storage, read> tone_curve: array<f32>;
@group(0) @binding(3) var<uniform> context: Context;
@group(0) @binding(4) var<storage, read_write> counts: array<atomic<u32>>;
@group(0) @binding(5) var<storage, read_write> candidates: array<vec4<f32>>;
@group(0) @binding(6) var<storage, read_write> chosen: array<f32>;
@group(0) @binding(7) var<storage, read_write> gains_out: array<f32>;
@group(0) @binding(10) var<storage, read> reference: array<f32>;
@group(0) @binding(11) var<storage, read> initial: array<f32>;

const BIN_COUNT: u32 = 4096u;
const CANDIDATES: u32 = 96u;
fn pixel(p: vec2<u32>) -> vec3<f32> {
    let i = (p.y * context.shape.x + p.x) * 3u;
    return vec3<f32>(source_rgb[i], source_rgb[i+1u], source_rgb[i+2u]);
}
fn ev_at(p: vec2<i32>) -> f32 {
    let q = vec2<u32>(clamp(p, vec2<i32>(context.bounds.xy), vec2<i32>(context.bounds.zw)-1));
    return log2(max(dot(pixel(q), context.luma.xyz) / context.values.x, exp2(-16.0)));
}
fn tone(ev: f32) -> f32 {
    let position = clamp(exp2(ev) * context.values.x * context.values.w, 0.0, f32(arrayLength(&tone_curve)-1u));
    let lo = u32(position);
    let value = mix(tone_curve[lo], tone_curve[min(lo+1u, arrayLength(&tone_curve)-1u)], fract(position));
    return select(12.92 * value, 1.055 * pow(max(value,0.0), 1.0/2.4)-0.055, value > 0.0031308);
}
@compute @workgroup_size(8,8)
fn histogram(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= context.shape.zw) { return; }
    let point = context.bounds.xy + min(vec2<u32>((vec2<f32>(id.xy)+0.5) * vec2<f32>(context.bounds.zw-context.bounds.xy) / vec2<f32>(context.shape.zw)), context.bounds.zw-context.bounds.xy-1u);
    let rgb = pixel(point);
    let y = dot(rgb, context.luma.xyz) / context.values.x;
    if y <= exp2(-14.0) { return; }
    let light = log2(y);
    let base = base_mask[point.y*context.shape.x+point.x] * 8.0 - 8.0;
    let residual = light-base;
    let peak = max(max(rgb.x,rgb.y),rgb.z) / context.values.x;
    let peak_ratio = max(0.0, log2(max(peak/y,1.0)));
    let b = u32(clamp(floor((base+12.0)*4.0),0.0,63.0));
    let d = u32(clamp(floor((residual+1.0)*4.0),0.0,7.0));
    let c = u32(clamp(round(peak_ratio*2.0),0.0,7.0));
    let slot = (b*8u+d)*8u+c;
    let p = vec2<i32>(point);
    let g2 = vec2<f32>(ev_at(p+vec2<i32>(round(vec2<f32>(2.,0.)*context.toe.z)))-ev_at(p-vec2<i32>(round(vec2<f32>(2.,0.)*context.toe.z))), ev_at(p+vec2<i32>(round(vec2<f32>(0.,2.)*context.toe.z)))-ev_at(p-vec2<i32>(round(vec2<f32>(0.,2.)*context.toe.z))));
    let g8 = vec2<f32>(ev_at(p+vec2<i32>(round(vec2<f32>(8.,0.)*context.toe.z)))-ev_at(p-vec2<i32>(round(vec2<f32>(8.,0.)*context.toe.z))), ev_at(p+vec2<i32>(round(vec2<f32>(0.,8.)*context.toe.z)))-ev_at(p-vec2<i32>(round(vec2<f32>(0.,8.)*context.toe.z))));
    // A cross-scale structure proxy, not a calibrated noise or HVS model.
    let structure = clamp(sqrt(max(dot(g2,g8),0.0))/0.3,0.0,1.0);
    atomicAdd(&counts[slot],1u);
    atomicAdd(&counts[BIN_COUNT+slot],u32(round(structure*1024.0)));
}
fn reference_amounts() -> vec4<f32> {
    return vec4<f32>(0.0,reference[0],reference[1],reference[2]);
}
fn trial_count() -> u32 { return select(CANDIDATES,29u,context.analysis.z>0.0); }
fn adjustment(id: u32) -> vec4<f32> {
    if context.analysis.z>0.0 {
        if id==27u { return reference_amounts(); }
        if id==28u { return vec4<f32>(0.0); }
        let offset=vec3<f32>(f32(id%3u)-1.0,f32((id/3u)%3u)-1.0,f32(id/9u)-1.0);
        let center=vec3<f32>(chosen[0],-chosen[1],-chosen[2]);
        let point=clamp(center+offset*context.analysis.z,vec3<f32>(0.0),vec3<f32>(1.0));
        return vec4<f32>(0.0,point.x,-point.y,-point.z);
    }
    // Search positions stay fixed as the user changes a profile cap.
    return vec4<f32>(0.0,f32(id/24u)/3.0,-f32((id/6u)%4u)/3.0,-f32(id%6u)/5.0);
}
fn capped(amounts:vec4<f32>,caps:vec4<f32>)->vec4<f32> {
    return vec4<f32>(0.0,min(amounts.y,caps.x),max(amounts.z,-caps.y),max(amounts.w,-caps.z));
}
fn displayed(light: f32, base: f32, amounts: vec4<f32>) -> f32 {
    return tone(light + log2(max(hdr_gain(light-context.values.y,base-context.values.y,amounts),1e-8)));
}
fn global_score(amounts: vec4<f32>, strength: f32) -> f32 {
    let floor_target = mix(0.055,0.08,strength);
    let ceiling = mix(0.96,0.92,strength);
    var score = 0.0;
    var total = 0.0;
    for(var slot=0u;slot<BIN_COUNT;slot++) {
        let n = f32(atomicLoad(&counts[slot]));
        if n == 0.0 { continue; }
        let structure = f32(atomicLoad(&counts[BIN_COUNT+slot]))/(n*1024.0);
        let base = -12.0+(f32(slot/64u)+0.5)/4.0;
        let residual = -1.0+(f32((slot/8u)%8u)+0.5)/4.0;
        let peak_offset = f32(slot%8u)*0.5;
        let light = base+residual;
        let gain_ev = log2(max(hdr_gain(light-context.values.y,base-context.values.y,amounts),1e-8));
        let before = tone(light);
        let after = tone(light+gain_ev);
        let peak_after = tone(light+peak_offset+gain_ev);
        let dark_loss = pow(max(0.0,(floor_target-after)/floor_target),2.0)*structure;
        let clip_loss = pow(max(0.0,(peak_after-ceiling)/(1.0-ceiling)),2.0)*(0.15+structure);
        let delta = 0.04+0.12*structure;
        let contrast_before = max(0.0,tone(light+delta)-tone(light-delta));
        let contrast_after = max(0.0,displayed(light+delta,base,amounts)-displayed(light-delta,base,amounts));
        let threshold = mix(0.006,0.01,strength);
        let visible_loss = pow(max(0.0,1.0-contrast_after/threshold),2.0)*structure;
        let contrast_loss = pow(max(0.0,1.0-contrast_after/max(contrast_before,threshold)),2.0)*structure;
        let useful_middle = smoothstep(0.08,0.2,before)*(1.0-smoothstep(0.7,0.95,before));
        let middle_loss = gain_ev*gain_ev*useful_middle*mix(0.25,0.10,strength);
        let shadow_risk = max(gain_ev,0.0)*max(gain_ev,0.0)*(1.0-structure)*0.03;
        score += n*(4.0*dark_loss+3.0*clip_loss+visible_loss+0.5*contrast_loss+middle_loss+shadow_risk);
        total += n;
    }
    score = score/max(total,1.0)+0.003*dot(amounts,amounts);
    return score;
}
@compute @workgroup_size(4)
fn fit(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= CANDIDATES { return; }
    let amounts = adjustment(id.x);
    candidates[id.x] = vec4<f32>(global_score(amounts,context.values.z),amounts.yzw);
}
@compute @workgroup_size(1)
fn select_fit() {
    let n=trial_count();
    var best=candidates[0].x;
    for(var i=1u;i<n;i++) { best=min(best,candidates[i].x); }
    var sum=vec3<f32>(0.0); var weight=0.0;
    // Blend nearly tied solutions to avoid whole-correction jumps.
    for(var i=0u;i<n;i++) {
        let w=exp(clamp((best-candidates[i].x)/0.006,-80.0,0.0));
        sum+=w*candidates[i].yzw; weight+=w;
    }
    var amounts=vec4<f32>(0.0,sum/max(weight,1e-30));
    let neutral_score=candidates[select(0u,28u,context.analysis.z>0.0)].x;
    // No measured improvement should produce no correction, even for a flat photo.
    let confidence_span=select(0.01,0.0001,context.analysis.w>0.5);
    amounts*=smoothstep(0.0,confidence_span,max(0.0,neutral_score-best));
    chosen[0]=amounts.y; chosen[1]=amounts.z; chosen[2]=amounts.w;
    chosen[3]=best; chosen[4]=neutral_score;
}
@compute @workgroup_size(4)
fn refine(@builtin(global_invocation_id) id:vec3<u32>) {
    if id.x>=trial_count() { return; }
    let amounts=adjustment(id.x);
    var score=global_score(amounts,context.values.z);
    if context.analysis.w>0.5 { score+=regional_score(amounts,context.values.z); }
    candidates[id.x]=vec4<f32>(score,amounts.yzw);
}
fn guide_ev(point:vec2<i32>)->f32 {
    let p=vec2<u32>(clamp(point,vec2<i32>(context.bounds.xy),vec2<i32>(context.bounds.zw)-1));
    return base_mask[p.y*context.shape.x+p.x]*8.0-8.0;
}
fn local_structure(point:vec2<u32>)->f32 {
    let p=vec2<i32>(point);
    let fine=vec2<f32>(ev_at(p+vec2<i32>(round(vec2<f32>(2.,0.)*context.toe.z)))-ev_at(p-vec2<i32>(round(vec2<f32>(2.,0.)*context.toe.z))),ev_at(p+vec2<i32>(round(vec2<f32>(0.,2.)*context.toe.z)))-ev_at(p-vec2<i32>(round(vec2<f32>(0.,2.)*context.toe.z))));
    let broad=vec2<f32>(guide_ev(p+vec2<i32>(round(vec2<f32>(8.,0.)*context.toe.z)))-guide_ev(p-vec2<i32>(round(vec2<f32>(8.,0.)*context.toe.z))),guide_ev(p+vec2<i32>(round(vec2<f32>(0.,8.)*context.toe.z)))-guide_ev(p-vec2<i32>(round(vec2<f32>(0.,8.)*context.toe.z))));
    // Require a native gradient that agrees with the existing filtered brightness guide.
    let coherence=clamp(sqrt(max(dot(fine,broad),0.0))/0.3,0.0,1.0);
    return smoothstep(0.05,0.25,coherence);
}
fn rescue_black(point:vec2<u32>,light:f32,base:f32,amounts:vec4<f32>,gain:f32)->f32 {
    if context.limits.w<=0.0 || context.toe.y<=0.0 || light<=-14.0 || any(point<context.bounds.xy) || any(point>=context.bounds.zw) { return gain; }
    let distance=min(max(0.0,context.toe.x-light-log2(max(gain,1e-8))),32.0);
    if distance<=0.0 { return gain; }
    let confidence=local_structure(point);
    if confidence<=0.0 { return gain; }
    let model=hdr_gain(light-context.values.y,base-context.values.y,amounts);
    let full=hdr_gain(light-context.values.y,base-context.values.y,vec4<f32>(1.0,amounts.yzw));
    let available=max(0.0,full/max(model,1e-8)-1.0);
    if available<=0.0 { return gain; }
    // A soft toe retains separation between dark tones and reaches exact black continuously.
    let desired=exp2(context.toe.y*distance*distance/(1.0+distance))-1.0;
    let amount=min(context.limits.w,desired/available)*confidence;
    return gain*(1.0+amount*available);
}
@compute @workgroup_size(8,8)
fn adaptive_gain(@builtin(global_invocation_id) id: vec3<u32>) {
    let half = (context.shape.xy+1u)/2u;
    if any(id.xy >= half) { return; }
    let point = min(id.xy*2u,context.shape.xy-1u);
    var brightness = 0.0;
    for(var dy=0u;dy<2u;dy++) { for(var dx=0u;dx<2u;dx++) {
        brightness += dot(pixel(min(point+vec2<u32>(dx,dy),context.shape.xy-1u)),context.luma.xyz);
    } }
    let light = log2(max(brightness/(4.0*context.values.x),exp2(-16.0)));
    let base = base_mask[point.y*context.shape.x+point.x]*8.0-8.0;
    let amounts=capped(vec4<f32>(0.0,chosen[0],chosen[1],chosen[2]),context.limits);
    var gain=hdr_gain(light-context.values.y,base-context.values.y,amounts);
    let transition=smoothstep(0.20,0.35,tone(light));
    if transition<1.0 {
        let caps=select(context.limits,min(context.limits,context.natural_limits),context.values.z>0.5);
        let anchor=capped(reference_amounts(),caps);
        var floor_gain=hdr_gain(light-context.values.y,base-context.values.y,anchor);
        if context.values.z>0.5 {
            let global_amounts=capped(vec4<f32>(0.0,initial[0],initial[1],initial[2]),caps);
            let global_gain=hdr_gain(light-context.values.y,base-context.values.y,global_amounts);
            floor_gain=max(floor_gain,mix(global_gain,floor_gain,transition));
        }
        gain=max(gain,mix(floor_gain,gain,transition));
    }
    gains_out[id.y*half.x+id.x]=rescue_black(point,light,base,amounts,gain);
}
