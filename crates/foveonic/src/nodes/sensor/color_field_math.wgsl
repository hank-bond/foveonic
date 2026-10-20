// N2 equations adapted from the reference implementation at 2b0dd443.
// Copyright 2026 Sang Lee and contributors. Apache-2.0. See color_field/NOTICE.
const NONE:u32=0xffffffffu;
const INF:f32=3.402823466e38;
struct Params {
    size:vec4<u32>, // full width, height, count, repair-site count
    grid:vec4<u32>, // cols, rows, count, step
    crop:vec4<u32>,
    gain:vec4<f32>,
    white:vec4<f32>,
    neutral:vec4<f32>, // native neutral xyz, consistency cutoff w (0 keeps original behavior)
    luminance:vec4<f32>,
    metric:vec4<f32>, // symmetric a,b,c and unused
    tuning:vec4<f32>, // fixed ratio scale, boundary EV, reserved, neutral multiplier
    options:vec4<u32>, // brightness solver, reserved, repaired provenance, reserved
};
@group(0) @binding(0) var<uniform> params:Params;
struct Tone { amplitude:f32, strength:f32, available:u32 };
fn smooth_onset(v:f32)->f32 {let t=clamp(v,0.0,1.0);return t*t*(3.0-2.0*t);}
fn finite(v:f32)->bool{return abs(v)<=3.402823466e38;}
fn finite3(v:vec3<f32>)->bool{return all(abs(v)<=vec3(3.402823466e38));}
fn reliability(v:f32)->u32 {
    if !finite(v) || v>=0.99{return 0u;}
    // Rust's reference rounds positive half ties upward; WGSL round uses even ties.
    return u32(clamp(floor(smooth_onset((0.99-v)/0.04)*255.0+0.5),1.0,255.0));
}
fn unpack_mask(v:u32)->vec3<f32>{return vec3(f32(v&255u),f32((v>>8u)&255u),f32((v>>16u)&255u))/255.0;}
fn recover(m:vec3<f32>,q:vec3<f32>)->Tone {
    let n=params.neutral.xyz;let y=params.luminance.xyz;let ny=dot(n,y);
    var t=Tone(0.0,0.0,0u);
    if all(q==vec3(1.0)) || !finite3(m){return t;}
    let support=dot(q,q);let my=dot(m,y);
    if support>0.0 && my>0.0 && finite(my) && ny>1e-12 {
        t.amplitude=my/ny;t.strength=smooth_onset(2.0*(1.0-min(min(q.x,q.y),q.z)));t.available=1u;
    }
    let middle=max(min(q.x,q.y),min(max(q.x,q.y),q.z));
    if middle>=128.0/255.0 || any(m<vec3(0.0)) || !finite(my) || ny<=1e-12{return t;}
    let amp=m/n;let envelope=max(max(amp.x,amp.y),amp.z);
    let strongest=max(max(q.x,q.y),q.z);let trust=smooth_onset(2.0*strongest);
    var mean=envelope;if support>0.0{mean=dot(q*q,amp)/support;}
    let guarded=trust*mean+(1.0-trust)*envelope;
    let held=max(my/ny,0.0);let amplitude=held+smooth_onset(1.0-2.0*middle)*(guarded-held);
    if amplitude>0.0 && finite(amplitude){return Tone(amplitude,1.0,1u);}return t;
}
struct Guide {pair:vec3<f32>,q:vec3<f32>,tone:f32};
fn evidence(m:vec3<f32>,mask:u32)->Guide {
    var q=unpack_mask(mask);q=q*q*q*q;
    for(var c=0u;c<3u;c++){q[c]*=smooth_onset((m[c]-0.002)/0.018);if !finite(m[c]) || (mask&0x1000000u)!=0u {q[c]=0.0;}}
    let l=log(max(m,vec3(1e-12)));
    return Guide(vec3(l.x-l.y,l.x-l.z,l.y-l.z),vec3(q.x*q.y,q.x*q.z,q.y*q.z),0.0);
}
fn agreement(a:Guide,b:Guide,tone_scale:f32,pair_scale:f32)->f32 {
    if !finite(a.tone)||!finite(b.tone)||a.tone<=-INF||b.tone<=-INF{return 0.0;}
    let t=(a.tone-b.tone)/(tone_scale*0.6931471805599453);
    let d=(a.pair-b.pair)/pair_scale;let e=min(a.q,b.q)*d*d;
    return exp(-0.5*(t*t+max(max(e.x,e.y),e.z)));
}
fn affinity(a:Guide,b:Guide)->f32{let w=agreement(a,b,0.25,params.tuning.x);return select(w,0.0,w<1e-4);}
fn metric_mul(x:vec2<f32>)->vec2<f32>{return vec2(params.metric.x*x.x+params.metric.z*x.y,params.metric.z*x.x+params.metric.y*x.y);}
fn prior_uv()->vec2<f32>{return log(params.neutral.xy/params.neutral.z);}
fn inverse(block:vec3<f32>,rhs:vec2<f32>)->vec2<f32>{let det=block.x*block.y-block.z*block.z;return vec2(block.y*rhs.x-block.z*rhs.y,block.x*rhs.y-block.z*rhs.x)/det;}
fn data_block(g:Guide)->vec3<f32>{let q=4.0*g.q;return vec3(q.x+q.y,q.x+q.z,-q.x);}
fn data_rhs(g:Guide)->vec2<f32>{let q=4.0*g.q;return vec2(q.x*g.pair.x+q.y*g.pair.y,-q.x*g.pair.x+q.z*g.pair.z);}
fn neutral_weight(g:Guide)->f32 {let u=1.0-clamp(max(max(g.q.x,g.q.y),g.q.z),0.0,1.0);return params.tuning.w*(0.0001+0.04*u*u);}
fn relative_rhs(g:Guide)->vec2<f32>{let d=g.pair-ratios(prior_uv());let q=4.0*g.q;return vec2(q.x*d.x+q.y*d.y,-q.x*d.x+q.z*d.z);}
fn fit(g:Guide,prior_value:vec2<f32>,weight:f32)->vec2<f32>{return prior_uv()+inverse(data_block(g)+weight*params.metric.xyz,relative_rhs(g)+weight*metric_mul(prior_value-prior_uv()));}
fn ratios(uv:vec2<f32>)->vec3<f32>{return vec3(uv.x-uv.y,uv.x,uv.y);}
fn retained(before:vec2<f32>,proposed:vec2<f32>,g:Guide)->f32 {
    let a=sqrt(g.q)*(ratios(before)-g.pair);let b=sqrt(g.q)*(ratios(proposed)-g.pair);
    var t=1.0;
    for(var k=0u;k<3u;k++){let bound=abs(a[k])+params.tuning.x;if g.q[k]>0.0 && abs(b[k])>bound{let edge=select(-bound,bound,b[k]>=0.0);t=min(t,(edge-a[k])/(b[k]-a[k]));}}
    return clamp(t,0.0,1.0);
}
fn direction(uv:vec2<f32>)->vec3<f32>{
    if any(abs(uv)>vec2(30.0)){return vec3(0.0);}
    let d=vec3(exp(uv),1.0);let y=dot(params.luminance.xyz,d);
    if !finite(y)||y<=1e-12{return vec3(0.0);}return d/y;
}
struct Equation {block:vec4<f32>,rhs:vec4<f32>,neighbors:vec4<u32>,weights:vec4<f32>};
struct Cell {pair:vec4<f32>,q:vec4<f32>,color:vec4<f32>,flags:vec4<u32>};
fn guide(n:Cell)->Guide{return Guide(n.pair.xyz,n.q.xyz,n.pair.w);}
