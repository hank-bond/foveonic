// Shared guide selects colors; edits preserve the source's reconstruction and code units.
struct Params {
    shape: vec4<u32>, values: vec4<f32>, bands: array<vec4<f32>, 8>,
    master: vec4<f32>, luma: vec4<f32>, guide_matrix: array<vec4<f32>, 3>,
    to_srgb: array<vec4<f32>, 3>, from_srgb: array<vec4<f32>, 3>,
    protection: vec4<f32>,
}
@group(0) @binding(0) var<storage, read> src: array<f32>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage, read_write> dst: array<f32>;
@group(0) @binding(3) var<storage, read> aux: array<f32>;
const MAX_TRANSFER_STOPS: f32 = 6.;
const RANGE_SIGMA_STOPS: f32 = 0.5;
const SPATIAL_SIGMA_FRACTION: f32 = 0.5;
// Normalized Hue ±1 rotates chroma by ±30 degrees.
const MAX_HUE_RADIANS: f32 = 0.5235987756;
fn valid(id: vec3<u32>) -> bool { return all(id.xy < p.shape.xy); }
fn rgb(i: u32) -> vec3<f32> { let n=i*3u; return vec3(src[n],src[n+1u],src[n+2u]); }
fn guide_y(i: u32) -> f32 {
    let n=i*3u; return dot(vec3(aux[n],aux[n+1u],aux[n+2u]),p.luma.xyz);
}
fn transform(v: vec3<f32>, rows: array<vec4<f32>,3>) -> vec3<f32> {
    return vec3(dot(v,rows[0].xyz),dot(v,rows[1].xyz),dot(v,rows[2].xyz));
}
// Lightness-only and monochrome edits keep the original scalar scratch footprint.
fn store_adjustment(i: u32, v: vec3<f32>) {
    if p.values.y==0. { dst[i]=v.z; }
    else { dst[i*4u]=v.x; dst[i*4u+1u]=v.y; dst[i*4u+2u]=v.z; dst[i*4u+3u]=0.; }
}
fn read_adjustment(i: u32) -> vec3<f32> {
    if p.values.y==0. { return vec3(0.,0.,src[i]); }
    return vec3(src[i*4u],src[i*4u+1u],src[i*4u+2u]);
}
fn auxiliary_adjustment(i: u32) -> vec3<f32> {
    if p.values.y==0. { return vec3(0.,0.,aux[i]); }
    return vec3(aux[i*4u],aux[i*4u+1u],aux[i*4u+2u]);
}
@compute @workgroup_size(8,8)
fn color_editor_filter(@builtin(global_invocation_id) id: vec3<u32>) {
    if !valid(id) { return; }
    let i=id.y*p.shape.x+id.x;
    let floor_value=p.values.x*SIGNAL_FLOOR;
    let center=log2(max(guide_y(i),floor_value));
    let radius=i32(p.shape.z);
    var sum=vec3(0.); var total=0.;
    for(var k=-radius;k<=radius;k++) {
        var q=vec2<i32>(id.xy);
        if p.shape.w==0u { q.x+=k; } else { q.y+=k; }
        q=clamp(q,vec2(0),vec2<i32>(p.shape.xy)-1);
        let n=u32(q.y)*p.shape.x+u32(q.x);
        let difference=(log2(max(guide_y(n),floor_value))-center)/RANGE_SIGMA_STOPS;
        let distance=f32(k)/max(f32(radius)*SPATIAL_SIGMA_FRACTION,1.);
        let weight=exp(-0.5*(distance*distance+difference*difference));
        sum+=read_adjustment(n)*weight;
        total+=weight;
    }
    store_adjustment(i,sum/total);
}
@compute @workgroup_size(8,8)
fn color_editor_apply(@builtin(global_invocation_id) id: vec3<u32>) {
    if !valid(id) { return; }
    let i=id.y*p.shape.x+id.x; let n=i*3u;
    let control=auxiliary_adjustment(i);
    var result=rgb(i)*exp2(control.z);
    // Lightness-only edits avoid all color-space round trips, including in monochrome.
    if control.x!=0. || control.y!=0. {
        var normalized=result/p.values.z;
        var lab=to_lab(transform(normalized,p.to_srgb));
        let angle=control.x*MAX_HUE_RADIANS;
        if angle!=0. {
            lab=rotate_chroma(lab,angle);
            normalized=transform(from_lab(lab),p.from_srgb);
        }
        let graded=perceptual_saturate(normalized,lab,control.y,0.,p.protection.x!=0.,p.from_srgb);
        if angle!=0. || any(graded!=normalized) { result=graded*p.values.z; }
    }
    dst[n]=result.x; dst[n+1u]=result.y; dst[n+2u]=result.z;
}
