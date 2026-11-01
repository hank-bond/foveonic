// Regionally gated green/magenta desaturation before camera tone.
struct Params {
    // Image width, height, native guide radius and filter axis.
    geometry: vec4<u32>,
    // Strength, linear shadow boundary, chroma tolerance and hue-width tangent.
    selectivity: vec4<f32>,
    to_srgb: array<vec4<f32>, 3>,
    from_srgb: array<vec4<f32>, 3>,
}
@group(0) @binding(0) var<storage,read> input_pixels:array<f32>;
@group(0) @binding(1) var<uniform> p:Params;
@group(0) @binding(2) var<storage,read_write> output_pixels:array<f32>;
@group(0) @binding(3) var<storage,read> guide_pixels:array<f32>;
const CODE_WHITE:f32=4095.;
const SIGNAL_FLOOR:f32=0.00001;
const SPATIAL_SIGMA_FRACTION:f32=0.5;
const LIGHTNESS_RANGE_FRACTION:f32=0.5;
const MIN_LIGHTNESS_RANGE:f32=0.005;
const MIN_GUIDE_LIGHTNESS:f32=0.04;
const PIXEL_CHROMA_TOLERANCE:f32=2.;
const SHADOW_FADE_START:f32=0.25;
const MIN_AXIS_SUPPORT:f32=0.0001;
const MIN_CORRECTION:f32=0.000001;
const MAGENTA_AXIS:vec2<f32>=vec2(0.8660254038,-0.5);
const SRGB_LUMA:vec3<f32>=vec3(0.2126,0.7152,0.0722);
fn transform(v:vec3<f32>,rows:array<vec4<f32>,3>)->vec3<f32>{
    return vec3(dot(v,rows[0].xyz),dot(v,rows[1].xyz),dot(v,rows[2].xyz));
}
fn load_rgb(i:u32)->vec3<f32>{
    let n=i*3u;
    return vec3(input_pixels[n],input_pixels[n+1u],input_pixels[n+2u]);
}
fn load_filter_sample(i:u32)->vec4<f32>{
    let n=i*4u;
    return vec4(input_pixels[n],input_pixels[n+1u],input_pixels[n+2u],input_pixels[n+3u]);
}
fn load_guide(i:u32)->vec4<f32>{
    let n=i*4u;
    return vec4(guide_pixels[n],guide_pixels[n+1u],guide_pixels[n+2u],guide_pixels[n+3u]);
}
fn store_guide(i:u32,v:vec4<f32>){
    let n=i*4u;
    output_pixels[n]=v.x;
    output_pixels[n+1u]=v.y;
    output_pixels[n+2u]=v.z;
    output_pixels[n+3u]=v.w;
}
@compute @workgroup_size(8,8)
fn analyze(@builtin(global_invocation_id) id:vec3<u32>){
    if any(id.xy>=p.geometry.xy){
        return;
    }
    let i=id.y*p.geometry.x+id.x;
    let v=transform(load_rgb(i)/CODE_WHITE,p.to_srgb);
    store_guide(i,vec4(linear_srgb_to_oklab(v),dot(v,SRGB_LUMA)));
}
@compute @workgroup_size(8,8)
fn regional_filter(@builtin(global_invocation_id) id:vec3<u32>){
    if any(id.xy>=p.geometry.xy){
        return;
    }
    let i=id.y*p.geometry.x+id.x;
    let center=load_guide(i);
    let radius=i32(p.geometry.z);
    var sum=vec4(0.);
    var total=0.;
    for(var k=-radius;k<=radius;k++){
        var q=vec2<i32>(id.xy);
        if p.geometry.w==0u{
            q.x+=k;
        }
        else{
            q.y+=k;
        }
        q=clamp(q,vec2(0),vec2<i32>(p.geometry.xy)-1);
        let n=u32(q.y)*p.geometry.x+u32(q.x);
        let difference=(load_guide(n).x-center.x)/max(abs(center.x)*LIGHTNESS_RANGE_FRACTION,MIN_LIGHTNESS_RANGE);
        let distance=f32(k)/max(f32(radius)*SPATIAL_SIGMA_FRACTION,1.);
        let weight=exp(-0.5*(difference*difference+distance*distance));
        sum+=load_filter_sample(n)*weight;
        total+=weight;
    }
    store_guide(i,sum/total);
}
@compute @workgroup_size(8,8)
fn neutralize(@builtin(global_invocation_id) id:vec3<u32>){
    if any(id.xy>=p.geometry.xy){
        return;
    }
    let i=id.y*p.geometry.x+id.x;
    let original=load_rgb(i);
    let v=transform(original/CODE_WHITE,p.to_srgb);
    let brightness=dot(v,SRGB_LUMA);
    let regional=load_guide(i);
    var result=original;
    if brightness>SIGNAL_FLOOR && brightness<p.selectivity.y {
        let local=linear_srgb_to_oklab(v);
        let chroma=regional.yz;
        let relative=length(chroma)/max(abs(regional.x),MIN_GUIDE_LIGHTNESS);
        let pixel_relative=length(local.yz)/max(abs(local.x),MIN_GUIDE_LIGHTNESS);
        let neutrality=exp(-0.5*pow(relative/p.selectivity.z,2.))*exp(-0.5*pow(pixel_relative/(PIXEL_CHROMA_TOLERANCE*p.selectivity.z),2.));
        let along=dot(chroma,MAGENTA_AXIS);
        let across=dot(chroma,vec2(-MAGENTA_AXIS.y,MAGENTA_AXIS.x));
        let hue=exp(-0.5*pow(across/max(abs(along)*p.selectivity.w,MIN_AXIS_SUPPORT),2.));
        let shadow=1.-smoothstep(p.selectivity.y*SHADOW_FADE_START,p.selectivity.y,max(brightness,regional.w));
        let amount=p.selectivity.x*shadow*neutrality*hue;
        if amount>MIN_CORRECTION {
            var changed=oklab_to_linear_srgb(vec3(local.x,local.yz*(1.-amount)));
            let changed_y=dot(changed,SRGB_LUMA);
            if changed_y>SIGNAL_FLOOR {
                changed*=brightness/changed_y;
                result=transform(changed,p.from_srgb)*CODE_WHITE;
            }
        }
    }
    let n=i*3u;
    output_pixels[n]=result.x;
    output_pixels[n+1u]=result.y;
    output_pixels[n+2u]=result.z;
}
