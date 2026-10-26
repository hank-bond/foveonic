// Luminance conditioning adapted from the reference implementation at e47af35.
@group(0) @binding(0) var<storage, read> input_values: array<f32>;
@group(0) @binding(1) var<uniform> params: NoiseParams;
@group(0) @binding(2) var<storage, read_write> output_values: array<f32>;
@group(0) @binding(3) var<storage, read> base: array<f32>;

fn base_rgb(pixel: u32) -> vec3<f32> { let q = pixel * 3u; return vec3(base[q], base[q + 1u], base[q + 2u]); }
fn raw_rgb(p: vec3<u32>) -> vec3<f32> {
    let q = tile_pixel(p,params)*3u;
    return vec3(input_values[q], input_values[q + 1u], input_values[q + 2u]);
}
@compute @workgroup_size(8, 8)
fn luminance(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * tile_count(params) { return; }
    let p = atlas_position(id.xy, params);
    output_values[tile_pixel(p, params)] = dot(raw_rgb(p), params.weights.xyz);
}
@compute @workgroup_size(8, 8)
fn diffuse(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * tile_count(params) { return; }
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
    if id.x >= params.tile.x || id.y >= params.tile.y * tile_count(params) { return; }
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
    if id.x >= params.tile.x || id.y >= params.tile.y * tile_count(params) { return; }
    let pixel = id.y * params.tile.x + id.x;
    output_values[pixel] = (input_values[pixel] + params.edge.z) / (max(dot(base_rgb(pixel), params.weights.xyz), 0.) + params.edge.z);
}
