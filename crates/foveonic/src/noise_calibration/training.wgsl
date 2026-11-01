// Cap measurements retain line profiles, running moments and persistent outlier locations.
struct Params {
    source: vec4<u32>, // Native extent, capture count and profile stride.
    area: vec4<u32>,
}
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> original: array<f32>;
@group(0) @binding(2) var<storage, read_write> mean: array<f32>;
@group(0) @binding(3) var<storage, read_write> m2: array<f32>;
@group(0) @binding(4) var<storage, read> black: array<f32>;
@group(0) @binding(5) var<storage, read_write> profiles: array<f32>;
@group(0) @binding(6) var<storage, read_write> count: atomic<u32>;
@group(0) @binding(7) var<storage, read_write> records: array<u32>;

const LAYERS: u32 = 3u;
// Separate strong pixel outliers from the much larger fixed-pattern grain population.
const MINIMUM_OFFSET: f32 = 32.;
const DEVIATION_SIGMAS: f32 = 6.;
const MEAN_SIGMAS: f32 = 8.;
var<workgroup> sums: array<vec3<f32>, 64>;

fn load_original(pixel: u32) -> vec3<f32> {
    return vec3(original[pixel * LAYERS], original[pixel * LAYERS + 1u], original[pixel * LAYERS + 2u]);
}
fn reflect(value: i32, extent: u32) -> u32 {
    let period = 2 * (i32(extent) - 1);
    let folded = ((value % period) + period) % period;
    return u32(min(folded, period - folded));
}

@compute @workgroup_size(8, 8)
fn cap_accumulate(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= p.source.xy) { return; }
    let pixel = (id.y * p.source.x + id.x) * LAYERS;
    for (var c = 0u; c < LAYERS; c++) {
        let value = original[pixel + c] - black[c];
        if p.source.z == 1u {
            mean[pixel + c] = value;
            m2[pixel + c] = 0.;
        } else {
            let delta = value - mean[pixel + c];
            let next = mean[pixel + c] + delta / f32(p.source.z);
            m2[pixel + c] += delta * (value - next);
            mean[pixel + c] = next;
        }
    }
}

@compute @workgroup_size(64)
fn cap_profiles(@builtin(workgroup_id) group: vec3<u32>, @builtin(local_invocation_index) lane: u32) {
    let axis = group.y;
    let extent = p.area.zw - p.area.xy;
    let length = select(extent.y, extent.x, axis == 1u);
    let across = select(extent.x, extent.y, axis == 1u);
    if group.x >= length { return; }
    var value = vec3(0.);
    for (var i = lane; i < across; i += 64u) {
        let point = p.area.xy + select(vec2(i, group.x), vec2(group.x, i), axis == 1u);
        value += load_original(point.y * p.source.x + point.x);
    }
    sums[lane] = value;
    workgroupBarrier();
    for (var stride = 32u; stride > 0u; stride /= 2u) {
        if lane < stride { sums[lane] += sums[lane + stride]; }
        workgroupBarrier();
    }
    if lane == 0u {
        let result = sums[0] / f32(across) - vec3(black[0], black[1], black[2]);
        let first = (axis * p.source.w + group.x) * LAYERS;
        for (var c = 0u; c < LAYERS; c++) { profiles[first + c] = result[c]; }
    }
}

@compute @workgroup_size(8, 8)
fn cap_defects(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy < p.area.xy) || any(id.xy >= p.area.zw) { return; }
    let index = (id.y * p.source.x + id.x) * LAYERS;
    if all(vec3(mean[index], mean[index + 1u], mean[index + 2u]) < vec3(MINIMUM_OFFSET)) { return; }
    var mask = 0u;
    for (var c = 0u; c < LAYERS; c++) {
        var neighbors: array<f32, 24>;
        var n = 0u;
        for (var dy = -2; dy <= 2; dy++) {
            for (var dx = -2; dx <= 2; dx++) {
                if dx == 0 && dy == 0 { continue; }
                let x = p.area.x + reflect(i32(id.x - p.area.x) + dx, p.area.z - p.area.x);
                let y = p.area.y + reflect(i32(id.y - p.area.y) + dy, p.area.w - p.area.y);
                let value = mean[(y * p.source.x + x) * LAYERS + c];
                var j = n;
                loop {
                    if j == 0u { break; }
                    if neighbors[j - 1u] <= value { break; }
                    neighbors[j] = neighbors[j - 1u];
                    j--;
                }
                neighbors[j] = value;
                n++;
            }
        }
        let offset = mean[index + c] - (neighbors[11] + neighbors[12]) * 0.5;
        let deviation = sqrt(max(m2[index + c] / f32(max(p.source.z - 1u, 1u)), 0.));
        let error = deviation / sqrt(f32(p.source.z));
        if offset > max(MINIMUM_OFFSET, max(DEVIATION_SIGMAS * deviation, MEAN_SIGMAS * error)) {
            mask |= 1u << c;
        }
    }
    if mask != 0u {
        let record = atomicAdd(&count, 1u);
        if record < arrayLength(&records) / 4u {
            records[record * 4u] = id.y * p.source.x + id.x;
            records[record * 4u + 1u] = mask;
            records[record * 4u + 2u] = 0u;
            records[record * 4u + 3u] = 0u;
        }
    }
}
