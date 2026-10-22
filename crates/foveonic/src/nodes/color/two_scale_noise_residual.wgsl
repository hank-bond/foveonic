struct Params { shape: vec4<u32>, controls: vec4<f32> }
@group(0) @binding(0) var<storage, read> current: array<f32>;
@group(0) @binding(1) var<storage, read> candidate: array<f32>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read> lookup: array<f32>;
@group(0) @binding(4) var<storage, read_write> destination: array<f32>;
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

fn divide(a:f32,b:f32)->f32 { let q=a/b; return fma(1.0,q,fma(-q,b,a)/b); }
// A direct pow(x,1.5) can shift a residual-LUT index at a half-integer.
// Refine sqrt with its exact product residual before narrowing x*sqrt(x).
fn power_three_halves(value:f32)->f32 {
    if value==0.0 { return 0.0; }
    let root=sqrt(value);
    let correction=divide(fma(-root,root,value),fma(root,2.0,0.0));
    let powered=add(product(value,root),product(value,correction));
    return fma(1.0,powered.x,powered.y);
}
@compute @workgroup_size(8,8)
fn noise_residual_blend(@builtin(global_invocation_id) id:vec3<u32>) {
    let width=params.shape.x; let height=params.shape.y;
    if id.x>=width || id.y>=height { return; }
    let pixel=(id.y*width+id.x)*3u;
    let old=vec3<f32>(current[pixel],current[pixel+1u],current[pixel+2u]);
    let proposed=vec3<f32>(candidate[pixel],candidate[pixel+1u],candidate[pixel+2u]);
    let index=u32(floor(clamp(fma(-1.0,448.21875,max(old.x,max(old.y,old.z))),1.0,14343.0)+0.5))-1u;
    var amount=lookup[index];
    if all(proposed>vec3<f32>(0.0)) {
        let brightness=divide(clamp(fma(-1.0,200.0,max(proposed.x,max(proposed.y,proposed.z))),0.0,2800.0),2800.0);
        var residuals=vec3<f32>(0.0);
        for(var ch=0u;ch<3u;ch++) { residuals[ch]=divide(power_three_halves(abs(fma(-1.0,old[ch],proposed[ch]))),proposed[ch]); }
        let residual=fma(fma(1.0,fma(1.0,residuals.x,residuals.y),residuals.z),brightness,0.0);
        var position=1023.0;
        if residual<=3.0 { position=fma(divide(residual,3.0),1023.0,0.0); }
        let residual_index=u32(floor(clamp(position,0.0,1023.0)+0.5));
        amount=fma(amount,fma(1.0,lookup[14344u+residual_index],1.0),0.0);
    }
    amount=max(amount,params.controls.y);
    let complement=add(vec2<f32>(1.0,0.0),vec2<f32>(-amount,0.0));
    for(var ch=0u;ch<3u;ch++) {
        let old_product=multiply(complement,old[ch]);
        destination[pixel+ch]=fma(1.0,fma(proposed[ch],amount,0.0),fma(1.0,old_product.x,old_product.y));
    }
}
