// Reference-style reconstruction parameters and processing-tile coordinates.
struct NoiseParams {
    source: vec4<u32>,
    tile: vec4<u32>,
    area: vec4<u32>,
    filtering: vec4<f32>,
    edge: vec4<f32>,
    weights: vec4<f32>,
    model: array<vec4<f32>, 3>,
    mono: vec4<f32>,
    residual: vec4<f32>,
    diffusion: vec4<f32>,
    column_area:vec4<u32>,
    column:vec4<f32>,
    reduction:vec4<f32>,
    geometry:vec4<u32>,
}
fn tile_pixel(p: vec3<u32>, params: NoiseParams) -> u32 {
    return (p.z * params.tile.y + p.y) * params.tile.x + p.x;
}
fn atlas_position(p: vec2<u32>, params: NoiseParams) -> vec3<u32> {
    return vec3(p.x, p.y % params.tile.y, p.y / params.tile.y);
}
fn full_position(p: vec3<u32>, params: NoiseParams) -> vec2<u32> {
    let core = params.source.xy / params.source.zw;
    let origin = vec2(p.z % params.source.z, p.z / params.source.z) * core;
    var xy = vec2<i32>(origin + p.xy) - i32(params.tile.z);
    // The reference repeats the second top/left sample and the final bottom/right sample.
    xy = select(xy, vec2(1), xy < vec2(0));
    return vec2<u32>(min(xy, vec2<i32>(params.source.xy) - 1));
}
fn variance(v: vec3<f32>, params: NoiseParams) -> vec3<f32> {
    return max(params.model[0].xyz + abs(v) * params.model[1].xyz + v * v * params.model[2].xyz, vec3(1e-12));
}
