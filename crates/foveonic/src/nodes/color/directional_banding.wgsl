// Native Fine correction followed by axis-coherent residual Coarse correction.
const FINE: u32 = 0u;
const BLOCK_SIZE: u32 = 40u;
// Preserve the accepted Fine block normalization in both residual passes.
const BLOCK_DIVISOR_OFFSET: f32 = 1.0;
// Pool along fixed sensor coordinates, allowing only slow band-amplitude drift.
const POOL_RADIUS: u32 = 640u;
const ALIGNMENT_SEPARATION: i32 = 160;
const PROFILE_HALF_TAPS: i32 = 4;
const PROFILE_SPACING_DIVISOR: u32 = 16u;
const ALIGNMENT_SHIFT_COUNT: u32 = 7u;
const ALIGNMENT_SHIFTS: array<i32, ALIGNMENT_SHIFT_COUNT> = array(0, 2, -2, 8, -8, 24, -24);
const DIAGONAL_ADVANTAGE_START: f32 = 0.15;
const DIAGONAL_ADVANTAGE_END: f32 = 0.45;
const ALIGNMENT_NOISE_FLOOR: f32 = 0.2;
const MAX_ALIGNMENT_ERROR: f32 = 1e30;
const MIN_POOL_WEIGHT: f32 = 0.0001;
const RANGE_LAST: f32 = 2047.0;
const RANGE_CUTOFF: f32 = 2047.5;
const RANGE_FACTOR: f32 = 8.0;
const RELATIVE_LIMIT: f32 = 0.06;
const NOISE_SIGMAS: f32 = 2.0;
const CODE_MAX: f32 = 32766.0;

struct BandParams {
    active_area: vec4<u32>,
    width: u32, axis: u32, kind: u32, reflect_samples: u32,
    strength: f32, padding0: u32, padding1: u32, padding2: u32,
}
@group(0) @binding(0) var<storage, read> input_pixels: array<f32>;
@group(0) @binding(1) var<uniform> params: NoiseParams;
@group(0) @binding(2) var<storage, read_write> smoothed: array<f32>;
@group(0) @binding(3) var<storage, read_write> estimates: array<f32>;
@group(0) @binding(4) var<storage, read_write> output_values: array<f32>;
@group(0) @binding(5) var<storage, read> table: array<f32>;
@group(0) @binding(6) var<uniform> band: BandParams;

fn load_input(index: u32) -> vec3<f32> {
    return vec3(input_pixels[index*3u], input_pixels[index*3u+1u], input_pixels[index*3u+2u]);
}

fn sensor_position(point: vec3<u32>) -> vec2<i32> {
    let origin = native_tile(point.z, params)*(params.source.xy/params.source.zw);
    return vec2<i32>(origin+point.xy)-i32(params.tile.z);
}

fn reflect_coordinate(value: i32, first: u32, last: u32) -> u32 {
    if value >= i32(first) && value<i32(last) {
        return u32(value);
    }
    let length = i32(last-first)-1;
    let period = 2*length;
    let shifted = value-i32(first);
    let wrapped = ((shifted%period)+period)%period;
    return first+u32(min(wrapped, period-wrapped));
}

fn measurement_index(point: vec3<u32>) -> u32 {
    if band.reflect_samples == 0u {
        return tile_pixel(point, params);
    }
    let global = sensor_position(point);
    let reflected = vec2(
    reflect_coordinate(global.x, band.active_area.x, band.active_area.z),
    reflect_coordinate(global.y, band.active_area.y, band.active_area.w));
    let origin = native_tile(point.z, params)*(params.source.xy/params.source.zw);
    let local = vec2<i32>(reflected)-vec2<i32>(origin)+i32(params.tile.z);
    return tile_pixel(vec3(vec2<u32>(local), point.z), params);
}

fn directional_point(direction: u32, other: u32, tile: u32) -> vec3<u32> {
    if band.axis == 0u {
        return vec3(direction, other, tile);
    }
    return vec3(other, direction, tile);
}

fn measurement_bounds(tile: u32) -> vec2<u32> {
    let perpendicular = 1u-band.axis;
    let index = native_tile(tile, params)[perpendicular];
    let start = select(0u, params.column_area[perpendicular], index == 0u);
    let end = select(params.tile[perpendicular],
    params.tile[perpendicular]-(params.source[perpendicular]-params.column_area[perpendicular+2u]),
    index+1u == params.source[perpendicular+2u]);
    return vec2(start, end);
}

fn block_capacity() -> u32 {
    let length = params.tile[1u-band.axis];
    return select((length+BLOCK_SIZE-1u)/BLOCK_SIZE+1u, length/BLOCK_SIZE, band.kind == FINE);
}

fn block_count(bounds: vec2<u32>) -> u32 {
    let count = (bounds.y-bounds.x)/BLOCK_SIZE;
    return select(max(1u, count), count, band.kind == FINE);
}

fn estimate_index(direction: u32, block: u32, tile: u32) -> u32 {
    return ((tile*block_capacity()+block)*params.tile[band.axis]+direction)*3u;
}

fn load_estimate(direction: u32, block: u32, tile: u32) -> vec3<f32> {
    let index = estimate_index(direction, block, tile);
    return vec3(estimates[index], estimates[index+1u], estimates[index+2u]);
}

fn block_center(block: u32, bounds: vec2<u32>) -> f32 {
    let first = bounds.x+block*BLOCK_SIZE;
    let last = select(first+BLOCK_SIZE, bounds.y, block+1u == block_count(bounds));
    return 0.5*f32(first+last-1u);
}

@compute @workgroup_size(8, 8)

fn neighborhoods(@builtin(global_invocation_id)id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y*tile_count(params) {
        return;
    }
    let point = atlas_position(id.xy, params);
    let index = tile_pixel(point, params);
    let direction = point[band.axis];
    let half = band.width/2u;
    var result = vec3(0.0);
    // Preserve the reviewed terminal halo, including its last unused smooth pixel.
    if direction >= half && direction<params.tile[band.axis]-half-1u {
        let center = load_input(measurement_index(point));
        let scales = (RANGE_LAST/(RANGE_FACTOR/params.column.x))/variance(center, params);
        var sum = vec3(0.0);
        var total = 0.0;
        for (var offset = direction-half;offset <= direction+half;offset++) {
            var neighbor = point;
            neighbor[band.axis] = offset;
            let sample = load_input(measurement_index(neighbor));
            let difference = sample-center;
            let distance = fma(difference.z*difference.z, scales.z,
            fma(difference.x*difference.x, scales.x, scales.y*(difference.y*difference.y)));
            if distance<RANGE_CUTOFF {
                let weight = table[u32(floor(max(distance, 0.0)+0.5))];
                total += weight;
                sum = fma(sample, vec3(weight), sum);
            }
        }
        result = center;
        if total != 0.0 {
            result = sum/total;
        }
    }
    smoothed[index*3u] = result.x;
    smoothed[index*3u+1u] = result.y;
    smoothed[index*3u+2u] = result.z;
}

@compute @workgroup_size(64)

fn block_deltas(@builtin(global_invocation_id)id: vec3<u32>) {
    let blocks = block_capacity();
    let tile = id.y/blocks;
    let block = id.y%blocks;
    if id.x >= params.tile[band.axis] || tile >= tile_count(params) {
        return;
    }
    let bounds = measurement_bounds(tile);
    let count = block_count(bounds);
    var delta = vec3(0.0);
    if block<count {
        let first = bounds.x+block*BLOCK_SIZE;
        let last = select(first+BLOCK_SIZE, bounds.y, block+1u == count);
        var original = vec3(0.0);
        var filtered = vec3(0.0);
        for (var other = first;other<last;other++) {
            let point = directional_point(id.x, other, tile);
            original += load_input(measurement_index(point));
            let index = tile_pixel(point, params)*3u;
            filtered += vec3(smoothed[index], smoothed[index+1u], smoothed[index+2u]);
        }
        let divisor = f32(last-first)+BLOCK_DIVISOR_OFFSET;
        delta = original/divisor-filtered/divisor;
    }
    let index = estimate_index(id.x, block, tile);
    estimates[index] = delta.x;
    estimates[index+1u] = delta.y;
    estimates[index+2u] = delta.z;
}
// Sample the compact raw estimate table in global sensor coordinates. Neighbor
// tiles contribute their core once. Fitting requires the complete native atlas.

fn global_estimate(direction: i32, along: i32) -> vec3<f32> {
    let perpendicular = 1u-band.axis;
    var xy = vec2<u32>(0u);
    xy[band.axis] = u32(clamp(direction, i32(band.active_area[band.axis]), i32(band.active_area[band.axis+2u])-1));
    xy[perpendicular] = u32(clamp(along, i32(band.active_area[perpendicular]), i32(band.active_area[perpendicular+2u])-1));
    let core = params.source.xy/params.source.zw;
    let native = xy/core;
    let local = xy-native*core+vec2(params.tile.z);
    let tile = (native.y-params.window.y)*params.window.z+native.x-params.window.x;
    let bounds = measurement_bounds(tile);
    return interpolated_estimate(local[band.axis], local[perpendicular], tile, bounds);
}

fn pooled_axis_offset(direction: i32, along: i32) -> vec3<f32> {
    var total = vec3(0.0);
    var weight = 0.0;
    // Equal global sample positions on either side of tile edges avoid a
    // separate fit per tile. A triangular window permits slow amplitude drift.
    let center = (along/i32(BLOCK_SIZE))*i32(BLOCK_SIZE)+i32(BLOCK_SIZE/2u);
    let radius = i32(POOL_RADIUS/BLOCK_SIZE);
    for (var k = -radius;k <= radius;k++) {
        let location = center+k*i32(BLOCK_SIZE);
        if location<i32(band.active_area[1u-band.axis]) || location >= i32(band.active_area[3u-band.axis]) {
            continue;
        }
        let w = max(0.0, 1.0-abs(f32(location-along))/f32(POOL_RADIUS+BLOCK_SIZE));
        total += global_estimate(direction, location)*w;
        weight += w;
    }
    return total/max(weight, MIN_POOL_WEIGHT);
}

fn axis_alignment_confidence(direction: i32, along: i32) -> f32 {
    let distance = ALIGNMENT_SEPARATION;
    let perpendicular = 1u-band.axis;
    // A slope must improve both separated directions to count as a feature.
    // Test several slopes over a short profile, rather than comparing one noisy
    // center pixel with its best accidental neighbor.
    var fixed_error = 0.0;
    var best_error = MAX_ALIGNMENT_ERROR;

    let spacing = max(1, i32(band.width/PROFILE_SPACING_DIVISOR));
    for (var slope = 0u;slope<ALIGNMENT_SHIFT_COUNT;slope++) {
        var error = 0.0;
        var count = 0.0;
        for (var side = -1;side <= 1;side += 2) {
            let location = along+side*distance;
            if location<i32(band.active_area[perpendicular]) || location >= i32(band.active_area[perpendicular+2u]) {
                continue;
            }
            for (var tap = -PROFILE_HALF_TAPS;tap <= PROFILE_HALF_TAPS;tap++) {
                let here = global_estimate(direction+tap*spacing, along);
                let there = global_estimate(direction+tap*spacing+side*ALIGNMENT_SHIFTS[slope], location);
                let difference = here-there;
                error += dot(difference, difference);
                count += 3.0;
            }
        }
        error /= max(1.0, count);
        if slope == 0u {
            fixed_error = error;
        }
        best_error = min(best_error, error);
    }
    let diagonal_advantage = (fixed_error-best_error)/(fixed_error+best_error+ALIGNMENT_NOISE_FLOOR);
    return 1.0-smoothstep(DIAGONAL_ADVANTAGE_START, DIAGONAL_ADVANTAGE_END, diagonal_advantage);
}

fn coarse_offset(direction: u32, block: u32, tile: u32) -> vec3<f32> {
    let center = u32(round(block_center(block, measurement_bounds(tile))));
    let global = sensor_position(directional_point(direction, center, tile));
    let d = global[band.axis];
    let a = global[1u-band.axis];
    return pooled_axis_offset(d, a)*axis_alignment_confidence(d, a);
}

@compute @workgroup_size(64)

fn fit_offsets(@builtin(global_invocation_id)id: vec3<u32>) {
    let tile = id.y/block_capacity();
    let block = id.y%block_capacity();
    if id.x >= params.tile[band.axis] || tile >= tile_count(params) {
        return;
    }
    var value = vec3(0.0);
    if block<block_count(measurement_bounds(tile)) {
        value = coarse_offset(id.x, block, tile);
    }
    let index = estimate_index(id.x, block, tile);
    output_values[index] = value.x;
    output_values[index+1u] = value.y;
    output_values[index+2u] = value.z;
}

fn interpolated_estimate(direction: u32, other: u32, tile: u32, bounds: vec2<u32>) -> vec3<f32> {
    let count = block_count(bounds);
    if count == 1u {
        return load_estimate(direction, 0u, tile);
    }
    let position = (f32(other)-block_center(0u, bounds))/f32(BLOCK_SIZE);
    let left = u32(clamp(floor(position), 0.0, f32(count-2u)));
    let right = min(left+1u, count-1u);
    let weight = clamp((f32(other)-block_center(left, bounds))/max(1.0, block_center(right, bounds)-block_center(left, bounds)), 0.0, 1.0);
    return mix(load_estimate(direction, left, tile), load_estimate(direction, right, tile), weight);
}

@compute @workgroup_size(8, 8)

fn correct(@builtin(global_invocation_id)id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y*tile_count(params) {
        return;
    }
    let point = atlas_position(id.xy, params);
    let index = tile_pixel(point, params);
    var value = load_input(index);
    let bounds = measurement_bounds(point.z);
    let count = block_count(bounds);
    let direction = point[band.axis];
    let other = point[1u-band.axis];
    let global = sensor_position(point);
    let in_active = all(global >= vec2<i32>(band.active_area.xy)) && all(global<vec2<i32>(band.active_area.zw));
    let half = band.width/2u;
    if count>0u && other >= bounds.x && other<bounds.y && direction >= half && direction<params.tile[band.axis]-half {
        let limit = max(abs(value*RELATIVE_LIMIT), NOISE_SIGMAS*sqrt(variance(value, params)/f32(BLOCK_SIZE)));
        if band.kind == FINE && (band.reflect_samples == 0u || in_active) {
            let block = min((other-bounds.x)/BLOCK_SIZE, count-1u);
            let delta = load_estimate(direction, block, point.z);
            let accepted_delta = select(vec3(0.0), delta, limit>abs(delta));
            let unit_corrected = clamp(value-accepted_delta, vec3(0.0), vec3(CODE_MAX));
            // Keep the reviewed unit floor. Extra strength scales only the band offset.
            if band.strength == 1.0 {
                value = unit_corrected;
            } else if band.strength>1.0 {
                value = unit_corrected-(band.strength-1.0)*accepted_delta;
            } else {
                value = mix(value, unit_corrected, band.strength);
            }
        }
        if band.kind != FINE && in_active && direction<params.tile[band.axis]-half-1u {
            let delta = interpolated_estimate(direction, other, point.z, bounds);
            if all(limit>abs(delta)) {
                value -= delta*band.strength;
            }
        }
    }
    output_values[index*3u] = value.x;
    output_values[index*3u+1u] = value.y;
    output_values[index*3u+2u] = value.z;
}
