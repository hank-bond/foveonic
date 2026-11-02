// Opt-in local color-reference experiments. The production Texture output is immutable.
struct Study {
    selection:vec4<u32>, // Mode, radius, reserved, reserved.
    area:vec4<u32>,
    rgb_y:vec4<f32>,
    neutral_y:vec4<f32>,
    limits:vec4<f32>, // Gain start/end and weak-signal proxy start/end.
    reliability:vec4<f32>, // Input disagreement start/end, maximum correction, reserved.
}
@group(0) @binding(8) var<uniform> study:Study;
@group(0) @binding(9) var<storage,read> original_texture:array<f32>;
@group(0) @binding(10) var<storage,read_write> applied:array<f32>;
fn original_at(pixel:u32)->vec3<f32>{let i=pixel*3u;return vec3(original_texture[i],original_texture[i+1u],original_texture[i+2u]);}
fn fractions(value:vec3<f32>,luma:f32)->vec3<f32>{return value*params.weights.xyz/max(luma,MIN_LUMINANCE);}
fn native_active(point:vec3<u32>)->bool {
    let q=full_position(point,params);
    return all(q>=study.area.xy)&&all(q<study.area.zw);
}
@compute @workgroup_size(8,8)
fn local_reference(@builtin(global_invocation_id)id:vec3<u32>){
    if id.x>=params.tile.x||id.y>=params.tile.y*tile_count(params){return;}
    let pixel=id.y*params.tile.x+id.x;let point=atlas_position(id.xy,params);
    var value=original_at(pixel);var amount=0.;
    let a=base_rgb(pixel);let base_y=dot(a,params.weights.xyz);
    let gain=exp2(mono_log(pixel));
    if study.selection.x!=0u&&controls.strength.w>0.&&base_y>MIN_LUMINANCE&&all(a>=vec3(0.))&&native_active(point){
        let sigma=sqrt(dot(variance(a,params),params.weights.xyz*params.weights.xyz));
        // This is an unfiltered-noise scale proxy, not calibrated uncertainty of the filtered base.
        let weak=1.-smoothstep(study.limits.z,study.limits.w,base_y/max(sigma,MIN_LUMINANCE));
        let amplification=smoothstep(study.limits.x,study.limits.y,gain);
        amount=weak*amplification;
        if amount>0. {
            if study.selection.x==5u {
                let d=max(raw_rgb(pixel),vec3(0.));
                let input_fraction=fractions(d,dot(d,params.weights.xyz));
                amount*=smoothstep(study.reliability.x,study.reliability.y,length(fractions(a,base_y)-input_fraction))*study.reliability.z;
                let reference_y=dot(a,study.rgb_y.xyz);
                if reference_y>MIN_LUMINANCE&&amount>0. {
                    var delta=(study.neutral_y.xyz*reference_y-a)*max(gain-1.,0.)*controls.strength.w*amount;
                    delta-=study.neutral_y.xyz*dot(delta,study.rgb_y.xyz);
                    value+=delta;
                } else {amount=0.;}
            } else {
            let radius=i32(study.selection.y);var sum=vec3(0.);var total=0.;
            for(var dy=-radius;dy<=radius;dy++){
                for(var dx=-radius;dx<=radius;dx++){
                    if dx==0&&dy==0{continue;}
                    let local=vec2<i32>(point.xy)+vec2(dx,dy);
                    if any(local<vec2(0))||any(local>=vec2<i32>(params.tile.xy)){continue;}
                    let neighbor=vec3(vec2<u32>(local),point.z);if !native_active(neighbor){continue;}
                    let n=base_rgb(tile_pixel(neighbor,params));let ny=dot(n,params.weights.xyz);
                    if ny<=MIN_LUMINANCE||any(n<vec3(0.)){continue;}
                    let spatial=exp(-f32(dx*dx+dy*dy)/max(f32(radius*radius),1.));
                    let difference=(ny-base_y)/max(sigma+0.5*(ny+base_y),MIN_LUMINANCE);
                    let support=min(ny/max(base_y+sigma,MIN_LUMINANCE),4.);
                    let weight=spatial*support/(1.+difference*difference);
                    sum+=fractions(n,ny)*weight;total+=weight;
                }
            }
            if total>0. {
                var local_fraction=sum/total;local_fraction/=max(dot(local_fraction,vec3(1.)),MIN_LUMINANCE);
                let current_fraction=fractions(a,base_y);
                let d=max(raw_rgb(pixel),vec3(0.));let dy=dot(d,params.weights.xyz);
                let input_fraction=fractions(d,dy);
                if study.selection.x!=2u {
                    let unsupported=smoothstep(0.04,0.14,length(current_fraction-local_fraction));
                    let input_disagreement=smoothstep(0.04,0.14,length(current_fraction-input_fraction));
                    amount*=unsupported*input_disagreement;
                }
                let brightness=base_y*gain;
                var delta=vec3(0.);
                if study.selection.x==1u {
                    // A deliberately limited control: retain the Chroma-2 proportion only at suspect pixels.
                    delta=(signal_proportion(raw_rgb(pixel))-a/base_y)*brightness*max(controls.strength.w-0.75,0.)*amount;
                } else {
                    let replacement=local_fraction/params.weights.xyz;
                    delta=(replacement-a/base_y)*brightness*controls.strength.w*amount;
                }
                // Eliminate only floating-point luminance drift. The selected brightness stays fixed.
                delta-=vec3(dot(delta,params.weights.xyz)/dot(params.weights.xyz,vec3(1.)));
                value+=delta;
                if study.selection.x==4u&&amount>0. {
                    let original_y=dot(original_at(pixel),study.rgb_y.xyz);
                    let changed_y=dot(value,study.rgb_y.xyz);
                    if original_y>MIN_LUMINANCE&&changed_y>MIN_LUMINANCE {
                        value*=original_y/changed_y;
                    } else {value=original_at(pixel);amount=0.;}
                }
            } else {amount=0.;}
            }
        }
    }
    if amount==0. {value=original_at(pixel);}
    let i=pixel*3u;output_values[i]=value.x;output_values[i+1u]=value.y;output_values[i+2u]=value.z;
    applied[pixel]=amount;
}

@group(0) @binding(11) var<storage,read> corrected_texture:array<f32>;
@group(0) @binding(12) var<storage,read_write> summaries:array<vec4<f32>>;
var<workgroup> stats:array<vec4<f32>,64>;
var<workgroup> rgb_stats:array<f32,64>;
var<workgroup> peak_stats:array<vec4<f32>,64>;
@compute @workgroup_size(64)
fn summarize_reference(@builtin(local_invocation_index)lane:u32,@builtin(workgroup_id)group:vec3<u32>){
    let count=params.tile.x*params.tile.y*tile_count(params);let block=group.x+group.y*256u;
    if block>=(count+255u)/256u{return;}
    var record=vec4(0.);var rgb_change=0.;var peak=vec4(0.);
    for(var k=0u;k<4u;k++){
        let pixel=block*256u+lane+k*64u;if pixel>=count{continue;}
        let i=pixel*3u;let delta=vec3(corrected_texture[i],corrected_texture[i+1u],corrected_texture[i+2u])-original_at(pixel);
        let change=max(abs(delta.x),max(abs(delta.y),abs(delta.z)));
        let point=atlas_position(vec2(pixel%params.tile.x,pixel/params.tile.x),params);
        let core=params.source.xy/params.source.zw;
        let core_sample=all(point.xy>=vec2(params.tile.z))&&all(point.xy<core+vec2(params.tile.z));
        if change>peak.x&&native_active(point)&&core_sample{
            peak=vec4(change,vec2<f32>(full_position(point,params)),applied[pixel]);
        }
        let outside=select(0.,change,applied[pixel]==0.);
        rgb_change=max(rgb_change,abs(dot(delta,study.rgb_y.xyz)));
        record.x=max(record.x,abs(dot(delta,params.weights.xyz)));record.y=max(record.y,outside);
        record.z+=select(0.,1.,change>0.00001);record.w+=applied[pixel];
    }
    stats[lane]=record;rgb_stats[lane]=rgb_change;peak_stats[lane]=peak;workgroupBarrier();
    for(var step=32u;step>0u;step/=2u){
        if lane<step{let other=stats[lane+step];stats[lane]=vec4(max(stats[lane].xy,other.xy),stats[lane].zw+other.zw);rgb_stats[lane]=max(rgb_stats[lane],rgb_stats[lane+step]);if peak_stats[lane+step].x>peak_stats[lane].x{peak_stats[lane]=peak_stats[lane+step];}}
        workgroupBarrier();
    }
    if lane==0u{summaries[block*3u]=stats[0];summaries[block*3u+1u]=vec4(rgb_stats[0],0.,0.,0.);summaries[block*3u+2u]=peak_stats[0];}
}
