struct Params { shape: vec4<u32>, controls: vec4<f32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read> stats: array<f32>;
@group(0) @binding(3) var<storage, read> range_weights: array<f32>;
@group(0) @binding(4) var<storage, read_write> destination: array<f32>;
fn divide(numerator:f32,denominator:f32)->f32 {
    let q=numerator/denominator;
    let residual=fma(-q,denominator,numerator);
    return fma(1.0,q,residual/denominator);
}
fn sample(index:u32)->vec3<f32> {
    let base=index*3u;
    if base+2u>=arrayLength(&source) { return vec3<f32>(0.0); }
    return vec3<f32>(source[base],source[base+1u],source[base+2u]);
}
fn update(cx:u32,cy:u32,value:vec3<f32>,incoming:vec3<f32>)->vec3<f32> {
    let base=(cy*params.shape.x+cx)*7u;
    let mean=vec3<f32>(stats[base],stats[base+1u],stats[base+2u]);
    let variance=vec3<f32>(stats[base+3u],stats[base+4u],stats[base+5u]);
    let delta=value-mean;
    let squares=fma(delta,delta,vec3<f32>(0.0));
    let terms=vec3<f32>(divide(squares.x,variance.x),divide(squares.y,variance.y),divide(squares.z,variance.z));
    let sum=fma(1.0,fma(1.0,terms.x,terms.y),terms.z);
    let scaled=fma(fma(stats[base+6u],params.controls.x,0.0),sum,0.0);
    let range_index=min(2047u,u32(fma(1.0,scaled,0.5)));
    let weight=range_weights[range_index];
    let complement=fma(-1.0,weight,1.0);
    let mixed=fma(vec3<f32>(1.0),value*complement,mean*weight);
    return fma(vec3<f32>(1.0),incoming*0.5,mixed*0.5);
}
fn visit(cx:u32,cy:u32,pixel_index:u32,incoming:vec3<f32>)->vec3<f32> {
    var result=incoming;
    for(var dy=-1;dy<=1;dy++) {
        for(var dx=-1;dx<=1;dx++) {
            let y=i32(cy)+dy;
            var index=0u; var value=vec3<f32>(0.0);
            if y<i32(params.shape.y) {
                index=u32(max(y,0))*params.shape.x+u32(clamp(i32(cx)+dx,0,i32(params.shape.x)));
                value=sample(index);
            }
            if index!=pixel_index { continue; }
            result=update(cx,cy,value,result);
        }
    }
    return result;
}
@compute @workgroup_size(8,8)
fn neighborhood_decorrelate(@builtin(global_invocation_id) id:vec3<u32>) {
    let width=params.shape.x; let height=params.shape.y;
    if id.x>=width || id.y>=height { return; }
    let pixel_index=id.y*width+id.x;
    var result=sample(pixel_index);
    if id.x>0u && id.x+1u<width && id.y>0u && id.y+1u<height {
        // Interior pixels receive one update from each of nine centers.
        // Visit them directly in the reference's center order.
        let value=result;
        for(var cy=id.y-1u;cy<=id.y+1u;cy++) {
            for(var cx=id.x-1u;cx<=id.x+1u;cx++) {
                result=update(cx,cy,value,result);
            }
        }
    } else {
        let first_y=max(0,i32(id.y)-2); let last_y=min(height-1u,id.y+1u);
        for(var cy=u32(first_y);cy<=last_y;cy++) {
            if pixel_index==0u && cy+1u==height {
                for(var cx=0u;cx<width;cx++) { result=visit(cx,cy,pixel_index,result); }
            } else if id.x==0u {
                result=visit(0u,cy,pixel_index,result);
                if width>1u { result=visit(1u,cy,pixel_index,result); }
                if width>2u { result=visit(width-1u,cy,pixel_index,result); }
            } else {
                for(var cx=id.x-1u;cx<=min(width-1u,id.x+1u);cx++) { result=visit(cx,cy,pixel_index,result); }
            }
        }
        // The bottom row's missing neighborhood entries are zero-valued updates at
        // the origin. They occur after every ordinary contributing center.
        if pixel_index==0u && height>2u {
            for(var cx=0u;cx<width;cx++) { result=visit(cx,height-1u,pixel_index,result); }
        }
    }
    let base=pixel_index*3u;
    destination[base]=result.x; destination[base+1u]=result.y; destination[base+2u]=result.z;
}
