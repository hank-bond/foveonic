// Apply black subtraction, interpolated spatial gains, WB factors, and radial shading.
@group(0) @binding(0) var<storage, read> input_pixels: array<f32>;
@group(0) @binding(1) var<storage, read> black: array<f32>;
@group(0) @binding(2) var<storage, read> grid_values: array<f32>;
@group(0) @binding(3) var<uniform> calibration: CalibrationParams;
@group(0) @binding(4) var<storage, read_write> output_pixels: array<f32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= calibration.source.x || id.y >= calibration.source.y { return; }
    let p = (id.y * calibration.source.x + id.x) * 3u;
    let delta = vec2<f32>(id.xy) - calibration.shading_geometry.xy;
    let r2 = dot(delta, delta) / (calibration.shading_geometry.z * calibration.shading_geometry.z);
    let r4 = r2 * r2;
    let shading = vec3<f32>(1.0 + calibration.shading.x * r4 + calibration.shading.y * r2, 1.0, 1.0 + calibration.shading.z * r4 + calibration.shading.w * r2);
    for (var c = 0u; c < 3u; c++) {
        output_pixels[p + c] = (input_pixels[p + c] - black[c]) * spatial_gain(c, vec2<f32>(id.xy)) * calibration.gains[c] * shading[c];
    }
}
