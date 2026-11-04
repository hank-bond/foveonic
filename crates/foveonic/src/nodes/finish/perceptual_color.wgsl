// Oklab matrices by Björn Ottosson, public-domain 2021 revision:
// https://bottosson.github.io/posts/oklab/
// Signed cube roots extend the transform to linear headroom and negative samples.
fn to_lab(v: vec3<f32>) -> vec3<f32> {
    let lms=vec3(dot(v,vec3(0.4122214708,0.5363325363,0.0514459929)),
                 dot(v,vec3(0.2119034982,0.6806995451,0.1073969566)),
                 dot(v,vec3(0.0883024619,0.2817188376,0.6299787005)));
    let q=sign(lms)*pow(abs(lms),vec3(1./3.));
    return vec3(dot(q,vec3(0.2104542553,0.7936177850,-0.0040720468)),
                dot(q,vec3(1.9779984951,-2.4285922050,0.4505937099)),
                dot(q,vec3(0.0259040371,0.7827717662,-0.8086757660)));
}
fn from_lms(lms: vec3<f32>) -> vec3<f32> {
    return vec3(dot(lms,vec3(4.0767416621,-3.3077115913,0.2309699292)),
                dot(lms,vec3(-1.2684380046,2.6097574011,-0.3413193965)),
                dot(lms,vec3(-0.0041960863,-0.7034186147,1.7076147010)));
}
fn lab_roots(v: vec3<f32>) -> vec3<f32> {
    return vec3(dot(v,vec3(1.,0.3963377774,0.2158037573)),
                dot(v,vec3(1.,-0.1055613458,-0.0638541728)),
                dot(v,vec3(1.,-0.0894841775,-1.2914855480)));
}
fn from_lab(v: vec3<f32>) -> vec3<f32> {
    let q=lab_roots(v);
    return from_lms(q*q*q);
}
fn rotate_chroma(lab: vec3<f32>, angle: f32) -> vec3<f32> {
    let c=cos(angle); let s=sin(angle);
    let chroma=vec2(c*lab.y-s*lab.z,s*lab.y+c*lab.z);
    return vec3(lab.x,chroma);
}
