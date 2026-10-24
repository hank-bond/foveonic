// Present the selected image using its display mapping.
// The native surface tag must match this extended, gamma-encoded output.
override EXTENDED_DISPLAY: bool = false;
// Small native textures retain linear light for egui texture interpolation.
override LINEAR_OUTPUT: bool = false;
struct Params { rect: vec4<f32>, options: vec4<f32>, primaries: array<vec4<f32>, 3>, color_transform: ColorTransform }
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
    // Subtract in the signal's own units before display mapping. Subtracting
    // mapped colors would show display changes rather than correction detail.
    if params.options.z < -2.5 { return output - textureLoad(input_image, q, 0).rgb; }
    return output;
}
fn sample_at(uv: vec2<f32>) -> vec3<f32> {
    let p = uv * vec2<f32>(textureDimensions(image)) - 0.5;
    let base = vec2<i32>(floor(p)); let f = fract(p);
    return mix(mix(fetch(base), fetch(base + vec2(1,0)), f.x), mix(fetch(base + vec2(0,1)), fetch(base + vec2(1,1)), f.x), f.y);
}
fn encode(v: vec3<f32>) -> vec3<f32> { return select(1.055 * pow(v, vec3(1./2.4)) - 0.055, v * 12.92, v <= vec3(0.0031308)); }
fn encode_extended(v: vec3<f32>) -> vec3<f32> { return sign(v) * encode(abs(v)); }
@fragment fn fs(v: Vertex) -> @location(0) vec4<f32> {
    let uv = (v.uv - params.rect.xy) / params.rect.zw;
    if any(uv < vec2(0.)) || any(uv > vec2(1.)) { return vec4(select(vec3(0.055), vec3(0.0044), params.options.w > 0.), 1.); }
    var rgb = sample_at(uv);
    // Bound SDR in the image's own primaries before converting it. The converted
    // components can be negative or above one without representing HDR white.
    if EXTENDED_DISPLAY && params.options.z >= 0. { rgb = clamp(rgb * params.options.y, vec3(0.), vec3(1.)); }
    let channel = i32(params.options.x);
    if channel >= 0 { rgb = vec3(rgb[channel]); }
    else if params.options.z >= 0. { rgb = apply_color_matrix(rgb, params.primaries); }
    if !EXTENDED_DISPLAY || params.options.z < 0. { rgb *= params.options.y; }
    if params.options.z < -1.5 { rgb += 0.5; }
    if params.options.z >= 0. { rgb = map_display_color(rgb, params.color_transform); }
    if !EXTENDED_DISPLAY || params.options.z < 0. { rgb = clamp(rgb, vec3(0.), vec3(1.)); }
    if params.options.z < 0. {
        // Coverage is a mask, not a color value. A color transfer would distort
        // how much of the neighborhood is marked.
        if params.options.w > 0. { rgb = select(pow((rgb + 0.055) / 1.055, vec3(2.4)), rgb / 12.92, rgb <= vec3(0.04045)); }
    } else if EXTENDED_DISPLAY && !LINEAR_OUTPUT { rgb = encode_extended(rgb); }
    else if !EXTENDED_DISPLAY && params.options.w == 0. { rgb = encode(rgb); }
    return vec4(rgb, 1.);
}
