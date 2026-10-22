struct Params {
    shape: vec4<u32>, grid: vec4<u32>, count: vec4<u32>
}
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> positions: array<u32>;
@group(0) @binding(2) var<storage, read_write> flags: array<atomic<u32>>;
@compute @workgroup_size(64)
fn fppd_mask(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x<p.count.z {
        atomicStore(&flags[positions[id.x]], 1u);
    }
}
