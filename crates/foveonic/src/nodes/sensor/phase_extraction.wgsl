const CHANNEL_GAIN_CALIBRATION = 2u;
struct Params { shape: vec4<u32>, calibration: vec4<f32>, gain_high: vec4<f32>, gain_low: vec4<f32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read_write> rgb: array<f32>;
@group(0) @binding(3) var<storage, read_write> blue: array<f32>;

fn channel_code(value: f32, channel: u32) -> f32 {
    let high = fma(value, params.gain_high[channel], 0.);
    let low = fma(value, params.gain_low[channel], 0.);
    let error = fma(1., fma(value, params.gain_high[channel], -high), low);
    return min(fma(1., high, error), params.calibration.z);
}

@compute @workgroup_size(8, 8)
fn phase_extraction(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = params.shape.x;
    if id.x >= width || id.y >= params.shape.y { return; }
    let cell = id.y * width + id.x;
    let packed = cell * 6u;
    let output = cell * 3u;
    let b00 = source[packed + 2u];
    let b01 = source[packed + 3u];
    let b10 = source[packed + 4u];
    let b11 = source[packed + 5u];
    var red = source[packed];
    var green = source[packed + 1u];
    var mean_blue = (((b00 + b01) + b10) + b11) * 0.25;
    var phases = vec4<f32>(b00, b01, b10, b11);
    if params.shape.z == CHANNEL_GAIN_CALIBRATION {
        // This overload scales each blue phase before summing its mean.
        mean_blue = (((b00 * 0.25 + b01 * 0.25) + b10 * 0.25) + b11 * 0.25);
    }
    if params.shape.z != 0u {
        let offset = params.calibration.x;
        let scale = params.calibration.y;
        red = (red - offset) * scale;
        green = (green - offset) * scale;
        mean_blue = (mean_blue - offset) * scale;
        phases = (phases - vec4<f32>(offset)) * scale;
        if params.shape.z == CHANNEL_GAIN_CALIBRATION {
            red = channel_code(red, 0u);
            green = channel_code(green, 1u);
            mean_blue = channel_code(mean_blue, 2u);
            for (var phase = 0u; phase < 4u; phase++) {
                phases[phase] = channel_code(phases[phase], 2u);
            }
        } else {
            phases = min(phases, vec4<f32>(params.calibration.z));
        }
    }
    rgb[output] = red;
    rgb[output + 1u] = green;
    rgb[output + 2u] = mean_blue;
    let top = (id.y * 2u) * (width * 2u) + id.x * 2u;
    blue[top] = phases.x;
    blue[top + 1u] = phases.y;
    blue[top + width * 2u] = phases.z;
    blue[top + width * 2u + 1u] = phases.w;
}
