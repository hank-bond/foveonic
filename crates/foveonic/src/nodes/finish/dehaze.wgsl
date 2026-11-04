// Haze-reference subtraction with a soft floor, bounded gain and chromatic restraint.
struct Basis {
    to_camera:array<vec4<f32>,3>, from_camera:array<vec4<f32>,3>,
    to_srgb:array<vec4<f32>,3>, from_srgb:array<vec4<f32>,3>, luma:vec4<f32>,
}
struct Params {
    shape:vec4<u32>, values:vec4<f32>, manual:vec4<f32>,
    basis:Basis,
}
@group(0) @binding(0) var<storage,read> src:array<f32>;
@group(0) @binding(1) var<uniform> p:Params;
@group(0) @binding(2) var<storage,read> analysis:array<vec4<f32>>;
@group(0) @binding(3) var<storage,read_write> dst:array<f32>;
const DISPLAY_LUMA:vec3<f32>=vec3(.2126,.7152,.0722);
const EPS:f32=0.00000011920928955078125;
fn transform(rgb:vec3<f32>,rows:array<vec4<f32>,3>)->vec3<f32> {
    return vec3(dot(rgb,rows[0].xyz),dot(rgb,rows[1].xyz),dot(rgb,rows[2].xyz));
}
fn saturation(v:vec3<f32>)->f32 {
    let high=max(v.x,max(v.y,v.z));
    // The chromatic gate can make q tiny while its saturation remains high.
    // A fixed denominator floor would turn vivid colors gray at this boundary.
    if high<=0. { return 0.; }
    return max(0.,high-min(v.x,min(v.y,v.z)))/high;
}
fn soft_subtract(v:vec3<f32>,a:f32)->vec3<f32> {
    let delta=v-vec3(a);
    let knee=max(vec3(a)-abs(delta),vec3(0.));
    return max(delta,vec3(0.))+knee*knee*knee/max(EPS,6.*a*a);
}
fn dehaze_pixel(strength:f32,haze:vec3<f32>,pixel:vec3<f32>,floor:f32)->vec3<f32> {
    var reference=max(haze,vec3(EPS)); var q=pixel; var residual=vec3(0.);
    if strength>0. {
        let subtract=min(2.*strength,1.)*floor;
        reference=max(soft_subtract(reference,subtract),vec3(EPS));
        let value=soft_subtract(pixel,subtract);
        let fraction=dot(reference,p.basis.luma.xyz)/max(EPS,dot(value+reference,p.basis.luma.xyz));
        q=value*fraction; residual=value*(1.-fraction);
    }
    let ratios=q/reference;
    let raw_min=min(ratios.x,min(ratios.y,ratios.z));
    var transmission=clamp(raw_min,0.,1.);
    if strength>0. {
        let norm=length(q)+EPS;
        let chroma=length(q-reference*raw_min)+EPS;
        let gate=1./(1.+exp(4.*(.5-(3.*norm-8.*chroma)/norm)));
        transmission*=gate;
        residual+=q*(1.-gate); q*=gate;
    }
    let gain=1./max(.05,1.-transmission*strength);
    var output=residual+reference+(q-reference)*gain;
    let limit=sqrt((2.*raw_min+.5)*(2.*raw_min+.5)+1.)*saturation(q);
    let sat=saturation(output);
    if limit<sat { output=mix(vec3(dot(output,p.basis.luma.xyz)),output,limit/sat); }
    return output;
}
// Use one RGB blend so the correction eases before consuming a weak channel.
// The rational shoulder makes the largest relative loss r approach r/(1+r).
// Absolute signal capacity keeps protection continuous through signed zero.
fn protect_shadow_signal(input:vec3<f32>,candidate:vec3<f32>,white:f32)->vec3<f32> {
    var loss=0.;
    for (var c=0u;c<3u;c++) {
        loss=max(loss,max(0.,input[c]-candidate[c])/max(0.000001*white,abs(input[c])));
    }
    if loss==0. { return candidate; }
    return mix(input,candidate,1./(1.+loss));
}
@compute @workgroup_size(8,8)
fn dehaze_apply(@builtin(global_invocation_id) id:vec3<u32>) {
    if any(id.xy>=p.shape.xy) { return; }
    let n=(id.y*p.shape.x+id.x)*3u;
    let original=vec3(src[n],src[n+1u],src[n+2u]);
    var output=original;
    if p.values.x!=0. && (analysis[0].w>0. || p.manual.w>0.) {
        var rgb=original/p.values.y;
        if p.shape.z!=0u { rgb=vec3(rgb.y); }
        else { rgb=transform(rgb,p.basis.to_camera); }
        var haze=select(analysis[0].xyz,p.manual.xyz,p.manual.w>0.);
        if p.shape.z!=0u { haze=vec3(dot(haze,DISPLAY_LUMA)); }
        else { haze=transform(haze,p.basis.from_srgb); }
        let white=max(.05,analysis[1].y);
        let strength=select(2.*p.values.x,p.values.x,p.values.x>0.);
        var corrected=dehaze_pixel(strength,haze,max(rgb,vec3(0.)),analysis[1].x);
        if strength>0. {
            corrected*=vec3(white)/max(vec3(EPS),dehaze_pixel(strength,haze,vec3(white),analysis[1].x));
        }
        corrected+=min(rgb,vec3(0.));
        if p.shape.z!=0u { output=vec3(corrected.y*p.values.y); }
        else { output=transform(corrected,p.basis.from_camera)*p.values.y; }
    }
    if p.values.x>0. {
        let baseline=select(original,vec3(original.y),p.shape.z!=0u);
        output=protect_shadow_signal(baseline,output,p.values.y);
    }
    dst[n]=output.x; dst[n+1u]=output.y; dst[n+2u]=output.z;
}
