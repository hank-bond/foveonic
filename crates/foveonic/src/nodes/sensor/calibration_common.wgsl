// Shared spatial calibration evaluated in original sensor coordinates.
struct CalibrationParams {
    source: vec4<u32>, dark: vec4<u32>, gains: vec4<f32>,
    shading: vec4<f32>, shading_geometry: vec4<f32>,
    grids: array<vec4<u32>, 3>,
};
fn spatial_gain(c: u32, point: vec2<f32>) -> f32 {
    let grid = calibration.grids[c];
    let coordinate = vec2<f32>(point) * vec2<f32>(f32(grid.y - 1u) / f32(calibration.source.x), f32(grid.z - 1u) / f32(calibration.source.y - 1u));
    let lower = vec2<u32>(floor(coordinate));
    let upper = min(lower + vec2<u32>(1u), vec2<u32>(grid.y - 1u, grid.z - 1u));
    let blend = fract(coordinate);
    let a = grid_values[grid.x + lower.y * grid.y + lower.x];
    let b = grid_values[grid.x + lower.y * grid.y + upper.x];
    let c0 = grid_values[grid.x + upper.y * grid.y + lower.x];
    let d = grid_values[grid.x + upper.y * grid.y + upper.x];
    return mix(mix(a, b, blend.x), mix(c0, d, blend.x), blend.y);
}

fn calibration_gain(point: vec2<f32>) -> vec3<f32> {
    let delta = point - calibration.shading_geometry.xy;
    let r2 = dot(delta, delta) / (calibration.shading_geometry.z * calibration.shading_geometry.z);
    let r4 = r2 * r2;
    let shading = vec3<f32>(1.0 + calibration.shading.x * r4 + calibration.shading.y * r2, 1.0, 1.0 + calibration.shading.z * r4 + calibration.shading.w * r2);
    return vec3(spatial_gain(0u, point), spatial_gain(1u, point), spatial_gain(2u, point)) * calibration.gains.xyz * shading;
}

// Calibration varies slowly within a pooled cell. Its center supplies a local
// sensor-code basis for filtering, not a second calibration of the photograph.
fn working_calibration_gain(point: vec2<u32>, divisor: u32, origin: vec2<u32>) -> vec3<f32> {
    let cell = min(vec2<u32>(max(vec2<i32>(point) - vec2<i32>(origin), vec2<i32>(0))),
        (calibration.source.xy - vec2(1u)) / divisor);
    let first = cell * divisor;
    let last = min(first + vec2(divisor), calibration.source.xy);
    return calibration_gain((vec2<f32>(first) + vec2<f32>(last) - vec2(1.)) * 0.5);
}
