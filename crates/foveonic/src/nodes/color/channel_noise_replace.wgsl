struct Params { shape: vec4<u32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<storage, read> blue: array<f32>;
@group(0) @binding(2) var<uniform> p: Params;
@group(0) @binding(3) var<storage, read_write> output: array<f32>;

@compute @workgroup_size(8, 8)
fn channel_noise_replace(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= p.shape.x || id.y >= p.shape.y { return; }
    let n = id.y * p.shape.x + id.x;
    output[n * 3u] = max(0., source[n * 3u]);
    output[n * 3u + 1u] = max(0., source[n * 3u + 1u]);
    output[n * 3u + 2u] = max(0., blue[n]);
}
