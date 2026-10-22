struct Params { shape:vec4<u32>, controls:vec4<f32> }
@group(0) @binding(0) var<storage,read> source:array<f32>;
@group(0) @binding(1) var<uniform> params:Params;
@group(0) @binding(2) var<storage,read_write> destination:array<f32>;
fn luma(pixel:u32)->f32 {
    return fma(source[pixel*3u+2u],0.114,fma(source[pixel*3u],0.299,fma(source[pixel*3u+1u],0.587,0.0)));
}
@compute @workgroup_size(8,8)
fn box_contrast_horizontal(@builtin(global_invocation_id) id:vec3<u32>) {
    let width=params.shape.x; let height=params.shape.y; let radius=params.shape.z;
    if id.x>=width || id.y>=height { return; }
    let pixel=id.y*width+id.x;
    destination[pixel]=f32(u32(luma(pixel))&65535u);
    if id.x<radius || id.y<radius || id.x+radius>width || id.y+radius>height { return; }
    var sum=0.0;
    for(var dx=-i32(radius);dx<=i32(radius);dx++) {
        // The native last center can read the next row's first pixel. Retain
        // that logical position, with a defined replicated final physical row.
        let sample=min(u32(i32(pixel)+dx),width*height-1u);
        sum=fma(f32(u32(luma(sample))&65535u),params.controls.x,sum);
    }
    destination[pixel]=f32(u32(fma(1.0,sum,0.5))&65535u);
}
