// Transfer encode a shared logical image view and pack RGB16 into two u32 words per pixel.
@group(0) @binding(2) var<storage, read_write> output_pixels: array<u32>;

fn transfer(value: f32) -> u32 {
    let linear = clamp(value, 0.0, 1.0);
    var encoded: f32;
    if params.output_size.z == 0u {
        if linear <= 0.0031308 { encoded = linear * 12.92; }
        else { encoded = 1.055 * pow(linear, 1.0 / 2.4) - 0.055; }
    } else { encoded = pow(linear, 1.0 / 2.2); }
    return u32(clamp(floor(encoded * 65535.0 + 0.5), 0.0, 65535.0));
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.output_size.x || id.y >= params.output_size.y { return; }
    let rgb = sample_rgb(id.xy);
    let output_index = (id.y * params.output_size.x + id.x) * 2u;
    output_pixels[output_index] = transfer(rgb.r) | (transfer(rgb.g) << 16u);
    output_pixels[output_index + 1u] = transfer(rgb.b);
}
