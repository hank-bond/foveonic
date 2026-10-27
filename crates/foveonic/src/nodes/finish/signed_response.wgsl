// Finish signed 16-bit code planes using exact integer LUT and Q12 arithmetic.
// Inputs are represented in f32 storage but contain integral signed code values.
// Tables: exponential, root, three tone channels, then linear response f32 bits.
// Match the signed 16-bit LUT layout and fixed-point contracts on the host.
const LUT_LEN: u32 = 65536u;
const LUT_MASK: i32 = 65535;
const SIGNED_CODE_MAX: i32 = 32767;
// Linear response tables cover one negative Q12 signal unit through signed maximum.
const LINEAR_CODE_MIN: f32 = -4096.;
// Match ResponseMode on the host. The ordinary photographic mode is zero.
const MODE_LINEAR_DETAIL: u32 = 1u;
const MODE_LINEAR_PHOTOGRAPHIC: u32 = 2u;
const DETAIL_FRACTION_BITS: u32 = 14u;
const SATURATION_FRACTION_BITS: u32 = 12u;
const SATURATION_ROUNDING: i32 = 2048;
struct Params {
    size: vec4<u32>,
    matrix: array<vec4<i32>, 3>,
    detail_q14: vec4<i32>,
}
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> source: array<f32>;
@group(0) @binding(2) var<storage, read> detail: array<f32>;
@group(0) @binding(4) var<storage, read> tables: array<i32>;
@group(0) @binding(5) var<storage, read_write> output_rgb: array<f32>;

fn index16(value: i32) -> u32 { return u32(value & LUT_MASK); }
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.size.x || id.y >= params.size.y { return; }
    let pixel = id.y * params.size.x + id.x;
    var addition = 0;
    if params.size.z != MODE_LINEAR_PHOTOGRAPHIC { addition = (i32(detail[pixel]) * params.detail_q14.x) >> DETAIL_FRACTION_BITS; }
    var rgb: vec3<i32>;
    for (var c = 0u; c < 3u; c++) {
        var value = i32(source[pixel * 3u + c]) + addition;
        if params.size.z == MODE_LINEAR_PHOTOGRAPHIC {
            value = i32(round(clamp(source[pixel * 3u + c], LINEAR_CODE_MIN, f32(SIGNED_CODE_MAX))));
        } else { value = min(tables[index16(value)], SIGNED_CODE_MAX); }
        if params.size.z == MODE_LINEAR_DETAIL {
            output_rgb[pixel * 3u + c] = f32(value);
            continue;
        }
        value = tables[LUT_LEN + index16(value)];
        rgb[c] = tables[(2u + c) * LUT_LEN + index16(value)];
    }
    if params.size.z == MODE_LINEAR_DETAIL { return; }
    for (var c = 0u; c < 3u; c++) {
        let m = params.matrix[c].xyz;
        let code = (m.x * rgb.x + m.y * rgb.y + m.z * rgb.z + SATURATION_ROUNDING) >> SATURATION_FRACTION_BITS;
        // The prepared response retains photographic rendering, while removing
        // the recipe-selected profile encoding. Keep these values until output.
        output_rgb[pixel * 3u + c] = bitcast<f32>(tables[5u * LUT_LEN + index16(code)]);
    }
}
