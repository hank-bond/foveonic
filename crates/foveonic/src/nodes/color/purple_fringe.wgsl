struct Params { shape: vec4<u32>, bounds: vec4<u32>, gains: vec4<f32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read> tables: array<f32>;
@group(0) @binding(3) var<storage, read> matrix: array<vec2<f32>>;
@group(0) @binding(4) var<storage, read> original: array<f32>;
@group(0) @binding(5) var<storage, read> baseline: array<f32>;
@group(0) @binding(6) var<storage, read_write> destination: array<f32>;

// Green correction transitions from five to six percent local disagreement.
const GREEN_RELATIVE_START: f32 = 0.05;
const GREEN_RELATIVE_WIDTH: f32 = 0.01;
const GREEN_DARK_FLOOR: f32 = 0.01;
const HUE_NOISE_FLOOR: f32 = 200.0;
const HUE_GATE_SCALE: f32 = 1000.0;
// The recovered brightness measure deliberately uses 0.33, not one third.
const BRIGHTNESS_FRACTION: f32 = 0.33;
const BRIGHTNESS_STABILIZER: f32 = 5.0;
const GREEN_ENERGY_FLOOR: vec2<f32> = vec2<f32>(0.001, -0.0000000000474974513053894);

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
fn multiply(a: vec2<f32>, b: f32) -> vec2<f32> { return add(product(a.x, b), product(a.y, b)); }
fn pair_product(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> { return add(multiply(a, b.x), multiply(a, b.y)); }
fn rounded(a: vec2<f32>) -> f32 { return fma(1.0, a.x, a.y); }
fn divide(a: f32, b: f32) -> f32 { let q = a / b; return fma(1.0, q, fma(-q, b, a) / b); }
fn pair_divide(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let q = a.x / b.x;
    let remainder = add(a, -multiply(b, q));
    return add(vec2<f32>(q, 0.0), vec2<f32>(divide(rounded(remainder), b.x), 0.0));
}
fn project(rgb: vec3<f32>) -> vec3<f32> {
    let value = vec3<f32>(fma(rgb.x, params.gains.x, 0.0), fma(rgb.y, params.gains.y, 0.0), fma(rgb.z, params.gains.z, 0.0));
    var result: vec3<f32>;
    for (var channel = 0u; channel < 3u; channel++) {
        let row = channel * 3u;
        result[channel] = rounded(add(add(multiply(matrix[row + 1u], value.y), multiply(matrix[row], value.x)), multiply(matrix[row + 2u], value.z)));
    }
    return result;
}
fn green(center: f32, neighbor: f32, weight: f32) -> f32 {
    let mean = fma(fma(1.0, center, neighbor), 0.5, 0.0);
    let relative = fma(1.0, divide(abs(fma(-1.0, mean, center)), max(GREEN_DARK_FLOOR, max(center, mean))), -GREEN_RELATIVE_START);
    let amount = fma(1.0, divide(clamp(relative, 0.0, GREEN_RELATIVE_WIDTH), -GREEN_RELATIVE_WIDTH), 1.0);
    let candidate = divide(fma(neighbor, weight, center), fma(1.0, weight, 1.0));
    return fma(center, amount, fma(candidate, fma(-1.0, amount, 1.0), 0.0));
}
fn hue(rgb: vec3<f32>) -> f32 {
    let high = max(rgb.x, max(rgb.y, rgb.z));
    let low = min(rgb.x, min(rgb.y, rgb.z));
    let delta = fma(-1.0, low, high);
    if delta == 0.0 { return 0.0; }
    var h: f32;
    if rgb.x == high {
        h = divide(fma(-1.0, rgb.z, rgb.y), delta);
        if h < 0.0 { h = fma(1.0, h, 6.0); }
    } else if rgb.y == high { h = fma(1.0, divide(fma(-1.0, rgb.x, rgb.z), delta), 2.0); }
    else { h = fma(1.0, divide(fma(-1.0, rgb.y, rgb.x), delta), 4.0); }
    return fma(h, 0.16666666666666666, 0.0);
}
fn deviation(rgb: vec3<f32>) -> vec2<f32> {
    let total = fma(1.0, fma(1.0, rgb.x, rgb.y), rgb.z);
    return add(vec2<f32>(rgb.y, 0.0), pair_divide(vec2<f32>(total, 0.0), vec2<f32>(-3.0, 0.0)));
}
@compute @workgroup_size(8, 8)
fn purple_fringe(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = params.shape.x;
    if id.x >= width || id.y >= params.shape.y { return; }
    let i = (id.y * width + id.x) * 3u;
    let rgb = vec3<f32>(source[i], source[i + 1u], source[i + 2u]);
    var result = vec3<f32>(baseline[i], baseline[i + 1u], baseline[i + 2u]);
    let bounds = params.bounds;
    if id.x >= bounds.x && id.x < bounds.z && id.y >= bounds.y && id.y <= bounds.w {
        let mode = params.shape.z;
        if mode == 0u {
            let other = select(id.x - 1u, id.x + 1u, id.x <= (bounds.z - bounds.x) / 2u);
            result.y = green(rgb.y, source[(id.y * width + other) * 3u + 1u], tables[id.x]);
        } else if mode == 1u {
            var amount = tables[width + id.x];
            result = rgb;
            if amount > 0.0 {
                let difference = fma(-1.0, baseline[i + 1u], rgb.y);
                let weight = tables[id.x];
                let gate = clamp(fma(difference, difference, fma(weight, -HUE_NOISE_FLOOR, 0.0)), 0.0, 1.0);
                amount = fma(amount, divide(gate, fma(weight, HUE_GATE_SCALE, 0.0)), 0.0);
                if amount > 0.0 {
                    let value = hue(project(rgb));
                    let centers = array<f32, 7>(0.0, 0.16666666666666666, 0.3333333333333333, 0.5, 0.6666666666666666, 0.8333333333333334, 1.0);
                    var triangles: array<f32, 7>;
                    var total = 0.0;
                    for (var n = 0u; n < 7u; n++) {
                        triangles[n] = max(0.0, fma(-1.0, fma(abs(fma(-1.0, value, centers[n])), 6.0, 0.0), 1.0));
                        total = fma(1.0, total, triangles[n]);
                    }
                    let selected = fma(1.0, fma(1.0, triangles[3], triangles[4]), triangles[5]);
                    if total > 0.0 { amount = fma(amount, divide(selected, total), 0.0); }
                    // The native destination still owns its original red and blue.
                    result.x = baseline[i];
                    result.z = baseline[i + 2u];
                    result.y = divide(fma(fma(fma(1.0, rgb.x, rgb.z), amount, 0.0), 0.5, rgb.y), fma(1.0, amount, 1.0));
                }
            }
        } else {
            let other = select(id.y - 1u, id.y + 1u, id.y <= (bounds.w - bounds.y) / 2u);
            result.y = green(rgb.y, source[(other * width + id.x) * 3u + 1u], tables[width * 2u + id.y]);
            let old = vec3<f32>(original[i], original[i + 1u], original[i + 2u]);
            let p = deviation(project(old));
            let q = deviation(project(result));
            let square = pair_product(p, p);
            let denominator = add(add(square, GREEN_ENERGY_FLOOR), pair_product(q, q));
            let amount = rounded(add(vec2<f32>(1.0, 0.0), -pair_divide(square, denominator)));
            let other_amount = fma(-1.0, amount, 1.0);
            for (var channel = 0u; channel < 3u; channel++) {
                result[channel] = fma(1.0, fma(old[channel], amount, 0.0), fma(result[channel], other_amount, 0.0));
            }
            let brightness = fma(fma(1.0, fma(1.0, result.y, result.x), result.z), BRIGHTNESS_FRACTION, 0.0);
            if brightness <= 0.0 { result = old; }
            else {
                let old_brightness = fma(fma(1.0, fma(1.0, old.x, old.y), old.z), BRIGHTNESS_FRACTION, 0.0);
                let scale = divide(select(BRIGHTNESS_STABILIZER, fma(1.0, old_brightness, BRIGHTNESS_STABILIZER), old_brightness >= 0.0), fma(1.0, brightness, BRIGHTNESS_STABILIZER));
                for (var channel = 0u; channel < 3u; channel++) { result[channel] = fma(result[channel], scale, 0.0); }
            }
        }
    }
    destination[i] = result.x;
    destination[i + 1u] = result.y;
    destination[i + 2u] = result.z;
}
