// Shared sampling of a logical crop/orientation/size from floating RGB or a scalar field.
struct Params { source: vec4<u32>, crop: vec4<u32>, output_size: vec4<u32> };
@group(0) @binding(0) var<storage, read> input_pixels: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;

fn pixel(point: vec2<u32>, channel: u32) -> f32 {
    let index = point.y * params.source.x + point.x;
    if params.source.w == 1u { return input_pixels[index]; }
    return input_pixels[index * 3u + channel];
}

fn sample_rgb(id: vec2<u32>) -> vec3<f32> {
    let size = params.crop.zw - params.crop.xy;
    var oriented_size = size;
    if params.source.z == 90u || params.source.z == 270u { oriented_size = size.yx; }
    var oriented = vec2<f32>(id);
    if any(oriented_size != params.output_size.xy) {
        oriented = clamp((vec2<f32>(id) + vec2<f32>(0.5)) * vec2<f32>(oriented_size) / vec2<f32>(params.output_size.xy) - vec2<f32>(0.5), vec2<f32>(0.0), vec2<f32>(oriented_size) - vec2<f32>(1.0));
    }
    return sample_oriented(oriented);
}

fn sample_oriented(oriented: vec2<f32>) -> vec3<f32> {
    let size = params.crop.zw - params.crop.xy;
    var point = oriented;
    switch params.source.z {
        case 90u: { point = vec2<f32>(oriented.y, f32(size.y - 1u) - oriented.x); }
        case 180u: { point = vec2<f32>(size - vec2<u32>(1u)) - oriented; }
        case 270u: { point = vec2<f32>(f32(size.x - 1u) - oriented.y, oriented.x); }
        default: {}
    }
    point += vec2<f32>(params.crop.xy);
    let lower = vec2<u32>(floor(point));
    let upper = min(lower + vec2<u32>(1u), params.crop.zw - vec2<u32>(1u));
    let fraction = fract(point);
    var rgb: vec3<f32>;
    for (var c = 0u; c < 3u; c++) {
        let top = mix(pixel(lower, c), pixel(vec2<u32>(upper.x, lower.y), c), fraction.x);
        let bottom = mix(pixel(vec2<u32>(lower.x, upper.y), c), pixel(upper, c), fraction.x);
        rgb[c] = mix(top, bottom, fraction.y);
    }
    return rgb;
}
