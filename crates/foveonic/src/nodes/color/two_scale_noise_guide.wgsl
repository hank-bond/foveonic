struct Params { shape: vec4<u32>, controls: vec4<f32> }
@group(0) @binding(0) var<storage, read> original: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read> model: array<vec2<f32>>;
@group(0) @binding(3) var<storage, read_write> guide: array<f32>;
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

@compute @workgroup_size(8,8)
fn noise_edge_guide(@builtin(global_invocation_id) id: vec3<u32>) {
    let width=params.shape.x; let height=params.shape.y;
    if id.x>=width || id.y>=height { return; }
    let pixel=id.y*width+id.x;
    guide[pixel]=0.0;
    // The recovered eleven-wide worker has asymmetric horizontal endpoints.
    if id.x<5u || id.x+6u>=width || id.y<5u || id.y+5u>height { return; }
    let center=vec3(original[pixel*3u],original[pixel*3u+1u],original[pixel*3u+2u]);
    var variance=vec3<f32>(0.0);
    for(var ch=0u;ch<3u;ch++) {
        let value=center[ch];
        let linear=add(multiply(model[ch+3u],value),model[ch]);
        let quadratic=multiply(multiply(model[ch+6u],value),value);
        let polynomial=add(linear,quadratic);
        variance[ch]=fma(abs(fma(1.0,polynomial.x,polynomial.y)),3.0,0.0);
    }
    var count=0u;
    for(var y=id.y-5u;y<=min(id.y+5u,height-1u);y++) {
        for(var x=id.x-5u;x<=id.x+5u;x++) {
            let index=(y*width+x)*3u;
            let sample=vec3(original[index],original[index+1u],original[index+2u]);
            let difference=fma(vec3(-1.),center,sample);
            let different=any(fma(difference,difference,vec3(0.))>variance);
            if different { count++; }
        }
    }
    let quotient=f32(count)/120.0;
    let fraction=fma(1.0,quotient,fma(-quotient,120.0,f32(count))/120.0);
    guide[pixel]=clamp(fma(-1.0,fraction,1.0),0.0,1.0);
}
