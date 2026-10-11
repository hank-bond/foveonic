// Ordered, masked camera repair adapted with permission from Foveonic
// repair.rs at e47af35. Reads earlier repairs while the source stays immutable.
struct Params { source: vec4<u32> };
@group(0) @binding(1) var<storage, read> records: array<vec2<u32>>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read_write> pixels: array<f32>;
const offsets = array<vec2<i32>, 8>(vec2(-1,-1), vec2(0,-1), vec2(1,-1), vec2(-1,0), vec2(1,0), vec2(-1,1), vec2(0,1), vec2(1,1));
fn color(p: u32) -> vec3<f32> { return vec3(pixels[p*3u],pixels[p*3u+1u],pixels[p*3u+2u]); }
fn anchor(original: vec3<f32>, sum: vec3<f32>, c: u32, a: u32, denominator: f32) -> f32 {
    return original[a]+(sum[c]-sum[a])/denominator;
}
@compute @workgroup_size(1)
fn main() {
    // The host first copies the complete input into this separately owned
    // output. One invocation preserves read-after-write ordering across records.
    for (var index=0u; index<params.source.z; index++) {
        let record=records[index]; let p=record.x;
        let xy=vec2<i32>(i32(p%params.source.x),i32(p/params.source.x));
        let mask=record.y&255u;
        if xy.x<2 || xy.y<2 || xy.x>=i32(params.source.x)-1 || xy.y>=i32(params.source.y)-1 || mask==0u { continue; }
        var sum=vec3(0.0); var low=vec3(0.0); var high=vec3(0.0); var count=0.0;
        for (var n=0u; n<8u; n++) {
            if (mask&(1u<<n))==0u { continue; }
            let q=xy+offsets[n]; let v=color(u32(q.y)*params.source.x+u32(q.x));
            // Preserve the native all-zero color sentinels, including genuine
            // zero neighbors. Trim independently per channel after collecting.
            low=select(min(low,v),v,all(low==vec3(0.0)));
            high=select(max(high,v),v,all(high==vec3(0.0)));
            sum+=v; count+=1.0;
        }
        var denominator=count;
        if count>2.0 { sum-=high+low; denominator=count-2.0; }
        let original=color(p); var result=original;
        switch record.y>>12u {
            case 1u: { result.z=(anchor(original,sum,2u,0u,denominator)+anchor(original,sum,2u,1u,denominator))*0.5; }
            case 2u: { result.y=(anchor(original,sum,1u,0u,denominator)+anchor(original,sum,1u,2u,denominator))*0.5; }
            case 3u: { result.y=anchor(original,sum,1u,0u,denominator); result.z=anchor(original,sum,2u,0u,denominator); }
            case 4u: { result.x=(anchor(original,sum,0u,1u,denominator)+anchor(original,sum,0u,2u,denominator))*0.5; }
            case 5u: { result.x=anchor(original,sum,0u,1u,denominator); result.z=anchor(original,sum,2u,1u,denominator); }
            case 6u: { result.x=anchor(original,sum,0u,2u,denominator); result.y=anchor(original,sum,1u,2u,denominator); }
            case 7u: { result=sum/denominator; }
            default: {}
        }
        if all(abs(result)<=vec3(3.402823e38)) { pixels[p*3u]=result.x; pixels[p*3u+1u]=result.y; pixels[p*3u+2u]=result.z; }
    }
}
