// Transfer encode a shared logical image view and pack RGB16 into two u32 words per pixel.
@group(0) @binding(2) var<storage, read_write> output_pixels: array<u32>;
struct Transform { rows: array<vec4<f32>, 3> };
@group(0) @binding(3) var<uniform> transform: Transform;

fn transfer(value: f32) -> u32 {
    let linear = clamp(value, 0.0, 1.0);
    var encoded: f32;
    if params.output_size.z == 0u {
        if linear <= 0.0031308 { encoded = linear * 12.92; }
        else { encoded = 1.055 * pow(linear, 1.0 / 2.4) - 0.055; }
    } else if params.output_size.z == 1u {
        encoded = pow(linear, 1.0 / 2.2);
    } else {
        // ProPhoto's linear toe matches the embedded ROMM profile.
        if linear < 1.0 / 512.0 { encoded = linear * 16.0; }
        else { encoded = pow(linear, 1.0 / 1.8); }
    }
    return u32(clamp(floor(encoded * 65535.0 + 0.5), 0.0, 65535.0));
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.output_size.x || id.y >= params.output_size.y { return; }
    var rgb = sample_rgb(id.xy);
    // Bypass identity conversion to keep existing exports byte-identical.
    if params.output_size.w != 0u {
        rgb = vec3<f32>(dot(transform.rows[0].xyz, rgb),
            dot(transform.rows[1].xyz, rgb), dot(transform.rows[2].xyz, rgb));
    }
    let output_index = (id.y * params.output_size.x + id.x) * 2u;
    output_pixels[output_index] = transfer(rgb.r) | (transfer(rgb.g) << 16u);
    output_pixels[output_index + 1u] = transfer(rgb.b);
}
