// Adapted from Foveonic filtering and local_mean components at e47af35.
@group(0) @binding(0) var<storage, read> input_pixels: array<f32>;
@group(0) @binding(1) var<uniform> params: NoiseParams;
@group(0) @binding(2) var<storage, read_write> output_pixels: array<f32>;
@group(0) @binding(3) var<storage, read> edges: array<f32>;
@group(0) @binding(4) var<storage, read_write> scalar_work: array<f32>;
@group(0) @binding(5) var<storage, read_write> prefix: array<f32>;
@group(0) @binding(6) var<storage, read> table: array<f32>;
@group(0) @binding(7) var<storage, read_write> radii: array<u32>;

fn rgb(index: u32) -> vec3<f32> {
    let p = index * 3u;
    return vec3(input_pixels[p], input_pixels[p + 1u], input_pixels[p + 2u]);
}
fn put(index: u32, value: vec3<f32>) {
    let p = index * 3u;
    output_pixels[p] = value.x;
    output_pixels[p + 1u] = value.y;
    output_pixels[p + 2u] = value.z;
}
@compute @workgroup_size(8, 8)
fn tile_input(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * params.source.z * params.source.w { return; }
    let p = atlas_position(id.xy, params);
    let xy = full_position(p, params);
    var value = rgb(xy.y * params.source.x + xy.x);
    if any(xy < params.area.xy) || any(xy >= params.area.zw) { value = vec3(1000.); }
    put(tile_pixel(p, params), value);
}

@compute @workgroup_size(8, 8)
fn block_mean(@builtin(global_invocation_id) id: vec3<u32>) {
    let grid = (params.tile.xy + 31u) / 32u;
    if id.x >= grid.x || id.y >= grid.y * params.source.z * params.source.w { return; }
    let tile = id.y / grid.y;
    let row = id.y % grid.y;
    let first = vec2<i32>(vec2(id.x * 32u, row * 32u)) - 1;
    let last = min(first + 32, vec2<i32>(params.tile.xy));
    var sum = 0.;
    for (var y = max(0, first.y); y < last.y; y++) {
        for (var x = max(0, first.x); x < last.x; x++) {
            let v = rgb(tile_pixel(vec3<u32>(u32(x), u32(y), tile), params));
            sum += min(v.x, min(v.y, v.z));
        }
    }
    let nominal = min(vec2(32u), params.tile.xy - vec2(id.x, row) * 32u);
    scalar_work[id.y * grid.x + id.x] = sum / f32(nominal.x * nominal.y);
}
@compute @workgroup_size(8, 8)
fn radius_map(@builtin(global_invocation_id) id: vec3<u32>) {
    let grid = (params.tile.xy + 31u) / 32u;
    if id.x >= grid.x || id.y >= grid.y * params.source.z * params.source.w { return; }
    let tile = id.y / grid.y;
    let row = id.y % grid.y;
    var value = 1e30;
    for (var y = max(0, i32(row) - 1); y <= min(i32(grid.y) - 1, i32(row) + 1); y++) {
        for (var x = max(0, i32(id.x) - 1); x <= min(i32(grid.x) - 1, i32(id.x) + 1); x++) {
            value = min(value, scalar_work[(tile * grid.y + u32(y)) * grid.x + u32(x)]);
        }
    }
    let high = params.edge.w > 400.;
    var radius = select(2u, 3u, high);
    if value < 700. { radius = select(3u, 5u, high); }
    if value < 500. { radius = select(4u, 6u, high); }
    if value < 300. { radius = select(5u, 7u, high); }
    if value < 150. { radius = 9u; }
    radii[id.y * grid.x + id.x] = radius;
}

@compute @workgroup_size(8, 8)
fn bilateral(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * params.source.z * params.source.w { return; }
    let p = atlas_position(id.xy, params);
    let pixel = tile_pixel(p, params);
    let center = rgb(pixel);
    let half = params.tile.w / 2u;
    var filtered = vec3(0.);
    if p.x >= half && p.x < params.tile.x - half - 1u && p.y >= half && p.y < params.tile.y - half {
        let grid = (params.tile.xy + 31u) / 32u;
        let row = (p.y + 1u) / 32u;
        let column = (p.x + 1u) / 32u;
        var radius_index = (p.z * grid.y + row) * grid.x + column;
        // Preserve the reference's carried radius at the beginning of each row.
        if column == 0u && p.y != half && (p.y + 1u) % 32u != 0u {
            radius_index = (p.z * grid.y + (p.y / 32u)) * grid.x + (params.tile.x - half - 1u) / 32u;
        }
        let radius = i32(radii[radius_index]);
        let step = select(1, 2, radius > 3);
        let inverse = (2047. / (8. / params.filtering.x)) / variance(center, params);
        var sum = vec3(0.);
        var total = 0.;
        var horizontal_radius = radius;
        if i32(p.y) >= radius {
            for (var y = i32(p.y) - radius; y <= min(i32(params.tile.y) - 1, i32(p.y) + radius); y++) {
                if i32(p.x) >= horizontal_radius {
                    for (var x = i32(p.x) - horizontal_radius; x <= i32(p.x) + horizontal_radius; x += step) {
                        let sample = rgb((p.z * params.tile.y + u32(y)) * params.tile.x + u32(x));
                        let d = sample - center;
                        let distance = dot(d * d, inverse);
                        let index = u32(round(distance));
                        if index < 2048u {
                            let weight = table[index];
                            sum = fma(sample, vec3(weight), sum);
                            total += weight;
                        }
                    }
                }
                horizontal_radius = 2 * radius - (step - 1) - horizontal_radius;
            }
        }
        filtered = center;
        if total > 0. { filtered = sum / total; }
    }
    let border = clamp(p.xy, vec2(half), params.tile.xy - half - 1u);
    let original = rgb(tile_pixel(vec3(border, p.z), params));
    put(pixel, params.filtering.y * filtered + (1. - params.filtering.y) * original);
}

fn sample_at(p: vec3<u32>, delta: vec2<i32>) -> vec3<f32> {
    let xy = vec2<u32>(clamp(vec2<i32>(p.xy) + delta, vec2(0), vec2<i32>(params.tile.xy) - 1));
    return rgb(tile_pixel(vec3(xy, p.z), params)) + params.edge.y;
}
fn median_cross(p: vec3<u32>, delta: vec2<i32>) -> vec3<f32> {
    var values = array<vec3<f32>, 5>(sample_at(p, delta), sample_at(p, delta + vec2(-1, 0)), sample_at(p, delta + vec2(1, 0)), sample_at(p, delta + vec2(0, -1)), sample_at(p, delta + vec2(0, 1)));
    for (var i = 1u; i < 5u; i++) {
        for (var j = i; j > 0u; j--) {
            let low = min(values[j - 1u], values[j]);
            values[j] = max(values[j - 1u], values[j]);
            values[j - 1u] = low;
        }
    }
    return values[2];
}
fn gradient(p: vec3<u32>, offset: vec2<i32>) -> f32 {
    let center = median_cross(p, offset);
    let inverse = 1. / max(params.model[0].xyz + params.model[1].xyz * center, vec3(1e-12));
    let dx = (median_cross(p, offset + vec2(-1, 0)) - median_cross(p, offset + vec2(1, 0))) * inverse;
    let dy = (median_cross(p, offset + vec2(0, -1)) - median_cross(p, offset + vec2(0, 1))) * inverse;
    return sqrt((max(dx.x * dx.x, max(dx.y * dx.y, dx.z * dx.z)) + max(dy.x * dy.x, max(dy.y * dy.y, dy.z * dy.z))) * 0.5);
}
@compute @workgroup_size(8, 8)
fn edge_map(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * params.source.z * params.source.w { return; }
    let p = atlas_position(id.xy, params);
    var value = 0.;
    if p.x > 0u && p.x + 1u < params.tile.x && p.y >= 3u && p.y + 3u < params.tile.y {
        value = (gradient(p, vec2(0)) + gradient(p, vec2(-1, 0)) + gradient(p, vec2(1, 0)) + gradient(p, vec2(0, -1)) + gradient(p, vec2(0, 1))) * 0.2;
    }
    if p.x >= 9u && p.x + 9u < params.tile.x && p.y >= 8u && p.y + 8u < params.tile.y {
        let cap = 4. / params.edge.x;
        let index = u32(min(value, cap) * (2047. / cap));
        value = table[2048u + min(index, 2047u)];
    }
    scalar_work[tile_pixel(p, params)] = value;
}

fn line_pixel(line: u32, position: u32, vertical: bool) -> u32 {
    let lines = select(params.tile.y, params.tile.x, vertical);
    let tile = line / lines;
    let row = line % lines;
    let xy = select(vec2(position, row), vec2(row, position), vertical);
    return tile_pixel(vec3(xy, tile), params);
}
@compute @workgroup_size(64)
fn local_mean(@builtin(global_invocation_id) id: vec3<u32>) {
    let vertical = params.diffusion.w > 0.;
    let lines = select(params.tile.y, params.tile.x, vertical);
    let length = select(params.tile.x, params.tile.y, vertical);
    if id.x >= lines * params.source.z * params.source.w { return; }
    let row = id.x % lines;
    for (var x = 0u; x < length; x++) { let pixel = line_pixel(id.x, x, vertical); put(pixel, rgb(pixel)); }
    if row < 8u || row + 8u >= lines { return; }
    let first = 8u;
    let last = length - 9u;
    var sum = vec3(0.);
    var weight = 0.;
    for (var x = first; x <= last; x++) {
        let pixel = line_pixel(id.x, x, vertical);
        let e = select(edges[pixel], 0., x == first);
        sum = e * sum + rgb(pixel);
        weight = e * weight + 1.;
        let q = pixel * 3u;
        prefix[q] = sum.x; prefix[q + 1u] = sum.y; prefix[q + 2u] = sum.z;
        scalar_work[pixel] = weight;
    }
    sum = vec3(0.); weight = 0.;
    for (var x = i32(last); x >= i32(first); x--) {
        let pixel = line_pixel(id.x, u32(x), vertical);
        let e = select(edges[pixel], 0., u32(x) == last);
        sum = e * sum + rgb(pixel);
        weight = e * weight + 1.;
        let q = pixel * 3u;
        var value = (vec3(prefix[q], prefix[q + 1u], prefix[q + 2u]) + sum - rgb(pixel)) / (scalar_work[pixel] + weight - 1.);
        if u32(x) == first { value = sum / weight; }
        if u32(x) == last { value = vec3(prefix[q], prefix[q + 1u], prefix[q + 2u]) / scalar_work[pixel]; }
        put(pixel, value);
    }
}
