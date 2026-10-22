struct Params {
    shape: vec4<u32>, controls: vec4<f32>
}
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage, read> gains: array<f32>;
@group(0) @binding(3) var<storage, read_write> destination: array<f32>;
@compute @workgroup_size(8, 8)
fn column_gain(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= p.shape.x || id.y >= p.shape.y {
        return;
    }
    let start = (id.y*p.shape.x+id.x)*6u;
    for(var channel = 0u;channel<6u;channel++) {
        let difference = fma(-1.0, p.controls.x, source[start+channel]);
        destination[start+channel] = fma(gains[id.x*6u+channel], difference, p.controls.x);
    }
}
