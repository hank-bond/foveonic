// Present the selected image using its display mapping.
struct Params { rect: vec4<f32>, options: vec4<f32> }
@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var input_image: texture_2d<f32>;
struct Vertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs(@builtin(vertex_index) i: u32) -> Vertex {
    let xy = array<vec2<f32>, 6>(vec2(0.,0.), vec2(1.,0.), vec2(0.,1.), vec2(0.,1.), vec2(1.,0.), vec2(1.,1.));
    var out: Vertex; out.uv = xy[i]; out.position = vec4(xy[i].x * 2. - 1., 1. - xy[i].y * 2., 0., 1.); return out;
}
fn fetch(p: vec2<i32>) -> vec3<f32> {
    let q = clamp(p, vec2(0), vec2<i32>(textureDimensions(image)) - 1);
    let output = textureLoad(image, q, 0).rgb;
    // Subtract immutable, same-geometry signal samples before filtering and display mapping.
    if params.options.z < -2.5 { return output - textureLoad(input_image, q, 0).rgb; }
    return output;
}
fn sample_at(uv: vec2<f32>) -> vec3<f32> {
    let p = uv * vec2<f32>(textureDimensions(image)) - 0.5;
    let base = vec2<i32>(floor(p)); let f = fract(p);
    return mix(mix(fetch(base), fetch(base + vec2(1,0)), f.x), mix(fetch(base + vec2(0,1)), fetch(base + vec2(1,1)), f.x), f.y);
}
fn encode(v: vec3<f32>) -> vec3<f32> { return select(1.055 * pow(v, vec3(1./2.4)) - 0.055, v * 12.92, v <= vec3(0.0031308)); }
@fragment fn fs(v: Vertex) -> @location(0) vec4<f32> {
    let uv = (v.uv - params.rect.xy) / params.rect.zw;
    if any(uv < vec2(0.)) || any(uv > vec2(1.)) { return vec4(select(vec3(0.055), vec3(0.0044), params.options.w > 0.), 1.); }
    var rgb = sample_at(uv);
    let channel = i32(params.options.x);
    if channel >= 0 { rgb = vec3(rgb[channel]); }
    else if params.options.z > 0. { rgb = vec3(1.39835574 * rgb.r - 0.39835574 * rgb.g, rgb.g, 1.04292899 * rgb.b - 0.04292899 * rgb.g); }
    rgb *= params.options.y;
    if params.options.z < -1.5 { rgb += 0.5; }
    rgb = clamp(rgb, vec3(0.), vec3(1.));
    if params.options.z < 0. {
        // Map scalar coverage directly to display gray.
        if params.options.w > 0. { rgb = select(pow((rgb + 0.055) / 1.055, vec3(2.4)), rgb / 12.92, rgb <= vec3(0.04045)); }
    } else if params.options.w == 0. { rgb = encode(rgb); }
    return vec4(rgb, 1.);
}
