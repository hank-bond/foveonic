@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> shape: vec4<u32>;
@group(0) @binding(2) var<storage, read> tables: array<f32>;
@group(0) @binding(3) var<storage, read_write> destination: array<f32>;

fn dot_row(base: u32, value: vec3<f32>) -> f32 {
    let green = fma(value.y, tables[base + 1u], 0.0);
    let red = fma(tables[base], value.x, green);
    return fma(tables[base + 2u], value.z, red);
}

fn divide(a: f32, b: f32) -> f32 {
    let q = a / b;
    return fma(1.0, q, fma(-q, b, a) / b);
}

@compute @workgroup_size(8, 8)
fn sector_color(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= shape.x || id.y >= shape.y {
        return;
    }
    let i = (id.y * shape.x + id.x) * 3u;
    let value = vec3<f32>(source[i], source[i + 1u], source[i + 2u]);
    let u = dot_row(3u, value);
    let v = dot_row(6u, value);
    var output = value;
    if all(value == vec3<f32>(value.x)) || (u == 0.0 && v == 0.0) {
        output = vec3<f32>(vec3<i32>(value));
    }
    else {
        let denominator = fma(1.0, fma(1.0, abs(u), abs(v)), 1e-10);
        let ratios = vec2<f32>(divide(fma(abs(u), 2048.0, 1e-10), denominator), divide(fma(abs(v), 2048.0, 1e-10), denominator));
        var quadrant = 3u;
        if u>0.0 {
            quadrant = select(4u, 1u, v>0.0);
        }
        else if v>0.0 {
            quadrant = 2u;
        }
        var accumulated = vec3<f32>(0.0);
        for (var index = 0u;index<20u;index++) {
            let base = 15u + index * 11u;
            let q = bitcast<u32>(tables[base + 9u]);
            let bin = bitcast<u32>(tables[base + 10u]);
            let ratio = select(ratios.y, ratios.x, (q&1u) == 1u);
            if q == quadrant && ratio <= tables[9u + bin-1u] && tables[9u + bin]<ratio {
                // Preserve the native accumulation order: existing sum precedes
                // green, red and blue terms rather than a precomputed dot product.
                for (var ch = 0u;ch<3u;ch++) {
                    let row = base + ch * 3u;
                    let green = fma(value.y, tables[row + 1u], accumulated[ch]);
                    let red = fma(tables[row], value.x, green);
                    accumulated[ch] = fma(tables[row + 2u], value.z, red);
                }
            }
        }
        if all(accumulated != vec3<f32>(0.0)) {
            output = accumulated;
        }
    }
    destination[i] = output.x;
    destination[i + 1u] = output.y;
    destination[i + 2u] = output.z;
}
