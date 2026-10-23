struct Params { shape: vec4<u32> }
@group(0) @binding(0) var<storage, read> source: array<u32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read_write> destination: array<f32>;

@compute @workgroup_size(8, 8)
fn mask_image(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.shape.x || id.y >= params.shape.y { return; }
    let index = id.y * params.shape.x + id.x;
    destination[index] = f32(source[index] != 0u);
}
