// Exact square minima reuse two scalar planes for overlapping centered windows.
@group(0) @binding(0) var<uniform> p: vec4<u32>;
@group(0) @binding(1) var<storage,read> pixels: array<vec4<u32>>;
@group(0) @binding(2) var<storage,read> previous: array<u32>;
@group(0) @binding(3) var<storage,read_write> output: array<u32>;

fn index_at(point: vec2<i32>) -> u32 {
    let bounded=vec2<u32>(clamp(point,vec2(0),vec2<i32>(p.xy)-vec2(1)));
    return bounded.y*p.x+bounded.x;
}

@compute @workgroup_size(8,8)
fn dehaze_minimum_prepare(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy>=p.xy) { return; }
    var value=65535u;
    for(var y=-1;y<=1;y++) { for(var x=-1;x<=1;x++) {
        let rgb=pixels[index_at(vec2<i32>(id.xy)+vec2(x,y)*i32(p.z))].xyz;
        value=min(value,min(rgb.x,min(rgb.y,rgb.z)));
    }}
    output[id.y*p.x+id.x]=value;
}

@compute @workgroup_size(8,8)
fn dehaze_minimum_expand(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy>=p.xy) { return; }
    var value=65535u;
    for(var y=-1;y<=1;y++) { for(var x=-1;x<=1;x++) {
        value=min(value,previous[index_at(vec2<i32>(id.xy)+vec2(x,y)*i32(p.z))]);
    }}
    output[id.y*p.x+id.x]=value;
}
