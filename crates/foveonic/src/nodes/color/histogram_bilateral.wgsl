struct Params { shape: vec4<u32>, controls: vec4<f32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read> ranges: array<vec4<u32>>;
@group(0) @binding(3) var<storage, read> weights: array<f32>;
@group(0) @binding(4) var<storage, read> adaptive: array<f32>;
@group(0) @binding(5) var<storage, read_write> destination: array<f32>;

const WORKGROUP_EDGE = 8u;
// Cache the common eleven-wide window. Larger accepted windows use storage.
const MAXIMUM_CACHED_RADIUS = 5u;
const MAXIMUM_TILE_EDGE = WORKGROUP_EDGE + 2u * MAXIMUM_CACHED_RADIUS;
// Keep the original value bits beside its bin, quantized once per tile.
var<workgroup> samples: array<vec2<u32>, MAXIMUM_TILE_EDGE * MAXIMUM_TILE_EDGE>;

fn bin(value: f32) -> u32 { return u32(clamp(i32(value), 0, 16383)) >> 4u; }

@compute @workgroup_size(8, 8)
fn histogram_bilateral(
    @builtin(global_invocation_id) id: vec3<u32>,
    @builtin(local_invocation_id) local: vec3<u32>,
    @builtin(local_invocation_index) lane: u32,
    @builtin(workgroup_id) group: vec3<u32>,
) {
    let width = params.shape.x;
    let height = params.shape.y;
    let radius = params.shape.z;
    let cached = radius <= MAXIMUM_CACHED_RADIUS;
    let tile_edge = WORKGROUP_EDGE + 2u * radius;
    if cached {
        let origin = vec2<i32>(group.xy * WORKGROUP_EDGE) - vec2(i32(radius));
        for (var i = lane; i < tile_edge * tile_edge; i += WORKGROUP_EDGE * WORKGROUP_EDGE) {
            let point = clamp(origin + vec2<i32>(i32(i % tile_edge), i32(i / tile_edge)), vec2(0), vec2<i32>(i32(width)-1, i32(height)-1));
            let value = source[u32(point.y) * width + u32(point.x)];
            samples[i] = vec2(bitcast<u32>(value), bin(value));
        }
        workgroupBarrier();
    }
    if id.x >= width || id.y >= height { return; }
    let index = id.y * width + id.x;
    let original = source[index];
    if id.x < radius || id.y < radius || id.x + radius >= width || id.y + radius >= height {
        destination[index] = original;
        return;
    }
    let range = ranges[bin(original)];
    var numerator = 0.0;
    var denominator = 0.0;
    for (var y = 0u; y <= 2u * radius; y++) {
        for (var x = 0u; x <= 2u * radius; x++) {
            var sample: vec2<u32>;
            if cached {
                sample = samples[(local.y + y) * tile_edge + local.x + x];
            } else {
                let value = source[(id.y + y - radius) * width + id.x + x - radius];
                sample = vec2(bitcast<u32>(value), bin(value));
            }
            let value = bitcast<f32>(sample.x);
            let neighbor = sample.y;
            if neighbor >= range.x && neighbor < range.x + range.z {
                let weight = weights[range.y + neighbor - range.x];
                numerator = fma(value, weight, numerator);
                denominator = fma(1.0, weight, denominator);
            }
        }
    }
    let filtered = numerator / denominator;
    let position = filtered * params.controls.x;
    // Separating the integer and fraction avoids rounding a value immediately
    // below a half upward during a Float32 addition of 0.5.
    let integer = floor(position);
    let rounded = integer + select(0.0, 1.0, position - integer >= 0.5);
    let blend_index = u32(clamp(rounded, 0.0, 16383.0));
    let alpha = adaptive[blend_index];
    destination[index] = fma(original, 1.0 - alpha, filtered * alpha);
}
