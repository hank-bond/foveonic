// Range filtering and residual recovery operate in signed camera-code units.
struct Params {
    // Full width / height, then reduced width / height.
    shape: vec4<u32>,
    // Reduction factor, bilateral window, subsampling flag, tanh table length.
    support: vec4<u32>,
    // RGB constant noise variance and range-filter strength.
    noise_a: vec4<f32>,
    // RGB signal-dependent noise variance and edge strength.
    noise_b: vec4<f32>,
    // Mean decay, dark-neutralization low / high, diffusion step.
    guide: vec4<f32>,
    // Intensity-noise constant / linear terms, conduction threshold, residual strength.
    residual: vec4<f32>,
    // RGB luminance weights and residual-ratio epsilon.
    weights: vec4<f32>,
    // Rows of the camera-to-working transform, including output gain.
    matrix: array<vec4<f32>, 3>,
    // RGB color-correction amplitudes and reconstruction-enabled flag.
    channel_noise: vec4<f32>,
}
@group(0) @binding(0) var<storage, read> src: array<f32>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage, read> other: array<f32>;
@group(0) @binding(3) var<storage, read_write> dst: array<f32>;
@group(0) @binding(4) var<storage, read> tables: array<i32>;
@group(0) @binding(5) var<storage, read> black: array<f32>;

fn rgb(i: u32, stride: u32) -> vec3<f32> {
    let j = i * stride;
    return vec3(src[j], src[j + 1u], src[j + 2u]);
}

fn put(i: u32, v: vec3<f32>) {
    dst[4u * i] = v.x;
    dst[4u * i + 1u] = v.y;
    dst[4u * i + 2u] = v.z;
    dst[4u * i + 3u] = 0.;
}

// All image fringes have defined clamped support. The reference implementation
// leaves some resampling rows unwritten and copies cross-row top pixels.
fn index(x: i32, y: i32, w: u32, h: u32) -> u32 {
    return u32(clamp(y, 0, i32(h)-1)) * w + u32(clamp(x, 0, i32(w)-1));
}

fn luma(v: vec3<f32>) -> f32 {
    return fma(v.b, p.weights.b, fma(v.r, p.weights.r, v.g * p.weights.g));
}

fn small(id: vec3<u32>) -> bool {
    return id.x<p.shape.z && id.y<p.shape.w;
}

fn full(id: vec3<u32>) -> bool {
    return id.x<p.shape.x && id.y<p.shape.y;
}

// The eight-tap literals intentionally retain their recovered decimal rounding.
fn taps(k: u32, f: u32) -> f32 {
    if (f == 2u) {
        return array<f32, 4>(.125, .375, .375, .125)[k];
    }
    return array<f32, 8>(.0313, .0938, .1563, .2188, .2188, .1563, .0938, .0313)[k];
}

@compute @workgroup_size(8, 8)
fn pack(@builtin(global_invocation_id) id: vec3<u32>) {
    if (full(id)) {
        let i = id.y * p.shape.x + id.x;
        put(i, rgb(i, 3u));
    }
}

// Pair symmetric samples from the outside inward, retaining the recovered
// four / eight-tap Float32 coefficient literals and horizontal final-column rule.
fn reduced_column(x: u32, tap: u32) -> u32 {
    let f = p.support.x;
    let limit = p.shape.z * f;
    if (f == 4u && x + 1u == p.shape.z) {
        if (tap == 7u) {
            return limit-2u;
        }
        if (tap == 4u) {
            return limit-1u;
        }
    }
    return u32(clamp(i32(x * f + tap)-i32(f / 2u), 0, i32(limit)-1));
}

fn reduced_row(x: u32, y: i32) -> vec3<f32> {
    let f = p.support.x;
    let row = u32(clamp(y, 0, i32(p.shape.y)-1)) * p.shape.x;
    var value = vec3(0.);
    for (var k = 0u; k<f; k++) {
        let pair = rgb(row + reduced_column(x, k), 3u) + rgb(row + reduced_column(x, 2u * f-1u-k), 3u);
        let term = pair * taps(k, f);
        if (k == 0u) {
            value = term;
        } else {
            value += term;
        }
    }
    return value;
}

@compute @workgroup_size(8, 8)
fn reduce(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!small(id)) {
        return;
    }
    let f = p.support.x;
    let i = id.y * p.shape.z + id.x;
    if (f == 1u) {
        put(i, rgb(id.y * p.shape.x + id.x, 3u));
        return;
    }
    var value = vec3(0.);
    let origin = i32(id.y * f)-i32(f / 2u);
    for (var k = 0u; k<f; k++) {
        let pair = reduced_row(id.x, origin + i32(k)) + reduced_row(id.x, origin + i32(2u * f-1u-k));
        let term = pair * taps(k, f);
        if (k == 0u) {
            value = term;
        } else {
            value += term;
        }
    }
    put(i, value);
}

@compute @workgroup_size(1)
fn estimate_black() {
    dst[0] = 0.;
    dst[1] = 0.;
    dst[2] = 0.;
    dst[3] = 0.;
    let w = p.shape.z;
    let h = p.shape.w;
    let quantile_count = u32(f32((h-h / 10u) * (w-w / 10u)) * .000625);
    if (quantile_count <= 9u) {
        return;
    }
    let strength = p.channel_noise.x + p.channel_noise.y + .5 * p.channel_noise.z;
    let radius = min(200, i32(strength * 5.));
    if (radius<2) {
        return;
    }
    var histogram: array<u32, 399>;
    for (var y = h / 20u; y<h-h / 20u; y += 4u) {
        for (var x = w / 20u; x<w-w / 20u; x += 4u) {
            let v = rgb(y * w + x, 4u);
            let j = v.r + v.g + .5 * v.b + f32(radius)-1.;
            if (j >= 0. && j<f32(2 * radius-1)) {
                histogram[u32(j)]++;
            }
        }
    }
    var sum = histogram[0];
    var cutoff = 0;
    for (var j = 1; j<2 * radius-1; j++) {
        sum += histogram[u32(j)];
        if (sum<quantile_count) {
            cutoff = j;
        }
    }
    var total = vec3(0.);
    var count = 0u;
    for (var y = h / 20u; y<h-h / 20u; y += 4u) {
        for (var x = w / 20u; x<w-w / 20u; x += 4u) {
            let v = rgb(y * w + x, 4u);
            let value = v.r + v.g + .5 * v.b;
            if (value>f32(-radius) && value <= f32(i32(strength)-radius + cutoff + 1)) {
                total += v;
                count++;
            }
        }
    }
    if (count>200u) {
        let v = total / f32(count + 400u);
        dst[0] = v.r;
        dst[1] = v.g;
        dst[2] = v.b;
    }
}

@compute @workgroup_size(8, 8)
fn bilateral(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!small(id)) {
        return;
    }
    let i = id.y * p.shape.z + id.x;
    let center = rgb(i, 4u);
    let r = i32(p.support.y / 2u);
    // Preserve zero-initialized unwritten columns of the range-filter caller.
    if (i32(id.x)<r-1 || i32(id.x) >= i32(p.shape.z)-(r-1)) {
        put(i, vec3(0.));
        return;
    }
    let inverse = 1./max(p.noise_a.xyz + p.noise_b.xyz * abs(center), vec3(1e-12));
    var value = vec3(0.);
    var population = 0.;
    var reach = r;
    let step = select(1, 2, p.support.z != 0u);
    for (var dy = -r; dy <= r; dy++) {
        for (var dx = -reach; dx <= reach; dx += step) {
            let v = rgb(index(i32(id.x) + dx, i32(id.y) + dy, p.shape.z, p.shape.w), 4u);
            let d = v-center;
            let argument = dot((inverse * d), d);
            if (argument<16.) {
                let weight = exp(-f32(u32(argument * 100.)) * p.noise_a.w / 100.);
                value += weight * v;
                population += weight;
            }
        }
        reach = i32(p.support.y&~1u)-i32(p.support.z)-reach;
    }
    put(i, value / max(population, 1e-20));
}

@compute @workgroup_size(8, 8)
fn edge_guide(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!small(id)) {
        return;
    }
    let w = p.shape.z;
    let h = p.shape.w;
    let i = id.y * w + id.x;
    var g = 0.;
    if (id.x>0u && id.y>0u && id.x + 1u<w && id.y + 1u<h) {
        let c = rgb(i, 4u);
        let a = rgb(i-1u, 4u);
        let b = rgb(i + 1u, 4u);
        let up = rgb(i-w, 4u);
        let down = rgb(i + w, 4u);
        let inv = 1./max(p.noise_a.xyz + p.noise_b.xyz * c, vec3(1e-12));
        let dx = (a-b) * inv;
        let dy = (up-down) * inv;
        let one = 1.+tanh(-p.noise_b.w * sqrt((dot(dx, dx) + dot(dy, dy)) / 3.));
        let center = luma(c);
        let u = luma(up)-center;
        let d = luma(down)-center;
        let left = luma(a)-center;
        let right = luma(b)-center;
        let two = 2.*(1.-1./(1.+exp(-max(u * u + d * d, left * left + right * right))));
        g = fma(1.-two, two, one * two);
    }
    dst[4u * i] = g;
}

@compute @workgroup_size(8, 8)
fn neutralize(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!small(id)) {
        return;
    }
    let i = id.y * p.shape.z + id.x;
    let v = rgb(i, 4u);
    let maximum = max(max(v.r, v.g), v.b);
    let blend = clamp((maximum-p.guide.y) / (p.guide.z-p.guide.y), 0., 1.);
    put(i, fma(v, vec3(blend), vec3((1.-blend) * maximum)));
}

// Each invocation owns a complete line. Backward sums are stored before the forward pass.
fn mean_line(line: u32, vertical: bool) {
    let w = p.shape.z;
    let h = p.shape.w;
    let length = select(w, h, vertical);
    var accum = vec3(0.);
    var pop = 0.;
    for (var k = i32(length)-1; k >= 0; k--) {
        let i = select(line * w + u32(k), u32(k) * w + line, vertical);
        let d = p.guide.x * other[4u * i];
        accum = rgb(i, 4u) + d * accum;
        pop = 1.+d * pop;
        dst[4u * i] = accum.r;
        dst[4u * i + 1u] = accum.g;
        dst[4u * i + 2u] = accum.b;
        dst[4u * i + 3u] = pop;
    }
    accum = vec3(0.);
    pop = 0.;
    for (var k = 0u; k<length; k++) {
        let i = select(line * w + k, k * w + line, vertical);
        let d = p.guide.x * other[4u * i];
        let v = rgb(i, 4u);
        accum = v + d * accum;
        pop = 1.+d * pop;
        let reverse = vec3(dst[4u * i], dst[4u * i + 1u], dst[4u * i + 2u]);
        let reverse_pop = dst[4u * i + 3u];
        var result = (accum + reverse-v) / max(pop + reverse_pop-1., 1e-12);
        if (k == 0u) {
            result = reverse / reverse_pop;
        } else if (k + 1u == length) {
            result = accum / pop;
        }
        dst[4u * i] = result.r;
        dst[4u * i + 1u] = result.g;
        dst[4u * i + 2u] = result.b;
    }
}

@compute @workgroup_size(64)
fn mean_horizontal(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x<p.shape.w) {
        mean_line(id.x, false);
    }
}

@compute @workgroup_size(64)
fn mean_vertical(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x<p.shape.z) {
        mean_line(id.x, true);
    }
}

@compute @workgroup_size(8, 8)
fn expand(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!full(id)) {
        return;
    }
    let q = (vec2<f32>(id.xy) + .5) / f32(p.support.x)-.5;
    let a = vec2<i32>(floor(q));
    let t = fract(q);
    let w = p.shape.z;
    let h = p.shape.w;
    let top = mix(rgb(index(a.x, a.y, w, h), 4u), rgb(index(a.x + 1, a.y, w, h), 4u), t.x);
    let bottom = mix(rgb(index(a.x, a.y + 1, w, h), 4u), rgb(index(a.x + 1, a.y + 1, w, h), 4u), t.x);
    put(id.y * p.shape.x + id.x, mix(top, bottom, t.y));
}

@compute @workgroup_size(8, 8)
fn luminance(@builtin(global_invocation_id) id: vec3<u32>) {
    if (full(id)) {
        let i = id.y * p.shape.x + id.x;
        dst[4u * i] = luma(rgb(i, 3u));
    }
}

// Mono planes store weighted signed camera codes in the first of four lanes.
// Each pass reads immutable neighbors, giving deterministic GPU diffusion.
fn diffuse(id: vec3<u32>, constant: bool) {
    if (!full(id)) {
        return;
    }
    let w = p.shape.x;
    let h = p.shape.y;
    let i = id.y * w + id.x;
    let center = src[4u * i];
    let sigma = select(p.residual.x + p.residual.y * center, p.residual.z, constant);
    let inverse = 1./max(sigma * sigma, 1e-12);
    var update = 0.;
    let offsets = array<vec2<i32>, 4>(vec2(0, 1), vec2(0, -1), vec2(1, 0), vec2(-1, 0));
    for (var k = 0u; k<4u; k++) {
        let pos = vec2<i32>(id.xy) + offsets[k];
        let delta = src[4u * index(pos.x, pos.y, w, h)]-center;
        update += delta / (1.+delta * delta * inverse);
    }
    dst[4u * i] = fma(p.guide.w, update, center);
}

@compute @workgroup_size(8, 8)
fn diffuse_noise(@builtin(global_invocation_id) id: vec3<u32>) {
    diffuse(id, false);
}

@compute @workgroup_size(8, 8)
fn diffuse_constant(@builtin(global_invocation_id) id: vec3<u32>) {
    diffuse(id, true);
}

@compute @workgroup_size(8, 8)
fn extrema(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!full(id)) {
        return;
    }
    let i = id.y * p.shape.x + id.x;
    var maximum = -3.4e38;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            if (x != 0 || y != 0) {
                maximum = max(maximum, src[4u * index(i32(id.x) + x, i32(id.y) + y, p.shape.x, p.shape.y)]);
            }
        }
    }
    dst[4u * i] = min(src[4u * i], maximum);
}

fn lookup(c: u32, value: i32) -> i32 {
    let n = p.support.w;
    let limit = i32(n / 2u)-1;
    return tables[c * n + (u32(clamp(value, -limit, limit))&(n-1u))];
}

// Preserve floating matrix values until final signed16 narrowing. Integer tanh
// lookups remove chroma residuals; their luminance uses recovered R:G:B weights 1:2:1.
@compute @workgroup_size(8, 8)
fn finish(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!full(id)) {
        return;
    }
    let i = id.y * p.shape.x + id.x;
    let v = rgb(i, 4u);
    var ratio = 1.;
    if (p.channel_noise.w != 0.) {
        ratio = (p.weights.w + other[4u * i]) / (p.weights.w + max(0., luma(v)));
    }
    let neutral_black = (black[0] + 2.*black[1] + black[2]) * .25;
    let corrected = v-(vec3(black[0], black[1], black[2])-vec3(neutral_black));
    var value = vec3<f32>();
    for (var c = 0u; c<3u; c++) {
        let row = p.matrix[c];
        value[c] = fma(row.b, corrected.b, fma(row.r, corrected.r, row.g * corrected.g));
    }
    let integer = vec3<i32>(value);
    var first = vec3<i32>();
    for (var c = 0u; c<3u; c++) {
        first[c] = lookup(c, integer[c]);
    }
    let residual = integer-first;
    let mean = (residual.r + 2 * residual.g + residual.b)>>2;
    let amount = fma(ratio-1., p.residual.w, 1.);
    for (var c = 0u; c<3u; c++) {
        let out = (value[c]-f32(first[c])-f32(lookup(c, residual[c]-mean))) * amount;
        let code = i32(min(24000., out) + .5);
        dst[3u * i + c] = f32((code<<16)>>16);
    }
}
