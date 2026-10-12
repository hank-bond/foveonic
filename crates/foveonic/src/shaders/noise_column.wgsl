// Color-path native column conditioning after base filtering, before luminance
// detail. Adapted with owner permission from Foveonic conditioning/column.rs.
@group(0) @binding(0) var<storage,read> input_pixels:array<f32>;
@group(0) @binding(1) var<uniform> params:NoiseParams;
@group(0) @binding(2) var<storage,read_write> smoothed:array<f32>;
@group(0) @binding(3) var<storage,read_write> deltas:array<f32>;
@group(0) @binding(4) var<storage,read_write> output_pixels:array<f32>;
@group(0) @binding(5) var<storage,read> table:array<f32>;
fn rgb(p:u32)->vec3<f32>{return vec3(input_pixels[p*3u],input_pixels[p*3u+1u],input_pixels[p*3u+2u]);}
fn cuts(tile:u32)->vec2<u32>{
    let row=tile/params.source.z;
    let start=select(0u,params.column_area.y,row==0u);
    let end=select(params.tile.y,params.tile.y-(params.source.y-params.column_area.w),row+1u==params.source.w);
    return vec2(start,end);
}
@compute @workgroup_size(8,8)
fn horizontal(@builtin(global_invocation_id)id:vec3<u32>){
    if id.x>=params.tile.x || id.y>=params.tile.y*params.source.z*params.source.w{return;}
    let p=atlas_position(id.xy,params);let index=tile_pixel(p,params);var result=vec3(0.0);
    // Native filtering stops one column before the final correction column.
    if p.x>=13u && p.x<params.tile.x-14u{
        let center=rgb(index);let scales=(2047.0/(8.0/params.column.x))/variance(center,params);
        var sum=vec3(0.0);var total=0.0;
        for(var x=p.x-13u;x<=p.x+13u;x++){
            let sample=rgb(tile_pixel(vec3(x,p.y,p.z),params));let d=sample-center;
            let distance=fma(d.z*d.z,scales.z,fma(d.x*d.x,scales.x,scales.y*(d.y*d.y)));
            if distance<2047.5{let lookup=u32(floor(max(distance,0.0)+0.5));let weight=table[lookup];total+=weight;sum=fma(sample,vec3(weight),sum);}
        }
        result=center;if total!=0.0{result=sum/total;}
    }
    smoothed[index*3u]=result.x;smoothed[index*3u+1u]=result.y;smoothed[index*3u+2u]=result.z;
}
@compute @workgroup_size(64)
fn block_deltas(@builtin(global_invocation_id)id:vec3<u32>){
    let blocks=params.tile.y/40u;let tile=id.y/blocks;let block=id.y%blocks;
    if id.x>=params.tile.x || tile>=params.source.z*params.source.w{return;}
    let area=cuts(tile);let count=(area.y-area.x)/40u;var delta=vec3(0.0);
    if block<count{
        let first=area.x+block*40u;let last=select(first+40u,area.y,block+1u==count);
        var original=vec3(0.0);var filtered=vec3(0.0);
        for(var y=first;y<last;y++){
            let p=tile_pixel(vec3(id.x,y,tile),params);original+=rgb(p);
            filtered+=vec3(smoothed[p*3u],smoothed[p*3u+1u],smoothed[p*3u+2u]);
        }
        // The native divisor is deliberately block height plus one.
        let divisor=f32(last-first)+1.0;delta=original/divisor-filtered/divisor;
    }
    let out=((tile*blocks+block)*params.tile.x+id.x)*3u;
    deltas[out]=delta.x;deltas[out+1u]=delta.y;deltas[out+2u]=delta.z;
}
@compute @workgroup_size(8,8)
fn correct(@builtin(global_invocation_id)id:vec3<u32>){
    if id.x>=params.tile.x || id.y>=params.tile.y*params.source.z*params.source.w{return;}
    let p=atlas_position(id.xy,params);let index=tile_pixel(p,params);var value=rgb(index);let area=cuts(p.z);let count=(area.y-area.x)/40u;
    if count>0u && p.y>=area.x && p.y<area.y && p.x>=13u && p.x<params.tile.x-13u{
        let block=min((p.y-area.x)/40u,count-1u);let q=((p.z*(params.tile.y/40u)+block)*params.tile.x+p.x)*3u;
        let delta=vec3(deltas[q],deltas[q+1u],deltas[q+2u]);let eligible=abs(value*0.06)>abs(delta);
        value=clamp(value-select(vec3(0.0),delta,eligible),vec3(0.0),vec3(32766.0));
    }
    output_pixels[index*3u]=value.x;output_pixels[index*3u+1u]=value.y;output_pixels[index*3u+2u]=value.z;
}
