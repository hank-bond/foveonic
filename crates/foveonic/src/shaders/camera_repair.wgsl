// Native original-signal bad-red detection and main/JPEG/detected list order.
// Adapted with owner permission from Foveonic bad_reds.rs/repair_setup.rs at e47af35.
struct Params { source:vec4<u32>, selection:vec4<u32>, model:array<vec4<f32>,3> };
@group(0) @binding(0) var<storage,read> original:array<f32>;
@group(0) @binding(1) var<storage,read_write> records:array<vec2<u32>>;
@group(0) @binding(2) var<uniform> params:Params;
@group(0) @binding(3) var<storage,read_write> pixels:array<f32>;
@group(0) @binding(4) var<storage,read_write> detections:array<u32>;
@group(0) @binding(5) var<storage,read_write> prefixes:array<u32>;
@group(0) @binding(6) var<storage,read> repair_order:array<u32>;
@group(0) @binding(7) var<uniform> phase:vec4<u32>;
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
@compute @workgroup_size(64)
fn detect_records(@builtin(global_invocation_id)id:vec3<u32>){
    let index=id.x;if index>=params.selection.z{return;}
    let w=params.source.x;let h=params.source.y;let record=records[index];let xy=vec2<i32>(i32(record.x%w),i32(record.x/w));let mask=record.y&255u;var found=0u;
    // Original samples and each record's original mask are independent inputs.
    if xy.x>=2 && xy.y>=2 && xy.x<i32(w)-1 && xy.y<i32(h)-1 && mask!=0u{
        for(var n=0u;n<8u;n++){
            if (mask&(1u<<n))!=0u{let q=xy+camera_offsets[n];if bad_red(u32(q.y)*w+u32(q.x)){found|=1u<<n;}}
        }
    }
    detections[index]=found;
}
@compute @workgroup_size(1)
fn detection_prefix(){
    var count=0u;
    for(var index=0u;index<params.selection.z;index++){
        prefixes[index]=count;count=min(count+countOneBits(detections[index]),params.selection.y);
    }
    records[params.selection.z+params.selection.y]=vec2(count,0u);
}
@compute @workgroup_size(64)
fn scatter_detections(@builtin(global_invocation_id)id:vec3<u32>){
    let index=id.x;if index>=params.selection.z{return;}
    let w=params.source.x;let record=records[index];let xy=vec2<i32>(i32(record.x%w),i32(record.x/w));let mask=record.y&255u;var remaining=mask;var next=prefixes[index];
    for(var n=0u;n<8u;n++){
        if (detections[index]&(1u<<n))!=0u && next<params.selection.y{
            let q=xy+camera_offsets[n];let p=u32(q.y)*w+u32(q.x);
            records[params.selection.z+next]=vec2(p,0x4300u|new_mask(camera_offsets[n],mask));next++;remaining-=1u<<n;
        }
    }
    // Only accepted detections alter main masks; exhaustion retains the original.
    if index<params.source.z && index<1000000u && remaining!=0u{records[index].y=(record.y&0xff00u)|remaining;}
}
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id)id:vec3<u32>){
    if id.x>=phase.y{return;}
    // Metadata phases preserve all neighboring/duplicate record dependencies.
    let record=records[repair_order[phase.x+id.x]];
    camera_correct(record,params.source.x,params.source.y);
}
@compute @workgroup_size(1)
fn detected_correct(){
    let w=params.source.x;let h=params.source.y;let base=params.selection.z;let count=records[base+params.selection.y].x;
    for(var index=0u;index<count;index++){
        let record=records[base+index];if ((record.y>>8u)&15u)>=params.selection.x{camera_correct(record,w,h);}
    }
}
