struct Params { shape: vec4<u32> }
@group(0) @binding(0) var<storage, read> rgb: array<f32>;
@group(0) @binding(1) var<storage, read> blue: array<f32>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read_write> destination: array<f32>;

@compute @workgroup_size(8, 8)
fn blue_refresh(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = params.shape.x;
    if id.x >= width || id.y >= params.shape.y { return; }
    let output = (id.y * width + id.x) * 3u;
    let top = (id.y * 2u) * (width * 2u) + id.x * 2u;
    // Explicit additions preserve the reference's top-left, top-right,
    // bottom-left, bottom-right Float32 summation order.
    let upper = fma(1.0, blue[top], blue[top + 1u]);
    let third = fma(1.0, upper, blue[top + width * 2u]);
    let total = fma(1.0, third, blue[top + width * 2u + 1u]);
    destination[output] = rgb[output];
    destination[output + 1u] = rgb[output + 1u];
    destination[output + 2u] = total * 0.25;
}
