// Apply black subtraction, interpolated spatial gains, WB factors, and radial shading.
struct Params {
    source: vec4<u32>, dark: vec4<u32>, gains: vec4<f32>,
    shading: vec4<f32>, shading_geometry: vec4<f32>,
    grids: array<vec4<u32>, 3>,
};
@group(0) @binding(0) var<storage, read> input_pixels: array<f32>;
@group(0) @binding(1) var<storage, read> black: array<f32>;
@group(0) @binding(2) var<storage, read> grid_values: array<f32>;
@group(0) @binding(3) var<uniform> params: Params;
@group(0) @binding(4) var<storage, read_write> output_pixels: array<f32>;

fn spatial_gain(c: u32, point: vec2<u32>) -> f32 {
    let grid = params.grids[c];
    let coordinate = vec2<f32>(point) * vec2<f32>(f32(grid.y - 1u) / f32(params.source.x), f32(grid.z - 1u) / f32(params.source.y - 1u));
    let lower = vec2<u32>(floor(coordinate));
    let upper = min(lower + vec2<u32>(1u), vec2<u32>(grid.y - 1u, grid.z - 1u));
    let blend = fract(coordinate);
    let a = grid_values[grid.x + lower.y * grid.y + lower.x];
    let b = grid_values[grid.x + lower.y * grid.y + upper.x];
    let c0 = grid_values[grid.x + upper.y * grid.y + lower.x];
    let d = grid_values[grid.x + upper.y * grid.y + upper.x];
    return mix(mix(a, b, blend.x), mix(c0, d, blend.x), blend.y);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.source.x || id.y >= params.source.y { return; }
    let p = (id.y * params.source.x + id.x) * 3u;
    let delta = vec2<f32>(id.xy) - params.shading_geometry.xy;
    let r2 = dot(delta, delta) / (params.shading_geometry.z * params.shading_geometry.z);
    let r4 = r2 * r2;
    let shading = vec3<f32>(1.0 + params.shading.x * r4 + params.shading.y * r2, 1.0, 1.0 + params.shading.z * r4 + params.shading.w * r2);
    for (var c = 0u; c < 3u; c++) {
        output_pixels[p + c] = (input_pixels[p + c] - black[c]) * spatial_gain(c, id.xy) * params.gains[c] * shading[c];
    }
}
