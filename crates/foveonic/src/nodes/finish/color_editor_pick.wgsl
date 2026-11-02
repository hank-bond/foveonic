// Measure a small guide neighborhood without photographic adjustment parameters.
struct PickParams {
    shape: vec4<u32>, crop: vec4<u32>, guide_matrix: array<vec4<f32>,3>,
    normalization: vec4<f32>,
}
@group(0) @binding(0) var<storage,read> src: array<f32>;
@group(0) @binding(1) var<uniform> p: PickParams;
@group(0) @binding(2) var<storage,read_write> dst: array<f32>;

@compute @workgroup_size(1)
fn color_editor_pick() {
    var sample=vec3(0.);
    for (var y=-1;y<=1;y++) {
        for (var x=-1;x<=1;x++) {
            let q=clamp(vec2<i32>(p.shape.zw)+vec2(x,y),vec2<i32>(p.crop.xy),vec2<i32>(p.crop.zw)-1);
            let n=(u32(q.y)*p.shape.x+u32(q.x))*3u;
            sample+=vec3(src[n],src[n+1u],src[n+2u]);
        }
    }
    sample/=9.;
    let c=max(vec3(dot(sample,p.guide_matrix[0].xyz),dot(sample,p.guide_matrix[1].xyz),dot(sample,p.guide_matrix[2].xyz)),vec3(0.));
    let hi=max(c.x,max(c.y,c.z)); let lo=min(c.x,min(c.y,c.z));
    let chroma=hi-lo;
    let reliable=hi>p.normalization.x*SIGNAL_FLOOR*SHADOW_FADE && chroma>hi*NEUTRAL_CHROMA_LIMIT;
    let weights=range_weights(c);
    for (var k=0u;k<8u;k++) { dst[k]=select(0.,weights[k]/max(chroma,1e-20),reliable); }
}
