// Selections always use the immutable guide, independently of authored adjustments.
struct Range { selection:vec4<f32>, adjustment:vec4<f32>, options:vec4<f32>, }
struct Params {
    shape:vec4<u32>, normalization:vec4<f32>, guide_matrix:array<vec4<f32>,3>,
    to_srgb:array<vec4<f32>,3>,from_srgb:array<vec4<f32>,3>,ranges:array<Range,30>,
    protection:vec4<f32>,
}
@group(0) @binding(0) var<storage,read> src:array<f32>;
@group(0) @binding(1) var<uniform> p:Params;
@group(0) @binding(2) var<storage,read_write> dst:array<f32>;
@group(0) @binding(3) var<storage,read> guide:array<f32>;
fn transform(v:vec3<f32>,rows:array<vec4<f32>,3>)->vec3<f32> {
    return vec3(dot(v,rows[0].xyz),dot(v,rows[1].xyz),dot(v,rows[2].xyz));
}
fn coverage(coordinate:vec3<f32>,r:Range)->f32 {
    let distance=abs((coordinate.x-r.selection.x+540.)%360.-180.);
    let feather=max(r.adjustment.w*60.,0.001);
    var hue=1.-smoothstep(r.selection.y*0.5,r.selection.y*0.5+feather,distance);
    if r.selection.y>=360. {hue=1.;}
    let sf=max(r.adjustment.w*0.5,0.00001);
    let low=smoothstep(r.selection.z-sf,r.selection.z,coordinate.y);
    let high=1.-smoothstep(r.selection.w,r.selection.w+sf,coordinate.y);
    return hue*low*high*coordinate.z;
}
@compute @workgroup_size(8,8)
fn advanced_color(@builtin(global_invocation_id) id:vec3<u32>) {
    if any(id.xy>=p.shape.xy) {return;}
    let n=(id.y*p.shape.x+id.x)*3u;
    let original=vec3(src[n],src[n+1u],src[n+2u]);
    let c=max(transform(vec3(guide[n],guide[n+1u],guide[n+2u]),p.guide_matrix),vec3(0.));
    let coordinates=color_coordinates(c,p.normalization.z);
    var amounts=vec3(0.);var preview=1.;
    for(var k=0u;k<p.shape.z;k++) {
        let r=p.ranges[k];let w=coverage(coordinates,r);
        if f32(k)==p.normalization.w {preview=w;}
        if r.options.x==0. {continue;}
        amounts+=w*r.adjustment.xyz;
    }
    var result=original;
    if any(amounts!=vec3(0.)) || p.normalization.w>=0. {
        let normalized=original*p.normalization.x;
        if p.shape.w!=0u {
            let lightness=sign(normalized.y)*pow(abs(normalized.y),1./3.);
            let value=lightness*exp2(2.*amounts.z/3.);
            result=vec3(value*value*value)*p.normalization.y;
        } else {
            // Saturation shares the pre-exposure boundary used by global and Basic controls.
            let source_color=original/p.protection.y;
            var color=source_color;
            if amounts.x!=0. || amounts.y!=0. {
                var lab=to_lab(transform(color,p.to_srgb));
                if amounts.x!=0. {
                    lab=rotate_chroma(lab,amounts.x*0.5235987756);
                    color=transform(from_lab(lab),p.from_srgb);
                }
                color=perceptual_saturate(color,lab,amounts.y,0.,p.protection.x!=0.,p.from_srgb);
            }
            if any(color!=source_color) { result=color*p.protection.y; }
            if amounts.z!=0. || p.normalization.w>=0. {
                var lab=to_lab(transform(result*p.normalization.x,p.to_srgb));
                lab.x=lab.x*exp2(2.*amounts.z/3.);
                if p.normalization.w>=0. {lab=vec3(lab.x,lab.yz*preview);}
                result=transform(from_lab(lab),p.from_srgb)*p.normalization.y;
            }
        }
    }
    dst[n]=result.x;dst[n+1u]=result.y;dst[n+2u]=result.z;
}
