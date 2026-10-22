struct Params { shape: vec4<u32>, neutral: vec4<f32>, gains: vec4<f32>, appearance: vec4<f32>, response: vec4<f32>, hue_first: vec4<f32>, hue_last: vec4<f32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read_write> destination: array<f32>;
fn divide(a: f32, b: f32) -> f32 { let q = a / b; return fma(1.0, q, fma(-q, b, a) / b); }
@compute @workgroup_size(8, 8)
fn raw_color_neutralize(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.shape.x || id.y >= params.shape.y { return; }
    let i = (id.y * params.shape.x + id.x) * 3u;
    let scaled = fma(fma(vec3<f32>(source[i], source[i + 1u], source[i + 2u]), params.gains.xyz, vec3<f32>(0.0)), vec3<f32>(params.neutral.x), vec3<f32>(0.0));
    let third = 0.3333333333333333;
    let mean = fma(scaled.z, third, fma(scaled.x, third, fma(scaled.y, third, 0.0)));
    let amount = clamp(divide(fma(-1.0, params.neutral.y, mean), params.neutral.z), 0.0, 1.0);
    let common_value = fma(mean, amount, 0.0);
    let inverse = fma(-1.0, amount, 1.0);
    for (var ch = 0u; ch < 3u; ch++) { destination[i + ch] = divide(fma(scaled[ch], inverse, common_value), params.neutral.x); }
}
