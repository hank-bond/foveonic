struct Params { shape: vec4<u32>, controls: vec4<f32>, gains: vec4<f32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read_write> destination: array<f32>;
@compute @workgroup_size(8, 8)
fn rgb_prefilter_gain(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.shape.x || id.y >= params.shape.y { return; }
    let i = (id.y * params.shape.x + id.x) * 3u;
    for (var ch = 0u; ch < 3u; ch++) {
        if params.shape.w != 0u {
// Correct the approximate GPU quotient before its final Float32 rounding.
            let quotient = source[i + ch] / params.gains[ch];
            let residual = fma(-quotient, params.gains[ch], source[i + ch]);
            destination[i + ch] = fma(1.0, quotient, residual / params.gains[ch]);
        }
        else { destination[i + ch] = source[i + ch] * params.gains[ch]; }
    }
}
