// Native deterministic luminance-ratio mask, ordered line permutations, and blend.
// First-pass unwritten transposed scratch uses an explicit native backing seed.
struct PatternParams{shape:vec4<u32>,thresholds:vec4<f32>}
@group(0) @binding(0) var<storage,read> original:array<f32>;
@group(0) @binding(1) var<storage,read> edges:array<f32>;
@group(0) @binding(2) var<uniform> params:PatternParams;
@group(0) @binding(3) var<storage,read_write> masks:array<u32>;
@group(0) @binding(4) var<storage,read_write> counts:array<u32>;
@group(0) @binding(6) var<storage,read> offsets:array<u32>;
@group(0) @binding(7) var<storage,read_write> permuted:array<f32>;
@group(0) @binding(8) var<storage,read_write> output_values:array<f32>;
fn pixel(x:u32,y:u32,tile:u32)->u32{return (tile*params.shape.y+y)*params.shape.x+x;}
@compute @workgroup_size(8,8)
fn mask(@builtin(global_invocation_id)id:vec3<u32>){
    let w=params.shape.x;let h=params.shape.y;let n=params.shape.w;let span=2u*n+1u;
    if id.x>=w || id.y>=h*params.shape.z{return;}
    let x=id.x;let y=id.y%h;let tile=id.y/h;let p=pixel(x,y,tile);var selected=false;
    if w>2u*span && h>2u*span && x>=span && x<w-span && y>=span && y<h-span{
        var hi=0.0;var lo=0.0;
        // Exact final complete-group extent of the transposed second pass.
        let last=2u*n+((h-span-1u-2u*n)/span)*span;
        let horizontal_last=2u*n+((w-span-1u-2u*n)/span)*span;
        if y>=n && y<=last+n{
            hi=-3.402823466e38;lo=3.402823466e38;
            for(var yy=y-n;yy<=y+n;yy++){
                // Native full/reduced transposed allocation seed, prepared independently.
                var row_hi=edges[pixel(x,yy,tile)];var row_lo=row_hi;
                if yy>=span && yy<h-span && x<=horizontal_last+n{
                    row_hi=original[pixel(x-n,yy,tile)];row_lo=row_hi;
                    for(var xx=x-n+1u;xx<=x+n;xx++){
                        let value=original[pixel(xx,yy,tile)];row_hi=max(row_hi,value);row_lo=min(row_lo,value);
                    }
                }
                hi=max(hi,row_hi);lo=min(lo,row_lo);
            }
        }
        selected=abs(hi-1.0)<params.thresholds.y && abs(lo-1.0)<params.thresholds.y;
    }
    masks[p]=select(0u,1u,selected);
}
@compute @workgroup_size(64)
fn line_counts(@builtin(global_invocation_id)id:vec3<u32>){
    let w=params.shape.x;let h=params.shape.y;let tile=id.y;let lane=id.x;
    if tile>=params.shape.z{return;}
    let base=tile*(w+h);
    if lane<h{var sum=0u;for(var x=0u;x<w;x++){sum+=masks[pixel(x,lane,tile)];}counts[base+lane]=sum;}
    if lane<w{var sum=0u;for(var y=0u;y<h;y++){sum+=masks[pixel(lane,y,tile)];}counts[base+h+lane]=sum;}
}
@compute @workgroup_size(1)
fn line_prefix(@builtin(global_invocation_id)id:vec3<u32>){
    let w=params.shape.x;let h=params.shape.y;let tile=id.x;if tile>=params.shape.z{return;}
    let base=tile*(w+h);var sum=0u;
    for(var y=0u;y<h;y++){let value=counts[base+y];counts[base+y]=sum;sum+=value;}
    sum=0u;for(var x=0u;x<w;x++){let value=counts[base+h+x];counts[base+h+x]=sum;sum+=value;}
}
@compute @workgroup_size(64)
fn horizontal(@builtin(global_invocation_id)id:vec3<u32>){
    let w=params.shape.x;let h=params.shape.y;let n=params.shape.w;let span=n*2u+1u;
    let tile=id.x/h;let y=id.x%h;if tile>=params.shape.z || w<=2u*span || y<span || y>=h-span{return;}
    var index=(counts[tile*(w+h)+y]%512u)*span;
    for(var x=span;x<w-span;x++){
        if masks[pixel(x,y,tile)]==0u{continue;}
        for(var j=0u;j<span;j++){
            let a=pixel(x-n+j,y,tile);let b=pixel(x-n+offsets[index+j],y,tile);let value=permuted[a];permuted[a]=permuted[b];permuted[b]=value;
        }
        index+=span;if index>=span*512u{index=0u;}
    }
}
@compute @workgroup_size(64)
fn vertical(@builtin(global_invocation_id)id:vec3<u32>){
    let w=params.shape.x;let h=params.shape.y;let n=params.shape.w;let span=n*2u+1u;
    let tile=id.x/w;let x=id.x%w;if tile>=params.shape.z || h<=2u*span || x<span || x>=w-span{return;}
    var index=(counts[tile*(w+h)+h+x]%512u)*span;
    for(var y=span;y<h-span;y++){
        if masks[pixel(x,y,tile)]==0u{continue;}
        for(var j=0u;j<span;j++){
            let a=pixel(x,y-n+j,tile);let b=pixel(x,y-n+offsets[index+j],tile);let value=permuted[a];permuted[a]=permuted[b];permuted[b]=value;
        }
        index+=span;if index>=span*512u{index=0u;}
    }
}
@compute @workgroup_size(8,8)
fn blend(@builtin(global_invocation_id)id:vec3<u32>){
    let w=params.shape.x;let h=params.shape.y;let span=params.shape.w*2u+1u;
    if id.x>=w || id.y>=h*params.shape.z{return;}
    let y=id.y%h;let p=id.y*w+id.x;var value=permuted[p];
    if w>2u*span && h>2u*span && id.x>=span && id.x<w-span && y>=span && y<h-span{
        let source=original[p];value=source;
        if abs(source-1.0)<params.thresholds.x{value=fma(source,1.0-params.thresholds.z,permuted[p]*params.thresholds.z);}
    }
    output_values[p]=value;
}
@compute @workgroup_size(8,8)
fn mask_image(@builtin(global_invocation_id)id:vec3<u32>){
    if id.x>=params.shape.x || id.y>=params.shape.y*params.shape.z{return;}
    let p=id.y*params.shape.x+id.x;output_values[p]=f32(masks[p])*4095.0;
}
