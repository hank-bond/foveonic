struct Params { shape: vec4<u32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<storage, read> tables: array<f32>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read_write> destination: array<f32>;

@compute @workgroup_size(8, 8)
fn code_response(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.shape.x || id.y >= params.shape.y { return; }
    let pixel = id.y * params.shape.x + id.x;
    for (var channel = 0u; channel < 3u; channel++) {
        let code = u32(clamp(source[pixel * 3u + channel], 0.0, 65535.0));
        destination[pixel * 3u + channel] = tables[channel * 65536u + code];
    }
}
