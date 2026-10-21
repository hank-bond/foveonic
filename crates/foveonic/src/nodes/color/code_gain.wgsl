struct Params { shape: vec4<u32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read_write> destination: array<f32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.shape.x || id.y >= params.shape.y { return; }
    let first = (id.y * params.shape.x + id.x) * 3u;
    let gain = i32(params.shape.z);
    for (var channel = 0u; channel < 3u; channel++) {
        let sample = i32(clamp(source[first + channel], -32768., 32767.));
        // Splitting Q12 keeps both products inside i32 even at maximum gain.
        // The signed shift rounds negative fractions toward negative infinity.
        let scaled = sample * (gain >> 12u) + ((sample * (gain & 4095)) >> 12u);
        destination[first + channel] = f32(clamp(scaled, -500, 24000));
    }
}
