// Materialize a floating image view as a texture without clipping, transfer encoding, or readback.
@group(0) @binding(2) var output_image: texture_storage_2d<rgba32float, write>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.output_size.x || id.y >= params.output_size.y { return; }
    textureStore(output_image, vec2<i32>(id.xy), vec4<f32>(sample_rgb(id.xy), 1.0));
}
