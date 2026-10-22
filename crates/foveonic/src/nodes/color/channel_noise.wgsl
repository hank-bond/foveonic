struct Params { shape: vec4<u32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> extract_params: Params;
@group(0) @binding(2) var<storage, read_write> blue: array<f32>;

@compute @workgroup_size(8, 8)
fn channel_noise_extract(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= extract_params.shape.x || id.y >= extract_params.shape.y { return; }
    let n = id.y * extract_params.shape.x + id.x;
    blue[n] = source[n * 3u + 2u];
}
