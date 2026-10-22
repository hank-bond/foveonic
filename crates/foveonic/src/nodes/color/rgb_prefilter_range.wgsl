struct Params { shape: vec4<u32>, controls: vec4<f32>, gains: vec4<f32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<storage, read> mask: array<u32>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read> coefficients: array<vec2<f32>>;
@group(0) @binding(4) var<storage, read> intensity: array<f32>;
@group(0) @binding(5) var<storage, read> range_weights: array<f32>;
@group(0) @binding(6) var<storage, read_write> destination: array<f32>;
fn product(a: f32, b: f32) -> vec2<f32> {
    let high = fma(a, b, 0.0);
    return vec2<f32>(high, fma(a, b, -high));
}
fn add(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let sum = fma(1.0, a.x, b.x);
    let virtual_b = fma(-1.0, a.x, sum);
    let remainder_a = fma(-1.0, fma(-1.0, virtual_b, sum), a.x);
    let remainder_b = fma(-1.0, virtual_b, b.x);
    let error = fma(1.0, fma(1.0, remainder_a, remainder_b), fma(1.0, a.y, b.y));
    let high = fma(1.0, sum, error);
    return vec2<f32>(high, fma(-1.0, fma(-1.0, sum, high), error));
}
fn multiply(value: vec2<f32>, factor: f32) -> vec2<f32> {
    return add(product(value.x, factor), product(value.y, factor));
}
@compute @workgroup_size(8, 8)
fn rgb_prefilter_range(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = params.shape.x;
    let height = params.shape.y;
    if id.x >= width || id.y >= height { return; }
    let pixel = id.y * width + id.x;
    let i = pixel * 3u;
    let center = vec3<f32>(source[i], source[i + 1u], source[i + 2u]);
    var variance = vec3<f32>(0.0);
    for (var ch = 0u; ch < 3u; ch++) {
        let value = max(center[ch], 0.0);
        let polynomial = add(multiply(add(multiply(coefficients[ch + 6u], value), coefficients[ch + 3u]), value), coefficients[ch]);
        let index = 14382u - u32(floor(clamp(center[ch], 1.0, 14382.0)));
        var value_variance=fma(1.0,polynomial.x,polynomial.y);
        if params.shape.w!=0u {value_variance=abs(value_variance);}
        variance[ch] = fma(value_variance, intensity[index], 0.0);
    }
    if params.shape.w == 0u && mask[pixel] == 1u {
        var mixed = vec3<f32>(0.0);
        for (var ch = 0u; ch < 3u; ch++) {
            let row = 9u + ch * 3u;
            let initial = add(multiply(coefficients[row + 1u], variance.y), multiply(coefficients[row], variance.x));
            let result = add(initial, multiply(coefficients[row + 2u], variance.z));
            mixed[ch] = fma(1.0, result.x, result.y);
        }
        variance = mixed;
    }
    var strength=params.controls.x;
    if params.shape.w == 1u {
        let brightness=fma(1.,fma(1.,center.z,center.x),center.y)*.33;
        let lower=floor(brightness);
        let index=u32(clamp(lower+select(0.,1.,brightness-lower>=.5),0.,16383.));
        strength*=intensity[14385u+index];
    }
    let quotient = vec3<f32>(strength) / variance;
    let residual = fma(-quotient, variance, vec3<f32>(strength));
    let gain = fma(vec3<f32>(1.0), quotient, residual / variance);
    let radius = i32(params.shape.z);
    var last_row=radius;
    if params.shape.w!=0u {last_row=min(radius,i32(height)-1-i32(id.y));}
    var sum = vec3<f32>(0.0);
    var total = 0.0;
    for (var dy = -radius; dy <= last_row; dy++) {
        let y = u32(clamp(i32(id.y) + dy, 0, i32(height) - 1));
        for (var dx = -radius; dx <= radius; dx++) {
            let x = u32(clamp(i32(id.x) + dx, 0, i32(width) - 1));
            let j = (y * width + x) * 3u;
            let sample = vec3<f32>(source[j], source[j + 1u], source[j + 2u]);
            let difference = sample - center;
            let squared = fma(difference, difference, vec3<f32>(0.0));
            let red_green = fma(squared.x, gain.x, fma(squared.y, gain.y, 0.0));
            let distance = fma(squared.z, gain.z, red_green);
            let index = u32(max(fma(1.0, distance, 0.5), 0.0));
            var weight = 0.0;
            if index < 2048u { weight = range_weights[index]; }
            sum = fma(vec3<f32>(weight), sample, sum);
            total = fma(1.0, weight, total);
        }
    }
    for (var ch = 0u; ch < 3u; ch++) {
        var result = center[ch];
        if total > 0.0 {
            let quotient = sum[ch] / total;
            let residual = fma(-quotient, total, sum[ch]);
            result = fma(1.0, quotient, residual / total);
        }
        destination[i + ch] = result;
    }
}
