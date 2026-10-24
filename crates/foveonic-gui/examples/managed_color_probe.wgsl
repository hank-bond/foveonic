// Opaque photographic colors use linear sampling and a final extended sRGB transfer.
struct Vertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs(@builtin(vertex_index) index: u32) -> Vertex {
    let xy = array<vec2<f32>,3>(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.))[index];
    var result: Vertex;
    result.position = vec4(xy,0.,1.);
    result.uv = vec2((xy.x+1.)*0.5,(1.-xy.y)*0.5);
    return result;
}
fn encode(value: vec3<f32>) -> vec3<f32> {
    let absolute = abs(value);
    return sign(value) * select(1.055 * pow(absolute,vec3(1./2.4)) - 0.055, absolute * 12.92, absolute <= vec3(0.0031308));
}
@fragment fn fs(vertex: Vertex) -> @location(0) vec4<f32> {
    let colors = array<vec3<f32>,6>(vec3(1.,0.,0.),vec3(0.,1.,0.),vec3(0.,0.,1.),vec3(0.02,0.8,0.06),vec3(0.6,0.1,0.5),vec3(0.05,0.5,0.8));
    if vertex.uv.y >= 0.6666667 { return vec4(vec3(vertex.uv.x),1.); }
    let source = colors[min(u32(vertex.uv.x * 6.),5u)];
    var linear_srgb = vec3(1.39835574 * source.r - 0.39835574 * source.g, source.g, 1.04292899 * source.b - 0.04292899 * source.g);
    if vertex.uv.y >= 0.3333333 { linear_srgb = clamp(linear_srgb,vec3(0.),vec3(1.)); }
    return vec4(encode(linear_srgb),1.);
}
