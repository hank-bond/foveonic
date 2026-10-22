struct Params {
    shape: vec4<u32>, grid: vec4<u32>, count: vec4<u32>
}
// Preserve the recovered Float32 diagonal distances and dimensionless gates.
const DIAGONAL_DISTANCE: f32 = 1.4142135;
const DOUBLE_DIAGONAL_DISTANCE: f32 = 2.828427;
const VERTICAL_DOMINANCE_RATIO: f32 = 2.2;
const VERTICAL_BRIGHTNESS_RATIO: f32 = 0.45;
const HORIZONTAL_DOMINANCE_RATIO: f32 = 0.5;
const HORIZONTAL_BRIGHTNESS_RATIO: f32 = 0.1;
const PATCH_SEARCH_RADIUS: i32 = 10;
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<storage, read> mask: array<u32>;
@group(0) @binding(2) var<uniform> p: Params;
@group(0) @binding(3) var<storage, read> gains: array<f32>;
@group(0) @binding(4) var<storage, read_write> output: array<f32>;
fn at(point: vec2<i32>) -> f32 {
    return source[u32(point.y) * p.shape.x + u32(point.x)];
}
fn bad(point: vec2<i32>) -> u32 {
    return mask[u32(point.y) * p.shape.x + u32(point.x)];
}
// Explicit FMA calls retain Float32 rounding between native arithmetic steps.
fn add(a: f32, b: f32) -> f32 {
    return fma(1.0, a, b);
}
fn mul(a: f32, b: f32) -> f32 {
    return fma(a, b, 0.0);
}
fn delta(a: f32, b: f32) -> f32 {
    return abs(add(a, -b));
}
// Correct the quotient residual so Metal's approximate divide cannot change
// direction selection when two gradients are almost equal.
fn divide(a: f32, b: f32) -> f32 {
    let quotient = a/b;
    return fma(fma(-quotient, b, a), 1.0/b, quotient);
}
fn median(a: f32, b: f32, c: f32) -> f32 {
    return max(min(a, b), min(max(a, b), c));
}
// The two corrected centers are e/h in this neighborhood: // a b c
// d e f
// g h i
// j k l
fn pair(center: vec2<i32>, gain: vec2<f32>) -> vec2<f32> {
    let a = at(center+vec2<i32>(-1, -1));
    let b = at(center+vec2<i32>(0, -1));
    let c = at(center+vec2<i32>(1, -1));
    let d = at(center+vec2<i32>(-1, 0));
    let e = at(center);
    let f = at(center+vec2<i32>(1, 0));
    let g = at(center+vec2<i32>(-1, 1));
    let h = at(center+vec2<i32>(0, 1));
    let i = at(center+vec2<i32>(1, 1));
    let j = at(center+vec2<i32>(-1, 2));
    let k = at(center+vec2<i32>(0, 2));
    let l = at(center+vec2<i32>(1, 2));
    let n = (bad(center+vec2<i32>(0, -1))+bad(center+vec2<i32>(-1, 0))+bad(center+vec2<i32>(1, 0))+bad(center+vec2<i32>(-1, 1))+bad(center+vec2<i32>(1, 1))+bad(center+vec2<i32>(0, 2)))&255u;
    let horizontal = divide(add(add(add(add(add(delta(a, b), delta(b, c)), mul(delta(d, f), 0.5)), mul(delta(g, i), 0.5)), delta(j, k)), delta(k, l)), 6.0);
    let vertical = divide(add(add(add(add(add(delta(a, d), delta(d, g)), delta(g, j)), delta(i, l)), delta(f, i)), delta(c, f)), 6.0);
    let centers = add(e, h);
    if n == 0u && (vertical>mul(horizontal, VERTICAL_DOMINANCE_RATIO) || vertical>mul(centers, VERTICAL_BRIGHTNESS_RATIO)) {
        return vec2<f32>(median(d, f, mul(e, gain.x)), median(g, i, mul(h, gain.y)));
    }
    if n == 0u && (vertical<mul(horizontal, HORIZONTAL_DOMINANCE_RATIO) || vertical<mul(centers, HORIZONTAL_BRIGHTNESS_RATIO)) {
        let diagonal = mul(add(add(add(divide(delta(g, c), DOUBLE_DIAGONAL_DISTANCE), divide(delta(b, d), DIAGONAL_DISTANCE)), divide(delta(i, k), DIAGONAL_DISTANCE)), divide(delta(j, f), DOUBLE_DIAGONAL_DISTANCE)), 0.25);
        let other = mul(add(add(add(divide(delta(b, f), DIAGONAL_DISTANCE), divide(delta(a, i), DOUBLE_DIAGONAL_DISTANCE)), divide(delta(g, k), DIAGONAL_DISTANCE)), divide(delta(d, l), DOUBLE_DIAGONAL_DISTANCE)), 0.25);
        // Preserve the instruction-level sum order above. Near-equal gradients
        // can select different diagonals when the four terms are reordered.
        if min(vertical, diagonal) <= other {
            if vertical <= diagonal {
                return vec2<f32>(divide(add(k, mul(b, 2.0)), 3.0), divide(add(b, mul(k, 2.0)), 3.0));
            }
            return vec2<f32>(mul(add(c, g), 0.5), mul(add(f, j), 0.5));
        }
        return vec2<f32>(mul(add(a, i), 0.5), mul(add(d, l), 0.5));
    }
    if n >= 4u {
        return vec2<f32>(mul(e, gain.x), mul(h, gain.y));
    }
    let offsets = array<vec2<i32>, 10>(vec2<i32>(-1, -1), vec2<i32>(0, -1), vec2<i32>(1, -1), vec2<i32>(-1, 0), vec2<i32>(1, 0), vec2<i32>(-1, 1), vec2<i32>(1, 1), vec2<i32>(-1, 2), vec2<i32>(0, 2), vec2<i32>(1, 2));
    var best = 3.4028234663852886e38;
    var chosen = vec2<f32>(e, h);
    for(var dy = -PATCH_SEARCH_RADIUS;dy <= PATCH_SEARCH_RADIUS;dy++) {
        for(var dx = -PATCH_SEARCH_RADIUS;dx <= PATCH_SEARCH_RADIUS;dx++) {
            let candidate = center+vec2<i32>(dx, dy);
            if (abs(dy) <= 1 && dx == 0) || bad(candidate) != 0u || bad(candidate+vec2<i32>(0, 1)) != 0u {
                continue;
            }
            var score = 0.0;
            for(var t = 0u;t<10u;t++) {
                let offset = offsets[t];
                let difference = add(at(candidate+offset), -at(center+offset));
                let square = mul(difference, difference);
                score = fma(square, f32(1i-i32(bad(center+offset))), score);
            }
            if score<best {
                best = score;
                chosen = vec2<f32>(at(candidate), at(candidate+vec2<i32>(0, 1)));
            }
        }
    }
    return vec2<f32>(clamp(chosen.x, min(b, min(d, f)), max(0.0, max(b, max(d, f)))), clamp(chosen.y, min(g, min(i, k)), max(0.0, max(g, max(i, k)))));
}
@compute @workgroup_size(8, 8)
fn fppd(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= p.count.x || id.y >= p.count.y {
        return;
    }
    let point = p.grid.xy+id.xy*p.grid.zw;
    let position = point.y*p.shape.x+point.x;
    if p.shape.z == 0u {
        for(var channel = 0u;channel<3u;channel++) {
            output[position*3u+channel] = mul(source[position*3u+channel], gains[channel*p.count.x+id.x]);
        }
    }
    else {
        let values = pair(vec2<i32>(point), vec2<f32>(gains[id.x], gains[p.count.x+id.x]));
        output[position] = values.x;
        output[position+p.shape.x] = values.y;
    }
}
