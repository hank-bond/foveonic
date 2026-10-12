// Native original-signal bad-red detection and main/JPEG/detected list order.
// Adapted with owner permission from Foveonic bad_reds.rs/repair_setup.rs at e47af35.
struct Params { source:vec4<u32>, selection:vec4<u32>, model:array<vec4<f32>,3> };
@group(0) @binding(0) var<storage,read> original:array<f32>;
@group(0) @binding(1) var<storage,read_write> records:array<vec2<u32>>;
@group(0) @binding(2) var<uniform> params:Params;
@group(0) @binding(3) var<storage,read_write> pixels:array<f32>;
fn bad_red(p:u32)->bool{
    let r=original[p*3u];let b=original[p*3u+2u];
    let rn=(params.model[0].x+r*params.model[1].x)+(r*r)*params.model[2].x;
    let bn=(params.model[0].z+b*params.model[1].z)+(b*b)*params.model[2].z;
    return max(r,0.0)-rn>bn+max(b,0.0);
}
fn new_mask(delta:vec2<i32>,original_mask:u32)->u32{
    var mask=255u;
    for(var n=0u;n<8u;n++){
        let q=delta+camera_offsets[n];var exclude=all(q==vec2(0));
        for(var k=0u;k<8u;k++){if all(q==camera_offsets[k]) && (original_mask&(1u<<k))==0u{exclude=true;}}
        if exclude{mask-=1u<<n;}
    }
    return mask;
}
@compute @workgroup_size(1)
fn main(){
    let w=params.source.x;let h=params.source.y;let base=params.selection.z;var count=0u;
    // Detector input never sees calibration, gain, or preceding repairs. Preserve
    // duplicate detections and native capacity-minus-one behavior across both lists.
    for(var index=0u;index<base;index++){
        let record=records[index];let xy=vec2<i32>(i32(record.x%w),i32(record.x/w));let mask=record.y&255u;var remaining=mask;
        if xy.x>=2 && xy.y>=2 && xy.x<i32(w)-1 && xy.y<i32(h)-1 && mask!=0u{
            for(var n=0u;n<8u;n++){
                if (mask&(1u<<n))==0u || count>=params.selection.y{continue;}
                let q=xy+camera_offsets[n];let p=u32(q.y)*w+u32(q.x);
                if bad_red(p){records[base+count]=vec2(p,0x4300u|new_mask(camera_offsets[n],mask));count++;remaining-=1u<<n;}
            }
        }
        // The JPEG list has no override masks. An exhausted all-zero main mask
        // retains its original, and native override indexing stops at one million.
        if index<params.source.z && index<1000000u && remaining!=0u{records[index].y=(record.y&0xff00u)|remaining;}
    }
    records[base+params.selection.y]=vec2(count,0u);
    for(var index=0u;index<base+count;index++){
        let record=records[index];
        if ((record.y>>8u)&15u)>=params.selection.x{camera_correct(record,w,h);}
    }
}
