// Estimate per-layer black from a trimmed histogram of original dark-shield samples.
struct Params {
    source: vec4<u32>, dark: vec4<u32>, gains: vec4<f32>,
    shading: vec4<f32>, shading_geometry: vec4<f32>,
    grids: array<vec4<u32>, 3>,
};
@group(0) @binding(0) var<storage, read> pixels: array<f32>;
@group(0) @binding(1) var<storage, read_write> histogram: array<atomic<u32>>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read_write> black: array<f32>;

@compute @workgroup_size(64)
fn histogram_pass(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if index >= params.source.z { return; }
    let width = params.dark.z - params.dark.x;
    let x = params.dark.x + index % width;
    let y = params.dark.y + index / width;
    let p = (y * params.source.x + x) * 3u;
    for (var c = 0u; c < 3u; c++) {
        let code = u32(pixels[p + c]);
        atomicAdd(&histogram[c * 4096u + code], 1u);
    }
}

@compute @workgroup_size(1)
fn mean_pass(@builtin(global_invocation_id) id: vec3<u32>) {
    let c = id.x;
    let count = params.source.z;
    let trim = count / 100u;
    let end = count - trim;
    var seen = 0u;
    var sum = 0u;
    for (var code = 0u; code < 4096u; code++) {
        let n = atomicLoad(&histogram[c * 4096u + code]);
        let low = max(seen, trim);
        let high = min(seen + n, end);
        if high > low { sum += (high - low) * code; }
        seen += n;
    }
    black[c] = f32(sum) / f32(count - 2u * trim);
}
