// Repair serial-matched locations only when they are outliers in the current shot.
struct Params {
    source: vec4<u32>, // Native extent, sparse count and clipped-only flag.
    area: vec4<u32>,
}
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> original: array<f32>;
@group(0) @binding(7) var<storage, read> records: array<u32>;
@group(0) @binding(8) var<storage, read_write> repaired: array<f32>;

const LAYERS: u32 = 3u;
const MINIMUM_OFFSET: f32 = 8.;
const SPATIAL_SIGMAS: f32 = 3.;
const MAD_TO_SIGMA: f32 = 1.4826;
const CLIPPED_CODE: f32 = 3900.;

fn reflect(value: i32, extent: u32) -> u32 {
    let period = 2 * (i32(extent) - 1);
    let folded = ((value % period) + period) % period;
    return u32(min(folded, period - folded));
}
fn defect_mask(pixel: u32) -> u32 {
    var low = 0u;
    var high = p.source.z;
    loop {
        if low >= high { break; }
        let middle = low + (high - low) / 2u;
        if records[middle * 4u] < pixel { low = middle + 1u; }
        else { high = middle; }
    }
    if low < p.source.z && records[low * 4u] == pixel { return records[low * 4u + 1u]; }
    return 0u;
}

@compute @workgroup_size(64)
fn calibrated_defect_repair(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= p.source.z { return; }
    let pixel = records[id.x * 4u];
    let mask = records[id.x * 4u + 1u];
    let triplet = vec3(original[pixel * LAYERS], original[pixel * LAYERS + 1u], original[pixel * LAYERS + 2u]);
    if p.source.w != 0u && !any(triplet >= vec3(CLIPPED_CODE)) { return; }
    let point = vec2(pixel % p.source.x, pixel / p.source.x);
    for (var c = 0u; c < LAYERS; c++) {
        if (mask & (1u << c)) == 0u { continue; }
        var values: array<f32, 24>;
        var n = 0u;
        for (var dy = -2; dy <= 2; dy++) {
            for (var dx = -2; dx <= 2; dx++) {
                if dx == 0 && dy == 0 { continue; }
                let x = reflect(i32(point.x) + dx, p.source.x);
                let y = reflect(i32(point.y) + dy, p.source.y);
                let neighbor = y * p.source.x + x;
                if (defect_mask(neighbor) & (1u << c)) != 0u { continue; }
                let value = original[neighbor * LAYERS + c];
                var j = n;
                loop {
                    if j == 0u { break; }
                    if values[j - 1u] <= value { break; }
                    values[j] = values[j - 1u];
                    j--;
                }
                values[j] = value;
                n++;
            }
        }
        if n == 0u { continue; }
        let median = (values[(n - 1u) / 2u] + values[n / 2u]) * 0.5;
        var deviations: array<f32, 24>;
        for (var i = 0u; i < n; i++) {
            let value = abs(values[i] - median);
            var j = i;
            loop {
                if j == 0u { break; }
                if deviations[j - 1u] <= value { break; }
                deviations[j] = deviations[j - 1u];
                j--;
            }
            deviations[j] = value;
        }
        let scatter = (deviations[(n - 1u) / 2u] + deviations[n / 2u]) * 0.5 * MAD_TO_SIGMA;
        if triplet[c] - median > max(MINIMUM_OFFSET, SPATIAL_SIGMAS * scatter) {
            repaired[pixel * LAYERS + c] = median;
        }
    }
}
