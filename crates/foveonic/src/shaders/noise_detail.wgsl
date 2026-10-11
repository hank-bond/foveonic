// Adapted from Foveonic conditioning, residual mixing, and reconstruction_signal at e47af35.
@group(0) @binding(0) var<storage, read> input_values: array<f32>;
@group(0) @binding(1) var<uniform> params: NoiseParams;
@group(0) @binding(2) var<storage, read_write> output_values: array<f32>;
@group(0) @binding(3) var<storage, read> base: array<f32>;
@group(0) @binding(4) var<storage, read> ratios: array<f32>;

fn base_rgb(pixel: u32) -> vec3<f32> { let q = pixel * 3u; return vec3(base[q], base[q + 1u], base[q + 2u]); }
fn raw_rgb(p: vec3<u32>) -> vec3<f32> {
    let xy = full_position(p, params);
    if any(xy < params.area.xy) || any(xy >= params.area.zw) { return vec3(1000.); }
    let q = (xy.y * params.source.x + xy.x) * 3u;
    return vec3(input_values[q], input_values[q + 1u], input_values[q + 2u]);
}
@compute @workgroup_size(8, 8)
fn luminance(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * params.source.z * params.source.w { return; }
    let p = atlas_position(id.xy, params);
    output_values[tile_pixel(p, params)] = dot(raw_rgb(p), params.weights.xyz);
}
@compute @workgroup_size(8, 8)
fn diffuse(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * params.source.z * params.source.w { return; }
    let p = atlas_position(id.xy, params);
    let pixel = tile_pixel(p, params);
    let value = input_values[pixel];
    var result = value;
    if p.x > 0u && p.x + 1u < params.tile.x && p.y > 0u && p.y + 1u < params.tile.y {
        var inverse = 1. / (params.diffusion.z * params.diffusion.z);
        if params.diffusion.w == 0. {
            let sigma = fma(value * params.mono.z, value, fma(params.mono.y, value, params.mono.x));
            inverse = 1. / max(sigma * sigma, 1e-12);
        }
        let d = vec4(input_values[pixel - params.tile.x], input_values[pixel + params.tile.x], input_values[pixel + 1u], input_values[pixel - 1u]) - value;
        let weight = 1. / (1. + d * d * inverse);
        result = fma(params.diffusion.x, dot(d, weight), value);
    }
    // Reference diffusion publishes delayed left-border samples.
    if p.x == 0u && p.y + 2u < params.tile.y { result = input_values[pixel + params.tile.x]; }
    if p.x + 1u == params.tile.x && p.y + 2u == params.tile.y { result = 0.; }
    output_values[pixel] = result;
}
@compute @workgroup_size(8, 8)
fn osf(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * params.source.z * params.source.w { return; }
    let p = atlas_position(id.xy, params);
    let pixel = tile_pixel(p, params);
    var value = input_values[pixel];
    if p.x > 0u && p.x + 1u < params.tile.x && p.y > 0u && p.y + 1u < params.tile.y {
        var low = 1e30;
        var high = -1e30;
        for (var y = -1; y <= 1; y++) {
            for (var x = -1; x <= 1; x++) {
                if x != 0 || y != 0 {
                    let neighbor = input_values[u32(i32(pixel) + y * i32(params.tile.x) + x)];
                    low = min(low, neighbor); high = max(high, neighbor);
                }
            }
        }
        value = clamp(value, low, high);
    }
    if p.x == 0u && p.y + 2u < params.tile.y { value = input_values[pixel + params.tile.x]; }
    if p.x + 1u == params.tile.x && p.y + 1u < params.tile.y { value = input_values[p.z * params.tile.x * params.tile.y + params.tile.x - 1u]; }
    output_values[pixel] = value;
}
@compute @workgroup_size(8, 8)
fn ratio(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * params.source.z * params.source.w { return; }
    let pixel = id.y * params.tile.x + id.x;
    output_values[pixel] = (input_values[pixel] + params.edge.z) / (max(dot(base_rgb(pixel), params.weights.xyz), 0.) + params.edge.z);
}
fn strengthen(value: f32) -> f32 {
    let strength = params.residual.z * params.residual.w;
    if value < 1. { return 1. / (1. + (1. / max(value, 1e-12) - 1.) * strength); }
    return 1. + (value - 1.) * strength;
}
@compute @workgroup_size(8, 8)
fn residual_mix(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * params.source.z * params.source.w { return; }
    let p = atlas_position(id.xy, params);
    let pixel = tile_pixel(p, params);
    let a = base_rgb(pixel) * params.weights.xyz;
    let d = max(raw_rgb(p) * params.weights.xyz, vec3(0.));
    let raw_ratio = (d.x + d.y + d.z + params.edge.z) / (max(a.x + a.y + a.z, 0.) + params.edge.z);
    let mono = mix(raw_ratio, ratios[pixel], params.residual.x);
    let channel = (3. * d + params.edge.z) / (3. * a + params.edge.z);
    let gains = mix(vec3(mono), channel, params.residual.y);
    let q = pixel * 3u;
    output_values[q] = strengthen(gains.x);
    output_values[q + 1u] = strengthen(gains.y);
    output_values[q + 2u] = strengthen(gains.z);
}
fn brightness(pixel: u32) -> f32 {
    let rgb = max(base_rgb(pixel), vec3(0.));
    let ratio = ratios[pixel];
    let fade = 1. - clamp((params.edge.w - 100.) / 1500., 0., 1.);
    var gain = 1. + (ratio - 1.) * fade;
    if ratio <= 1. { gain = 1. / (1. + (1. / max(ratio, 1e-12) - 1.) * fade); }
    return min((rgb.x + rgb.y + rgb.z) * gain, 32767.);
}
@compute @workgroup_size(8, 8)
fn signal(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * params.source.z * params.source.w { return; }
    let p = atlas_position(id.xy, params);
    let pixel = tile_pixel(p, params);
    var value = 0.;
    if p.x >= 2u && p.x + 2u < params.tile.x && p.y >= 2u && p.y + 2u < params.tile.y {
        var sum = 0.;
        for (var y = -1; y <= 1; y++) {
            for (var x = -1; x <= 1; x++) { sum += brightness(u32(i32(pixel) + y * i32(params.tile.x) + x)); }
        }
        value = brightness(pixel) - sum / 9.;
    }
    output_values[pixel] = value;
}
@compute @workgroup_size(8, 8)
fn restore(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * params.source.z * params.source.w { return; }
    let pixel = id.y * params.tile.x + id.x;
    let q = pixel * 3u;
    let color = max(base_rgb(pixel), vec3(0.));
    let gain = vec3(input_values[q], input_values[q + 1u], input_values[q + 2u]);
    let value = clamp(color * gain, vec3(0.), vec3(32766.));
    output_values[q] = value.x; output_values[q + 1u] = value.y; output_values[q + 2u] = value.z;
}
