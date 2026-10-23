// Shared highlight storage and camera-matrix arithmetic.
// Work contains RGB (0..3), green/clipped/low-camera markers (3..6),
// directional markers (6..7), horizontal RGB (7..10), and vertical RGB (10..13).
const CHANNEL_EXTENSION=1u;
const INVERSE_CAMERA_MATRIX=9u;
const GREEN_MARKER=3u;
const CLIPPED_MARKER=4u;
const LOW_CAMERA_MARKER=5u;
const DIRECTIONAL_MARKER=6u;
const HORIZONTAL_RGB=7u;
const VERTICAL_RGB=10u;
struct Params {
    shape: vec4<u32>, rectangle: vec4<u32>, gains: vec4<f32>,
    thresholds: vec4<f32>, neutralization: vec4<f32>, ceilings: vec4<f32>
}
@group(0) @binding(0) var<storage,read> pixels:array<f32>;
@group(0) @binding(1) var<uniform> p:Params;
@group(0) @binding(2) var<storage,read> matrix:array<vec2<f32>>;
@group(0) @binding(3) var<storage,read_write> work:array<f32>;
@group(0) @binding(5) var<storage,read_write> green_mask:array<u32>;
fn load_rgb(plane:u32,index:u32)->vec3<f32>{let i=plane*p.shape.z+index*3u;return vec3(work[i],work[i+1u],work[i+2u]);}
fn store_rgb(plane:u32,index:u32,value:vec3<f32>){let i=plane*p.shape.z+index*3u;work[i]=value.x;work[i+1u]=value.y;work[i+2u]=value.z;}
fn finite(value:vec3<f32>)->vec3<f32>{return select(vec3(0.),value,(value==value)&(abs(value)<=vec3(3.402823466e38)));}
fn product(a: f32, b: f32)->vec2<f32> {
    let high=fma(a, b, 0.);
    return vec2(high, fma(a, b, -high));
}
fn add_pair(a: vec2<f32>, b: vec2<f32>)->vec2<f32> {
    let sum=fma(1., a.x, b.x);
    let vb=fma(-1., a.x, sum);
    let ra=fma(-1., fma(-1., vb, sum), a.x);
    let rb=fma(-1., vb, b.x);
    let error=fma(1., fma(1., ra, rb), fma(1., a.y, b.y));
    let high=fma(1., sum, error);
    return vec2(high, fma(-1., fma(-1., sum, high), error));
}
fn multiply_pair(a: vec2<f32>, b: f32)->vec2<f32> {
    return add_pair(product(a.x, b), product(a.y, b));
}
fn color(v: vec3<f32>, base: u32)->vec3<f32> {
    var result: vec3<f32>;
    for(var c=0u;c<3u;c++) {
        let row=base+c*3u;
        let first=add_pair(multiply_pair(matrix[row+1u], v.y), multiply_pair(matrix[row], v.x));
        let total=add_pair(first, multiply_pair(matrix[row+2u], v.z));
        result[c]=fma(1., total.x, total.y);
    }
    return result;
}
