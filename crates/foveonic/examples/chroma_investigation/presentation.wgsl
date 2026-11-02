// Float-render crop presentation at native, +2 EV and +4 EV before display quantization.
struct DisplayParams {
    source: vec4<u32>, // Storage width/height and active origin.
    view: vec4<u32>, // Unrotated active width/height, clockwise rotation and crop height in workgroups.
    crop: vec4<u32>, // Oriented half-open crop.
    matrix: array<vec4<f32>,3>,
}
@group(0) @binding(0) var<storage,read> pixels:array<f32>;
@group(0) @binding(1) var<uniform> p:DisplayParams;
@group(0) @binding(2) var<storage,read_write> encoded:array<u32>;
fn srgb(v:vec3<f32>)->vec3<f32>{
    let positive=max(v,vec3(0.));
    return clamp(select(1.055*pow(positive,vec3(1./2.4))-0.055,12.92*positive,positive<=vec3(0.0031308)),vec3(0.),vec3(1.));
}
@compute @workgroup_size(8,8)
fn display_crops(@builtin(global_invocation_id) id:vec3<u32>){
    let native_width=p.crop.z-p.crop.x;let native_height=p.crop.w-p.crop.y;
    let scale=min(1.,1280./f32(max(native_width,native_height)));
    let width=select(native_width,u32(round(f32(native_width)*scale)),p.source.y==1u);
    let height=select(native_height,u32(round(f32(native_height)*scale)),p.source.y==1u);
    let plane=id.y/(p.view.w*8u);let y=id.y%(p.view.w*8u);
    if id.x>=width||y>=height||plane>=3u{return;}
    let offset=select(vec2(id.x,y),min(vec2<u32>((vec2<f32>(f32(id.x),f32(y))+vec2(0.5))/scale),vec2(native_width-1u,native_height-1u)),p.source.y==1u);
    let oriented=p.crop.xy+offset;var native=oriented;
    switch p.view.z {
        case 90u:{native=vec2(oriented.y,p.view.y-1u-oriented.x);}
        case 180u:{native=p.view.xy-vec2(1u)-oriented;}
        case 270u:{native=vec2(p.view.x-1u-oriented.y,oriented.x);}
        default:{}
    }
    native+=p.source.zw;
    let i=(native.y*p.source.x+native.x)*3u;
    let rgb=vec3(pixels[i],pixels[i+1u],pixels[i+2u]);
    let display=vec3(dot(p.matrix[0].xyz,rgb),dot(p.matrix[1].xyz,rgb),dot(p.matrix[2].xyz,rgb));
    let bytes=vec3<u32>(round(srgb(display*exp2(f32(plane)*2.))*255.));
    encoded[(plane*height+y)*width+id.x]=bytes.x|(bytes.y<<8u)|(bytes.z<<16u)|0xff000000u;
}
