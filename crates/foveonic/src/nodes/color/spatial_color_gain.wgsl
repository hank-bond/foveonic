struct Params {
    shape: vec4<u32>, spacing: vec4<u32>
}

@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read> tables: array<f32>;
@group(0) @binding(3) var<storage, read> axes: array<vec4<u32>>;
@group(0) @binding(4) var<storage, read_write> destination: array<f32>;

fn product(a: f32, b: f32) -> vec2<f32> {
    let h = fma(a, b, 0.0);
    return vec2<f32>(h, fma(a, b, -h));
}

fn add(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let s = fma(1.0, a.x, b.x);
    let v = fma(-1.0, a.x, s);
    let e = fma(1.0, fma(-1.0, fma(-1.0, v, s), a.x), fma(-1.0, v, b.x));
    let r = fma(1.0, e, fma(1.0, a.y, b.y));
    let h = fma(1.0, s, r);
    return vec2<f32>(h, fma(-1.0, fma(-1.0, s, h), r));
}

fn multiply(a: vec2<f32>, b: f32) -> vec2<f32> {
    return add(product(a.x, b), product(a.y, b));
}

@compute @workgroup_size(8, 8)
fn spatial_color_gain(@builtin(global_invocation_id) id: vec3<u32>) {
    let shape = params.shape;
    if id.x >= shape.x || id.y >= shape.y {
        return;
    }
    let i = (id.y * shape.x + id.x) * 3u;
    destination[i + 1u] = source[i + 1u];
    let x = axes[id.x];
    let y = axes[shape.x + id.y];
    let fraction = bitcast<f32>(y.z);
    for (var channel = 0u;channel<2u;channel++) {
        let base = channel * shape.z * shape.w;
        let a = tables[base + y.x * shape.z + x.x];
        let b = tables[base + y.y * shape.z + x.x];
        let c = tables[base + y.x * shape.z + x.x + 1u];
        let d = tables[base + y.y * shape.z + x.x + 1u];
        let first = fma(fraction, fma(-1.0, a, b), a);
        let second = fma(fraction, fma(-1.0, c, d), c);
        let delta = fma(-1.0, first, second);
        let step = multiply(vec2<f32>(bitcast<f32>(params.spacing.x), bitcast<f32>(params.spacing.y)), delta);
        let gain = add(vec2<f32>(first, 0.0), multiply(step, f32(x.y)));
        let result = multiply(gain, source[i + channel * 2u]);
        destination[i + channel * 2u] = fma(1.0, result.x, result.y);
    }
}
