struct Params {
    shape: vec4<u32>, controls: vec4<f32>
}
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<storage, read> residual: array<f32>;
@group(0) @binding(2) var<storage, read> mask: array<u32>;
@group(0) @binding(3) var<uniform> params: Params;
@group(0) @binding(4) var<storage, read_write> detail: array<f32>;
@group(0) @binding(5) var<storage, read_write> rgb: array<f32>;
@group(0) @binding(6) var<storage, read_write> damped: array<f32>;

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

fn signed16(value: f32)->f32 {
    let word = u32(i32(value))&65535u;
    return f32(select(i32(word), i32(word)-65536, word >= 32768u));
}

fn corrected(i: u32)->vec3<f32> {
    let gain = residual[i];
    return max(vec3(0.), vec3(times(source[i*3u], gain), times(source[i*3u+1u], gain), times(source[i*3u+2u], gain)));
}

fn luma(c: vec3<f32>, factor: f32)->f32 {
    return times(fma(c.z, .114, fma(c.x, .299, times(c.y, .587))), factor);
}

@compute @workgroup_size(8, 8)

fn residual_sharp(@builtin(global_invocation_id) id: vec3<u32>) {
    let w = params.shape.x;
    let h = params.shape.y;
    if id.x >= w || id.y >= h {
        return;
    }
    let i = id.y*w+id.x;
    if id.x == 0u || id.y == 0u || id.x+1u == w || id.y+1u == h {
        detail[i] = 0.;
        return;
    }
    let scale = params.controls.x;
    let threshold = params.controls.y;
    let stronger = params.shape.z>1u;
    let factor = select(1., 3., stronger);
    let dead = select(times(scale, min(threshold, 40.)), min(times(scale, threshold), 320.), stronger);
    let absolute = select(divide(2000., scale), 6000., stronger);
    let relative = select(.2, .35, stronger);
    let original = corrected(i);
    let center = luma(original, factor);
    var sums = corrected(i-w-1u);
    for (var j = 1u;j<9u;j++) {
        let delta = (i32(j/3u)-1)*i32(w)+i32(j%3u)-1;
        let c = corrected(u32(i32(i)+delta));
        sums = vec3(plus(sums.x, c.x), plus(sums.y, c.y), plus(sums.z, c.z));
    }
    let mean = vec3(times(plus(sums.x, -original.x), .125), times(plus(sums.y, -original.y), .125), times(plus(sums.z, -original.z), .125));
    let difference = plus(center, -luma(mean, factor));
    var result = 0.;
    if abs(difference) >= dead {
        result = times(plus(abs(difference), -dead), sign(difference));
    }
    result = clamp(result, -absolute, absolute);
    let limit = times(relative, center);
    result = clamp(result, -limit, limit);
    result = clamp(result, -32760., 32760.);
    detail[i] = signed16(plus(times(result, 4.), .5));
}

@compute @workgroup_size(8, 8)

fn residual_finish(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.shape.x || id.y >= params.shape.y {
        return;
    }
    let i = id.y*params.shape.x+id.x;
    let r = residual[i];
    let adaptive = params.controls.z;
    var gain = 0.;
    if r>1. {
        gain = fma(plus(r, -1.), adaptive, 1.);
    }
    else {
        gain = divide(-r, plus(fma(r, adaptive, -r), -adaptive));
    }
    for (var c = 0u;c<3u;c++) {
        rgb[i*3u+c] = trunc(clamp(times(gain, source[i*3u+c]), 0., 65535.));
    }
    var sharp = detail[i];
    if params.shape.w != 0u && mask[i] != 0u {
        let base = fma(params.controls.w, .25, 1.25);
        let delta = times(base, times(f32(mask[i]), -.0039));
        // Native widens delta before adding one and multiplying signed detail.
        // The sum/product pair keeps an integer truncation boundary without Float64.
        let term = times(delta, sharp);
        let low = fma(delta, sharp, -term);
        let high = plus(sharp, term);
        let vb = plus(high, -sharp);
        let error = plus(plus(plus(sharp, -plus(high, -vb)), plus(term, -vb)), low);
        var integer = trunc(high);
        if high == integer {
            if high>0. && error<0. {
                integer -= 1.;
            }
            else if high<0. && error>0. {
                integer += 1.;
            }
        }
        sharp = signed16(integer);
    }
    damped[i] = sharp;
}
