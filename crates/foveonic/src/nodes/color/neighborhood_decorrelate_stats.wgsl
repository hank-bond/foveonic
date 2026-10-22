struct Params { shape: vec4<u32>, controls: vec4<f32> }
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read> model: array<vec2<f32>>;
@group(0) @binding(3) var<storage, read> intensity: array<f32>;
@group(0) @binding(4) var<storage, read_write> stats: array<f32>;
fn product(a:f32,b:f32)->vec2<f32> { let h=fma(a,b,0.0); return vec2<f32>(h,fma(a,b,-h)); }
fn add(a:vec2<f32>,b:vec2<f32>)->vec2<f32> {
    let s=fma(1.0,a.x,b.x); let v=fma(-1.0,a.x,s);
    let e=fma(1.0,fma(-1.0,fma(-1.0,v,s),a.x),fma(-1.0,v,b.x));
    let t=fma(1.0,e,fma(1.0,a.y,b.y)); let h=fma(1.0,s,t);
    return vec2<f32>(h,fma(-1.0,fma(-1.0,s,h),t));
}
fn multiply(a:vec2<f32>,b:vec2<f32>)->vec2<f32> {
    return add(product(a.x,b.x),vec2<f32>(fma(a.x,b.y,a.y*b.x),0.0));
}
fn divide(a:vec2<f32>,b:f32)->vec2<f32> {
    let q=a.x/b; let r=add(a,-product(q,b)); return add(vec2<f32>(q,0.0),vec2<f32>((r.x+r.y)/b,0.0));
}
fn sample(x:i32,y:i32,ch:u32)->f32 {
    let index=(u32(max(y,0))*params.shape.x+u32(clamp(x,0,i32(params.shape.x))))*3u+ch;
    // The reference reads its right halo as the following packed row. The final
    // unowned tail contributes zero here and cannot affect the photographic crop.
    if index>=arrayLength(&source) { return 0.0; }
    return source[index];
}
@compute @workgroup_size(8,8)
fn neighborhood_noise_stats(@builtin(global_invocation_id) id:vec3<u32>) {
    if id.x>=params.shape.x || id.y>=params.shape.y { return; }
    let base=(id.y*params.shape.x+id.x)*7u;
    var means:array<vec2<f32>,3>;
    for(var ch=0u;ch<3u;ch++) {
        var total=vec2<f32>(0.0);
        for(var dy=-1;dy<=1;dy++) {
            let y=i32(id.y)+dy;
            if y>=i32(params.shape.y) { continue; }
            for(var dx=-1;dx<=1;dx++) { total=add(total,vec2<f32>(sample(i32(id.x)+dx,y,ch),0.0)); }
        }
        means[ch]=divide(total,9.0);
        stats[base+ch]=fma(1.0,means[ch].x,means[ch].y);
        let linear=add(multiply(means[ch],model[ch+3u]),model[ch]);
        let quadratic=multiply(multiply(model[ch+6u],means[ch]),means[ch]);
        let variance=add(linear,quadratic);
        stats[base+3u+ch]=abs(fma(1.0,variance.x,variance.y));
    }
    let gray=add(divide(add(add(means[0],means[1]),means[2]),3.0),vec2<f32>(1.5e-8,0.0));
    let lower=floor(gray.x); let fraction=add(gray,vec2<f32>(-lower,0.0));
    var rounded=lower;
    if fraction.x>0.5 || (fraction.x==0.5 && (fraction.y>0.0 || (fraction.y==0.0 && (u32(max(lower,0.0))&1u)!=0u))) { rounded+=1.0; }
    stats[base+6u]=intensity[u32(clamp(rounded,0.0,14384.0))];
}
