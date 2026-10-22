struct Params {
    shape: vec4<u32>, controls: vec4<f32>, noise: vec4<f32>, noise_low: vec4<f32>
}
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> source: array<f32>;
@group(0) @binding(2) var<storage, read> color: array<f32>;
@group(0) @binding(3) var<storage, read> spatial: array<vec2<f32>>;
@group(0) @binding(4) var<storage, read> blue: array<f32>;
@group(0) @binding(5) var<storage, read_write> vertical: array<vec2<f32>>;
@group(0) @binding(6) var<storage, read_write> signed_guide: array<f32>;
@group(0) @binding(7) var<storage, read_write> repaired: array<f32>;
@group(0) @binding(8) var<storage, read_write> rgb: array<f32>;
@group(0) @binding(9) var<storage, read_write> residual: array<f32>;

fn plus(a: f32, b: f32)->f32 {
    return fma(1., a, b);
}

fn times(a: f32, b: f32)->f32 {
    return fma(a, b, 0.);
}

fn divide(a: f32, b: f32)->f32 {
    let q = a/b;
    if a == 0. || b == 0. || q != q || abs(q)>3.402823e38 {
        return q;
    }
    return plus(q, fma(-q, b, a)/b);
}

fn product(a: f32, b: f32)->vec2<f32> {
    let high = times(a, b);
    return vec2(high, fma(a, b, -high));
}

fn add(a: vec2<f32>, b: vec2<f32>)->vec2<f32> {
    let sum = plus(a.x, b.x);
    let virtual_b = fma(-1., a.x, sum);
    let rem_a = fma(-1., fma(-1., virtual_b, sum), a.x);
    let rem_b = fma(-1., virtual_b, b.x);
    let error = plus(plus(rem_a, rem_b), plus(a.y, b.y));
    let high = plus(sum, error);
    return vec2(high, fma(-1., fma(-1., sum, high), error));
}

fn multiply(a: vec2<f32>, b: f32)->vec2<f32> {
    return add(product(a.x, b), product(a.y, b));
}
// The selected twofold Catmull-Rom phases are dyadic, with exact coefficients.

fn weights(coordinate: u32)->vec4<f32> {
    return select(vec4(-.0234375, .2265625, .8671875, -.0703125), vec4(-.0703125, .8671875, .2265625, -.0234375), (coordinate%2u) != 0u);
}

fn start(coordinate: u32)->i32 {
    return i32((coordinate+1u)/2u)-2;
}

fn signed16(value: f32)->f32 {
    let word = u32(i32(value))&65535u;
    return f32(select(i32(word), i32(word)-65536, word >= 32768u));
}

@compute @workgroup_size(8, 8)

fn blue_guide_vertical(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.shape.x || id.y >= params.shape.w {
        return;
    }
    let weights = weights(id.y);
    let first = start(id.y);
    for (var c = 0u;c<3u;c++) {
        var sum = vec2(0.);
        for (var k = 0u;k<4u;k++) {
            let y = u32(clamp(first+i32(k), 0, i32(params.shape.y)-1));
            sum = add(sum, product(source[(y*params.shape.x+id.x)*3u+c], weights[k]));
        }
        vertical[(id.y*params.shape.x+id.x)*3u+c] = sum;
    }
}

@compute @workgroup_size(8, 8)

fn blue_guide_horizontal(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.shape.z || id.y >= params.shape.w {
        return;
    }
    let weights = weights(id.x);
    let first = start(id.x);
    var channels = vec3(0.);
    for (var c = 0u;c<3u;c++) {
        var sum = vec2(0.);
        for (var k = 0u;k<4u;k++) {
            let x = u32(clamp(first+i32(k), 0, i32(params.shape.x)-1));
            sum = add(sum, multiply(vertical[(id.y*params.shape.x+x)*3u+c], weights[k]));
        }
        // Pair comparison retains the truncation boundary when the high word rounds to an integer.
        var value = clamp(sum.x, 0., 32767.);
        if value>0. && sum.x <= 32767. {
            let integer = trunc(value);
            value = integer;
            if sum.x == integer && sum.y<0. {
                value -= 1.;
            }
        }
        channels[c] = value;
    }
    let i = id.y*params.shape.z+id.x;
    let gain = divide(plus(max(0., blue[i]), 5.), plus(max(0., channels.z), 5.));
    for (var c = 0u;c<3u;c++) {
        signed_guide[i*3u+c] = signed16(plus(times(gain, channels[c]), .5));
    }
}

@compute @workgroup_size(8, 8)

fn blue_outliers(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.shape.z || id.y >= params.shape.w {
        return;
    }
    let i = id.y*params.shape.z+id.x;
    let center = blue[i];
    var candidate = center;
    if id.x>0u && id.y>0u && id.x+1u<params.shape.z && id.y+1u<params.shape.w {
        var low = 3.402823e38;
        var high = -3.402823e38;
        for (var dy = -1;dy <= 1;dy++) {
            for (var dx = -1;dx <= 1;dx++) {
                if dx != 0 || dy != 0 {
                    let value = blue[u32(i32(i)+dy*i32(params.shape.z)+dx)];
                    low = min(low, value);
                    high = max(high, value);
                }
            }
        }
        let extreme = clamp(center, low, high);
        let constant = vec2(params.noise.x, params.noise_low.x);
        let linear = multiply(vec2(params.noise.y, params.noise_low.y), abs(center));
        let quadratic = multiply(multiply(vec2(params.noise.z, params.noise_low.z), center), center);
        let variance = add(add(constant, linear), quadratic);
        let deviation = sqrt(plus(variance.x, variance.y));
        if abs(plus(center, -extreme))>deviation {
            candidate = extreme;
        }
    }
    repaired[i] = candidate;
}

@compute @workgroup_size(8, 8)

fn blue_guided_restore(@builtin(global_invocation_id) id: vec3<u32>) {
    let w = params.shape.z;
    let h = params.shape.w;
    if id.x >= w || id.y >= h {
        return;
    }
    let i = id.y*w+id.x;
    if id.x<5u || id.x+5u >= w || id.y<5u || id.y+5u >= h {
        for (var c = 0u;c<3u;c++) {
            rgb[i*3u+c] = max(0., signed_guide[i*3u+c]);
        }
        residual[i] = 1.;
        return;
    }
    let center = vec3(signed_guide[i*3u], signed_guide[i*3u+1u], signed_guide[i*3u+2u]);
    let maximum = max(center.x, max(center.y, center.z));
    var range_scale = 25.;
    if maximum>1000. {
        range_scale = divide(times(maximum, 25.), 1000.);
    }
    let x0 = i32(id.x/2u)-2;
    let y0 = i32(id.y/2u)-2;
    let parity = (id.y%2u)*25u+(id.x%2u)*50u;
    var total = 0.;
    var weighted_blue = 0.;
    var channels = vec3(0.);
    var j = 0u;
    for (var dy = 0;dy<5;dy++) {
        for (var dx = 0;dx<5;dx++) {
            let p = (u32(y0+dy)*params.shape.x+u32(x0+dx))*3u;
            let g = vec3(source[p], source[p+1u], source[p+2u]);
            let distance = plus(plus(abs(plus(g.x, -center.x)), abs(plus(g.y, -center.y))), abs(plus(g.z, -center.z)));
            let weight = times(divide(1., plus(divide(distance, range_scale), 1.)), spatial[parity+j].y);
            total = plus(total, weight);
            weighted_blue = fma(g.z, weight, weighted_blue);
            channels = vec3(plus(channels.x, times(color[p], weight)), plus(channels.y, times(color[p+1u], weight)), plus(channels.z, times(color[p+2u], weight)));
            j++;
        }
    }
    let mean_blue = divide(weighted_blue, total);
    var ratio = 1.;
    let b = blue[i];
    if mean_blue>0. {
        ratio = divide(plus(max(0., b), 5.), plus(mean_blue, 5.));
        if b >= 11000. && ratio<1. {
            let fade = clamp(divide(plus(b, -11000.), 3000.), 0., 1.);
            ratio = fma(plus(ratio, -1.), plus(1., -fade), 1.);
        }
    }
    let nearest = ((id.y/2u)*params.shape.x+id.x/2u)*3u;
    let red = source[nearest];
    let green = source[nearest+1u];
    let bguide = source[nearest+2u];
    let chroma = divide(fma(bguide, -2., plus(red, green)), max(.0001, bguide));
    let gate = plus(1., -divide(params.controls.x, plus(exp(fma(chroma, -4., 7.)), 1.)));
    if ratio >= 1. {
        ratio = fma(gate, plus(ratio, -1.), 1.);
    }
    else {
        ratio = divide(1., fma(gate, plus(divide(1., ratio), -1.), 1.));
    }
    residual[i] = clamp(ratio, .001, 1000.);
    for (var c = 0u;c<3u;c++) {
        rgb[i*3u+c] = trunc(clamp(plus(divide(channels[c], total), .5), 0., 65535.));
    }
}
