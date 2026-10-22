struct Params { shape: vec4<u32>, controls: vec4<f32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<storage, read> guide: array<f32>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read_write> destination: array<f32>;
@group(0) @binding(4) var<storage, read_write> transposed_guide: array<f32>;
@group(0) @binding(5) var<storage, read_write> scratch: array<vec4<f32>>;
fn product(a: f32, b: f32) -> vec2<f32> {
    let high = fma(a, b, 0.0);
    return vec2<f32>(high, fma(a, b, -high));
}
fn add(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let sum = fma(1.0, a.x, b.x);
    let virtual_b = fma(-1.0, a.x, sum);
    let remainder_a = fma(-1.0, fma(-1.0, virtual_b, sum), a.x);
    let remainder_b = fma(-1.0, virtual_b, b.x);
    let error = fma(1.0, fma(1.0, remainder_a, remainder_b), fma(1.0, a.y, b.y));
    let high = fma(1.0, sum, error);
    return vec2<f32>(high, fma(-1.0, fma(-1.0, sum, high), error));
}
fn multiply(value: vec2<f32>, factor: f32) -> vec2<f32> {
    return add(product(value.x, factor), product(value.y, factor));
}

fn gain_product(value: f32, weight: f32) -> f32 {
    let product=multiply(product(params.controls.x,weight),value);
    return fma(1.0,product.x,product.y);
}
fn divide(a:f32,b:f32)->f32 {
    let quotient=a/b;
    return fma(1.0,quotient,fma(-quotient,b,a)/b);
}
@compute @workgroup_size(64)
fn noise_recursive_mean(@builtin(global_invocation_id) id:vec3<u32>) {
    let width=params.shape.x; let height=params.shape.y; let y=id.x;
    if y>=height { return; }
    var forward=vec4<f32>(source[y*width*3u],source[y*width*3u+1u],source[y*width*3u+2u],1.0);
    scratch[y*width]=forward;
    for(var x=1u;x<width;x++) {
        let pixel=y*width+x;
        for(var ch=0u;ch<3u;ch++) { forward[ch]=fma(1.0,source[pixel*3u+ch],gain_product(forward[ch],guide[pixel])); }
        forward.w=fma(1.0,1.0,gain_product(forward.w,guide[pixel]));
        scratch[pixel]=forward;
    }
    var reverse=vec4<f32>(source[(y*width+width-1u)*3u],source[(y*width+width-1u)*3u+1u],source[(y*width+width-1u)*3u+2u],1.0);
    for(var x=i32(width)-1;x>=0;x--) {
        let pixel=y*width+u32(x);
        if u32(x)+1u<width {
            for(var ch=0u;ch<3u;ch++) { reverse[ch]=fma(1.0,source[pixel*3u+ch],gain_product(reverse[ch],guide[pixel])); }
            reverse.w=fma(1.0,1.0,gain_product(reverse.w,guide[pixel]));
        }
        var numerator=vec3<f32>(0.0); var denominator=1.0;
        if x==0 { numerator=reverse.xyz; denominator=reverse.w; }
        else if u32(x)+1u==width { numerator=scratch[pixel].xyz; denominator=scratch[pixel].w; }
        else {
            denominator=fma(-1.0,1.0,fma(1.0,scratch[pixel].w,reverse.w));
            for(var ch=0u;ch<3u;ch++) { numerator[ch]=fma(-1.0,source[pixel*3u+ch],fma(1.0,scratch[pixel][ch],reverse[ch])); }
        }
        let transposed=u32(x)*height+y;
        for(var ch=0u;ch<3u;ch++) { destination[transposed*3u+ch]=divide(numerator[ch],denominator); }
        transposed_guide[transposed]=guide[pixel];
    }
}
