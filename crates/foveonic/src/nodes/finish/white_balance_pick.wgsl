// Average a native 5x5 linear patch. Nominal white is a scale, not a clipping bound.
struct Params { shape: vec4<u32>, crop: vec4<u32>, xyz: array<vec4<f32>,3>, white: vec4<f32> }
@group(0) @binding(0) var<storage,read> src: array<f32>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage,read_write> dst: array<f32>;
@compute @workgroup_size(1)
fn white_balance_pick() {
    var sum = vec3(0.);
    var count = 0.;
    for (var y = -2; y <= 2; y++) {
        for (var x = -2; x <= 2; x++) {
            let q = vec2<i32>(p.shape.zw) + vec2(x,y);
            if (any(q < vec2<i32>(p.crop.xy)) || any(q >= vec2<i32>(p.crop.zw))) {
                continue;
            }
            let i = (u32(q.y) * p.shape.x + u32(q.x)) * 3u;
            let rgb = vec3(src[i],src[i+1u],src[i+2u]) / p.white.x;
            // Comparisons reject NaN and infinities without imposing a highlight ceiling.
            if (all(rgb > vec3(1e-5)) && all(rgb <= vec3(3.402823e38))) {
                sum += vec3(dot(p.xyz[0].xyz,rgb),dot(p.xyz[1].xyz,rgb),dot(p.xyz[2].xyz,rgb));
                count += 1.;
            }
        }
    }
    let mean = sum / max(count,1.);
    dst[0] = mean.x;
    dst[1] = mean.y;
    dst[2] = mean.z;
    dst[3] = count;
}
