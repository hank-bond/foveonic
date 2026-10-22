struct Params {
    shape: vec4<u32>, bounds: vec4<u32>, noise: vec4<f32>, controls: vec4<f32>
}
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read_write> output: array<f32>;
@group(0) @binding(3) var<storage, read_write> unowned: atomic<u32>;

fn plus(a: f32, b: f32)->f32 {
    return fma(1., a, b);
}

fn minus(a: f32, b: f32)->f32 {
    return fma(-1., b, a);
}

fn product(a: f32, b: f32)->f32 {
    return fma(a, b, 0.);
}

fn at(x: i32, y: i32)->f32 {
    return source[u32(y)*params.shape.x+u32(x)];
}

fn sigma(v: f32)->f32 {
    return sqrt(fma(product(v, v), params.noise.z, fma(abs(v), params.noise.y, params.noise.x)));
}

fn sort3(v: vec3<f32>)->vec3<f32> {
    let a = min(v.x, v.y);
    let b = max(v.x, v.y);
    return vec3(min(a, v.z), max(a, min(b, v.z)), max(b, v.z));
}

fn insert_min(v: vec3<f32>, sample: f32)->vec3<f32> {
    if sample <= v.x {
        return vec3(sample, v.x, v.y);
    }
    if sample<v.y {
        return vec3(v.x, sample, v.y);
    }
    return vec3(v.xy, min(v.z, sample));
}

fn insert_max(v: vec3<f32>, sample: f32)->vec3<f32> {
    if sample >= v.x {
        return vec3(sample, v.x, v.y);
    }
    if sample>v.y {
        return vec3(v.x, sample, v.y);
    }
    return vec3(v.xy, max(v.z, sample));
}

fn seventh(block: array<f32, 9>)->f32 {
    var top = vec3<f32>(-3.402823e38);
    for (var k = 0u;k<9u;k++) {
        top = insert_max(top, block[k]);
    }
    return top.z;
}

fn curvature(a: f32, b: f32, c: f32)->f32 {
    return minus(fma(a, 2., -b), c);
}

fn dark_pair(p: vec2<u32>) {
    let width = params.shape.x;
    let height = params.shape.y;
    if p.y<2u || p.y+2u >= height || p.y%2u != 0u || p.x<2u || p.x+1u >= width {
        return;
    }
    let x = i32(p.x);
    let y = i32(p.y);
    let a = at(x, y);
    let b = at(x, y+1);
    var low = 3.402823e38;
    for (var dy = -1;dy <= 2;dy++) {
        for (var dx = -1;dx <= 1;dx++) {
            if dx == 0 && (dy == 0 || dy == 1) {
                continue;
            }
            low = min(low, at(x+dx, y+dy));
        }
    }
    let sa = sigma(a);
    let sb = sigma(b);
    let da = minus(low, a);
    let db = minus(low, b);
    if !(da>product(sa, params.controls.x) && da>params.controls.y && db>params.controls.y && db>product(sb, params.controls.x) && abs(minus(a, b))<product(sa, 3.)) {
        return;
    }
    if x>4 && y>4 && p.y+5u <= height && p.x+5u <= width {
        // The source's enlarged windows can reach beyond the logical image.
        // Require ownership before reading that context instead of inventing pixels.
        if (p.y+6u)*width+p.x+5u >= params.shape.w {
            // Preserve the copied pair when its enlarged context is unowned.
            // Diagnostic callers can request an explicit error instead.
            if params.controls.z != 0.0 { atomicAdd(&unowned, 1u); }
            return;
        }
        var first = vec2<f32>(3.402823e38);
        var second = vec2<f32>(3.402823e38);
        for (var yy = y-5;yy <= y+6;yy++) {
            for (var xx = x-5;xx <= x+5;xx++) {
                if xx == x && (yy == y || yy == y+1) {
                    continue;
                }
                let v = at(xx, yy);
                if yy <= y+5 {
                    if v<first.x {
                        first = vec2(v, first.x);
                    }
                    else {
                        first.y = min(first.y, v);
                    }
                }
                if yy >= y-4 {
                    if v<second.x {
                        second = vec2(v, second.x);
                    }
                    else {
                        second.y = min(second.y, v);
                    }
                }
            }
        }
        if !(minus(first.y, a)>product(sa, params.controls.x) && minus(second.y, b)>product(sb, params.controls.x)) {
            return;
        }
    }
    var qa: array<f32, 9>;
    var qb: array<f32, 9>;
    var k = 0u;
    for (var dy = -1;dy <= 1;dy++) {
        for (var dx = -1;dx <= 1;dx++) {
            qa[k] = at(x+dx, y+dy);
            qb[k] = at(x+dx, y+dy+1);
            k++;
        }
    }
    let horizontal = abs(plus(curvature(at(x, y+2), at(x-1, y+2), at(x+1, y+2)), curvature(at(x, y-2), at(x-1, y-2), at(x+1, y-2))));
    let vertical = abs(plus(curvature(at(x-2, y), at(x-2, y-1), at(x-2, y+1)), curvature(at(x+2, y), at(x+2, y-1), at(x+2, y+1))));
    var aa = product(plus(at(x, y-2), at(x, y+2)), .5);
    var bb = product(plus(at(x, y-1), at(x, y+3)), .5);
    if horizontal<vertical {
        aa = product(plus(at(x-2, y), at(x+2, y)), .5);
        bb = product(plus(at(x-2, y+1), at(x+2, y+1)), .5);
    }
    output[p.y*width+p.x] = min(seventh(qa), aa);
    output[(p.y+1u)*width+p.x] = min(seventh(qb), bb);
}

fn general(p: vec2<u32>) {
    let bounds = params.bounds;
    let left = max(3u, bounds.x);
    let top = max(3u, bounds.y);
    let right = min(params.shape.x-3u, bounds.z);
    let bottom = min(params.shape.y-3u, bounds.w);
    if p.x<left || p.x>right || p.y<top || p.y>bottom {
        return;
    }
    let x = i32(p.x);
    let y = i32(p.y);
    let center = at(x, y);
    var low = 3.402823e38;
    var high = -3.402823e38;
    for (var dy = -1;dy <= 1;dy++) {
        for (var dx = -1;dx <= 1;dx++) {
            if dx == 0 && dy == 0 {
                continue;
            }
            var xx = x+dx;
            if dy != 0 {
                xx = clamp(xx, i32(bounds.x), i32(bounds.z));
            }
            let v = at(xx, y+dy);
            low = min(low, v);
            high = max(high, v);
        }
    }
    let brightness = min(low, center);
    if brightness >= 576. {
        low = max(low, -16383.);
        high = min(high, 16383.);
    }
    else {
        var far: array<f32, 24>;
        var k = 0u;
        for (var dx = -2;dx <= 2;dx++) {
            for (var dy = -2;dy <= 2;dy++) {
                if dx == 0 && dy == 0 {
                    continue;
                }
                far[k] = at(x+dx, y+dy);
                k++;
            }
        }
        var minima: vec3<f32>;
        var maxima: vec3<f32>;
        if brightness<288. {
            let mn = min(far[0], min(far[1], far[2]));
            let mx = max(far[0], max(far[1], far[2]));
            var seed_low = 0.;
            var seed_high = 0.;
            for (var i = 0u;i<3u;i++) {
                let v = far[i];
                if mn <= v && v<mx {
                    seed_low = v;
                }
                if mn<v && v <= mx {
                    seed_high = v;
                }
            }
            // Equal initial values leave the middle seed zero. Preserve its slot:
            // sorting these three values would repair a native network quirk.
            minima = vec3(mn, seed_low, mx);
            maxima = vec3(mx, seed_high, mn);
        }
        else {
            minima = sort3(vec3(far[0], far[1], far[2]));
            maxima = minima.zyx;
        }
        for (var i = 3u;i<24u;i++) {
            minima = insert_min(minima, far[i]);
            maxima = insert_max(maxima, far[i]);
        }
        if brightness<288. {
            low = max(low, minima.z);
            high = min(high, maxima.z);
        }
        else {
            low = max(low, minima.y);
            high = min(high, maxima.y);
        }
    }
    var variance = 0.;
    var boundary = center;
    if center>high {
        boundary = high;
    }
    else if center<low {
        boundary = low;
    }
    if center>high || center<low {
        let a = product(center, product(boundary, params.noise.z));
        let b = product(boundary, params.noise.y);
        variance = plus(params.noise.x, max(0., plus(a, b)));
    }
    let limit = product(params.controls.x, sqrt(variance));
    var value = center;
    if minus(low, center)>limit {
        value = low;
    }
    if minus(value, high)>limit {
        value = high;
    }
    output[p.y*params.shape.x+p.x] = value;
}

// The ordinary neighbor-range worker uses nonnegative brightness in its noise
// fit. Thresholds decide whether to replace; they do not soften the replacement.
fn neighbor_range(p: vec2<u32>) {
    if p.x < max(1u, params.bounds.x) || p.y < max(1u, params.bounds.y)
        || p.x > min(params.shape.x-2u, params.bounds.z)
        || p.y > min(params.shape.y-2u, params.bounds.w) { return; }
    let x = i32(p.x);
    let y = i32(p.y);
    let center = at(x,y);
    var low = 3.402823e38;
    var high = -3.402823e38;
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            if dx == 0 && dy == 0 { continue; }
            let v = at(x+dx,y+dy);
            low = min(low,v);
            high = max(high,v);
        }
    }
    let v = max(center,0.);
    let variance = plus(plus(params.noise.x, product(v,params.noise.y)), product(product(v,v),params.noise.z));
    let limit = product(params.controls.x,sqrt(variance));
    var result = center;
    if minus(center,high) > limit { result = high; }
    if minus(low,result) > limit { result = low; }
    output[p.y*params.shape.x+p.x] = result;
}

@compute @workgroup_size(8, 8)

fn blue_despeckle(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= params.shape.xy) {
        return;
    }
    if params.shape.z == 0u {
        dark_pair(id.xy);
    }
    else if params.shape.z == 1u {
        general(id.xy);
    }
    else {
        neighbor_range(id.xy);
    }
}
