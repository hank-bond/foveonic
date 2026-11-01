// Estimates cumulative bilateral offsets on immutable native layer tiles.
// The narrow compatibility pass retains the reference implementation's terminal
// halo and block divisor. Added bands reflect at the physical active boundary.
const BLOCK_SIZE:u32=40u;
const RANGE_LAST:f32=2047.0;
const RANGE_CUTOFF:f32=2047.5;
const RANGE_FACTOR:f32=8.0;
const RELATIVE_LIMIT:f32=0.06;
const NOISE_SIGMAS:f32=2.0;
const CODE_MAX:f32=32766.0;
const FINE_SLOT:u32=0u;
const COARSE_SLOT:u32=1u;
const REFLECTED_FINE_SLOT:u32=2u;
struct BandParams {
    active_area:vec4<u32>,
    sizes:vec4<u32>,
    strengths:vec4<f32>,
    axis:u32,
    estimate_slot:u32,
    reflect_samples:u32,
    reference_fine_boundary:u32,
}
@group(0) @binding(0) var<storage,read> input_pixels:array<f32>;
@group(0) @binding(1) var<uniform> params:NoiseParams;
@group(0) @binding(2) var<storage,read_write> smoothed:array<f32>;
@group(0) @binding(3) var<storage,read_write> deltas:array<f32>;
@group(0) @binding(4) var<storage,read_write> output_pixels:array<f32>;
@group(0) @binding(5) var<storage,read> table:array<f32>;
@group(0) @binding(6) var<uniform> band:BandParams;
fn load_input(index:u32)->vec3<f32>{
    return vec3(input_pixels[index*3u],input_pixels[index*3u+1u],input_pixels[index*3u+2u]);
}
fn sensor_position(point:vec3<u32>)->vec2<i32>{
    let origin=native_tile(point.z,params)*(params.source.xy/params.source.zw);
    return vec2<i32>(origin+point.xy)-i32(params.tile.z);
}
fn reflect_coordinate(value:i32,first:u32,last:u32)->u32{
    if value>=i32(first) && value<i32(last){
        return u32(value);
    }
    let length=i32(last-first)-1;
    let period=2*length;
    let shifted=value-i32(first);
    let wrapped=((shifted%period)+period)%period;
    return first+u32(min(wrapped,period-wrapped));
}
fn measurement_index(point:vec3<u32>)->u32{
    if band.reflect_samples==0u{
        return tile_pixel(point,params);
    }
    let global=sensor_position(point);
    let reflected=vec2(
    reflect_coordinate(global.x,band.active_area.x,band.active_area.z),
    reflect_coordinate(global.y,band.active_area.y,band.active_area.w));
    let origin=native_tile(point.z,params)*(params.source.xy/params.source.zw);
    let local=vec2<i32>(reflected)-vec2<i32>(origin)+i32(params.tile.z);
    return tile_pixel(vec3(vec2<u32>(local),point.z),params);
}
fn directional_point(direction:u32,other:u32,tile:u32)->vec3<u32>{
    if band.axis==0u{
        return vec3(direction,other,tile);
    }
    return vec3(other,direction,tile);
}
fn measurement_bounds(tile:u32)->vec2<u32>{
    let perpendicular=1u-band.axis;
    let index=native_tile(tile,params)[perpendicular];
    let start=select(0u,params.column_area[perpendicular],index==0u);
    let end=select(params.tile[perpendicular],
    params.tile[perpendicular]-(params.source[perpendicular]-params.column_area[perpendicular+2u]),
    index+1u==params.source[perpendicular+2u]);
    return vec2(start,end);
}
fn estimate_stride()->u32{
    return (params.tile[1u-band.axis]/BLOCK_SIZE)*params.tile[band.axis]*tile_count(params)*3u;
}
fn estimate_index(direction:u32,block:u32,tile:u32,scale:u32)->u32{
    let blocks=params.tile[1u-band.axis]/BLOCK_SIZE;
    return scale*estimate_stride()+((tile*blocks+block)*params.tile[band.axis]+direction)*3u;
}
fn load_estimate(direction:u32,block:u32,tile:u32,scale:u32)->vec3<f32>{
    let index=estimate_index(direction,block,tile,scale);
    return vec3(deltas[index],deltas[index+1u],deltas[index+2u]);
}
@compute @workgroup_size(8,8)
fn neighborhoods(@builtin(global_invocation_id)id:vec3<u32>){
    if id.x>=params.tile.x || id.y>=params.tile.y*tile_count(params){
        return;
    }
    let point=atlas_position(id.xy,params);
    let index=tile_pixel(point,params);
    let direction=point[band.axis];
    let half=band.sizes[band.estimate_slot]/2u;
    var result=vec3(0.0);
    // Preserve the reviewed terminal halo, including its last unused smooth pixel.
    if direction>=half && direction<params.tile[band.axis]-half-1u{
        let center=load_input(measurement_index(point));
        let scales=(RANGE_LAST/(RANGE_FACTOR/params.column.x))/variance(center,params);
        var sum=vec3(0.0);
        var total=0.0;
        for(var offset=direction-half;offset<=direction+half;offset++){
            var neighbor=point;
            neighbor[band.axis]=offset;
            let sample=load_input(measurement_index(neighbor));
            let difference=sample-center;
            let distance=fma(difference.z*difference.z,scales.z,
            fma(difference.x*difference.x,scales.x,scales.y*(difference.y*difference.y)));
            if distance<RANGE_CUTOFF{
                let weight=table[u32(floor(max(distance,0.0)+0.5))];
                total+=weight;
                sum=fma(sample,vec3(weight),sum);
            }
        }
        result=center;
        if total!=0.0{
            result=sum/total;
        }
    }
    smoothed[index*3u]=result.x;
    smoothed[index*3u+1u]=result.y;
    smoothed[index*3u+2u]=result.z;
}
@compute @workgroup_size(64)
fn block_deltas(@builtin(global_invocation_id)id:vec3<u32>){
    let blocks=params.tile[1u-band.axis]/BLOCK_SIZE;
    let tile=id.y/blocks;
    let block=id.y%blocks;
    if id.x>=params.tile[band.axis] || tile>=tile_count(params){
        return;
    }
    let bounds=measurement_bounds(tile);
    let count=(bounds.y-bounds.x)/BLOCK_SIZE;
    var delta=vec3(0.0);
    if block<count{
        let first=bounds.x+block*BLOCK_SIZE;
        let last=select(first+BLOCK_SIZE,bounds.y,block+1u==count);
        var original=vec3(0.0);
        var filtered=vec3(0.0);
        for(var other=first;other<last;other++){
            let point=directional_point(id.x,other,tile);
            original+=load_input(measurement_index(point));
            let index=tile_pixel(point,params)*3u;
            filtered+=vec3(smoothed[index],smoothed[index+1u],smoothed[index+2u]);
        }
        let divisor=f32(last-first)+1.0;
        delta=original/divisor-filtered/divisor;
    }
    let index=estimate_index(id.x,block,tile,band.estimate_slot);
    deltas[index]=delta.x;
    deltas[index+1u]=delta.y;
    deltas[index+2u]=delta.z;
}
@compute @workgroup_size(8,8)
fn correct(@builtin(global_invocation_id)id:vec3<u32>){
    if id.x>=params.tile.x || id.y>=params.tile.y*tile_count(params){
        return;
    }
    let point=atlas_position(id.xy,params);
    let index=tile_pixel(point,params);
    var value=load_input(index);
    let bounds=measurement_bounds(point.z);
    let count=(bounds.y-bounds.x)/BLOCK_SIZE;
    let direction=point[band.axis];
    let other=point[1u-band.axis];
    var correction=vec3(0.0);
    if count>0u && other>=bounds.x && other<bounds.y{
        let block=min((other-bounds.x)/BLOCK_SIZE,count-1u);
        let limit=max(abs(value*RELATIVE_LIMIT),NOISE_SIGMAS*sqrt(variance(value,params)/f32(BLOCK_SIZE)));
        let global=sensor_position(point);
        let in_active=all(global>=vec2<i32>(band.active_area.xy)) && all(global<vec2<i32>(band.active_area.zw));
        if band.strengths.x!=0.0 && (band.reference_fine_boundary!=0u || in_active){
            let half=band.sizes.x/2u;
            if direction>=half && direction<params.tile[band.axis]-half{
                let delta=load_estimate(direction,block,point.z,0u);
                let unit_corrected=clamp(value-select(vec3(0.0),delta,limit>abs(delta)),vec3(0.0),vec3(CODE_MAX));
                // Scale the complete unit correction, including its black floor.
                // Keep the reviewed unit-strength path exact. Higher strengths
                // extrapolate in the signed calibrated-layer domain.
                if band.strengths.x==1.0{
                    value=unit_corrected;
                }
                else{
                    value=mix(value,unit_corrected,band.strengths.x);
                }
            }
        }
        let coarse_radius=band.sizes[COARSE_SLOT]/2u;
        if band.strengths.y!=0.0 && in_active && direction>=coarse_radius && direction<params.tile[band.axis]-coarse_radius-1u{
            let baseline=select(FINE_SLOT,REFLECTED_FINE_SLOT,band.reference_fine_boundary!=0u);
            let delta=load_estimate(direction,block,point.z,COARSE_SLOT)-load_estimate(direction,block,point.z,baseline);
            // One decision retains the direction of the three-layer offset.
            if all(limit>abs(delta)){
                correction+=delta*band.strengths.y;
            }
        }
    }
    value-=correction;
    output_pixels[index*3u]=value.x;
    output_pixels[index*3u+1u]=value.y;
    output_pixels[index*3u+2u]=value.z;
}
