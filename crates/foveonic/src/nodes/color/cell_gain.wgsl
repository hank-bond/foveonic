struct Params {
    shape: vec4<u32>, spacing: vec4<u32>
}

@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read> grid: array<f32>;
@group(0) @binding(3) var<storage, read> rows: array<vec4<u32>>;
@group(0) @binding(4) var<storage, read> segments: array<vec4<u32>>;
@group(0) @binding(5) var<storage, read_write> destination: array<f32>;

fn divide(a: f32, b: f32) -> f32 {
    let q = a / b;
    return fma(1.0, q, fma(-q, b, a) / b);
}

@compute @workgroup_size(8, 8)
fn cell_gain(@builtin(global_invocation_id) id: vec3<u32>) {
    let shape = params.shape;
    if id.x >= shape.z-1u || id.y >= shape.y {
        return;
    }
    let segment = segments[id.x];
    let row = rows[id.y];
    let fraction = bitcast<f32>(row.z);
    let knot = segment.z;
    let a = grid[row.x * shape.z + knot];
    let b = grid[row.y * shape.z + knot];
    let c = grid[row.x * shape.z + knot + 1u];
    let d = grid[row.y * shape.z + knot + 1u];
    var gain = fma(fraction, fma(-1.0, a, b), a);
    let second = fma(fraction, fma(-1.0, c, d), c);
    let slope = divide(fma(-1.0, gain, second), bitcast<f32>(params.spacing.x));
    for (var x = segment.x;x<segment.y;x++) {
        let base = (id.y * 2u * shape.x * 2u + x * 2u);
        for (var dy = 0u;dy<2u;dy++) {
            for (var dx = 0u;dx<2u;dx++) {
                let i = base + dy * shape.x * 2u + dx;
                destination[i] = fma(gain, source[i], 0.0);
            }
        }
        gain = fma(1.0, gain, slope);
    }
}
