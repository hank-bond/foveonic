struct Params { shape: vec4<u32>, controls: vec4<f32>, noise: array<vec4<f32>, 3> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read> intensity: array<f32>;
@group(0) @binding(3) var<storage, read> range: array<f32>;
@group(0) @binding(4) var<storage, read> adaptive: array<f32>;
@group(0) @binding(5) var<storage, read_write> output: array<f32>;

fn divide(a: f32, b: f32) -> f32 {
    let quotient = a / b;
    return fma(1., quotient, fma(-quotient, b, a) / b);
}

@compute @workgroup_size(8, 8)
fn scalar_bilateral(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = params.shape.x;
    let height = params.shape.y;
    let radius = params.shape.z;
    if id.x >= width || id.y >= height { return; }
    let pixel = id.y * width + id.x;
    let center = source[pixel];
    if id.x < radius || id.x + radius + 1u >= width
        || id.y < radius || id.y + params.shape.w >= height {
        output[pixel] = center;
        return;
    }
    var variance = add_pair(params.noise[1].xy, multiply_pair(params.noise[2].xy, center));
    variance = add_pair(params.noise[0].xy, multiply_pair(variance, center));
    let magnitude = abs(variance.x);
    if magnitude <= 0. {
        output[pixel] = center;
        return;
    }
    let intensity_index = 14382u - u32(clamp(center, 1., 14382.));
    let scale = divide(
        divide(params.controls.y, fma(intensity[intensity_index], magnitude, 0.)),
        params.controls.x,
    );
    var numerator = 0.;
    var denominator = 0.;
    let last_y = min(2u * radius, height - 1u - id.y + radius);
    for (var y = 0u; y <= last_y; y++) {
        for (var x = 0u; x <= 2u * radius; x++) {
            let value = source[(id.y + y - radius) * width + id.x + x - radius];
            let delta = fma(-1., center, value);
            let square = fma(fma(scale, delta, 0.), delta, 0.);
            let index = u32(fma(1., square, .5));
            if index < 2048u {
                let weight = range[index];
                denominator = fma(1., weight, denominator);
                numerator = fma(value, weight, numerator);
            }
        }
    }
    var filtered = center;
    if denominator > 0. { filtered = divide(numerator, denominator); }
    let position = filtered * params.controls.z;
    let integer = floor(position);
    let index = u32(clamp(integer + select(0., 1., position - integer >= .5), 0., 16383.));
    let alpha = adaptive[index];
    output[pixel] = fma(center, 1. - alpha, fma(filtered, alpha, 0.));
}
