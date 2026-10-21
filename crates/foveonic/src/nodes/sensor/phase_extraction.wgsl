struct Params { shape: vec4<u32>, calibration: vec4<f32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read_write> rgb: array<f32>;
@group(0) @binding(3) var<storage, read_write> blue: array<f32>;

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
    if params.shape.z != 0u {
        let offset = params.calibration.x;
        let scale = params.calibration.y;
        red = (red - offset) * scale;
        green = (green - offset) * scale;
        mean_blue = (mean_blue - offset) * scale;
        phases = min((phases - vec4<f32>(offset)) * scale, vec4<f32>(params.calibration.z));
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
