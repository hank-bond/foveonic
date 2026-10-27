// Shared ABI and RGB access. Luminance weights declare brightness units.
struct Params {
    shape: vec4<u32>,
    values: vec4<f32>,
    luma: vec4<f32>,
}
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage, read_write> output: array<f32>;
const EPSILON: f32 = 0.000001;

fn valid(id: vec3<u32>) -> bool {
    return all(id.xy < p.shape.xy);
}
fn index(point: vec2<i32>) -> u32 {
    let q = vec2<u32>(clamp(point, vec2<i32>(0), vec2<i32>(p.shape.xy) - 1));
    return q.y * p.shape.x + q.x;
}
fn rgb(i: u32) -> vec3<f32> {
    let n = i * 3u;
    return vec3(source[n], source[n + 1u], source[n + 2u]);
}
fn store(i: u32, v: vec3<f32>) {
    output[i * 3u] = v.x;
    output[i * 3u + 1u] = v.y;
    output[i * 3u + 2u] = v.z;
}
fn light(i: u32) -> f32 {
    return max(dot(rgb(i), p.luma.xyz), 0.);
}
