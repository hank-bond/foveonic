struct Params { extent:vec2<u32>, padding:vec2<u32>, gains:vec4<f32> }
@group(0) @binding(0) var<storage,read> source:array<f32>;
@group(0) @binding(1) var<uniform> p:Params;
@group(0) @binding(2) var<storage,read> coefficients:array<vec2<f32>>;
@group(0) @binding(3) var<storage,read> intensity:array<f32>;
@group(0) @binding(4) var<storage,read_write> destination:array<f32>;
// Quadratic channel-ratio fits: constant, linear and quadratic coefficients.
const GREEN_FROM_RED = array<vec3<f32>,3>(
    vec3(.43149998784065247,.6410999894142151,-.08860000222921371),
    vec3(.6036999821662903,.7972000241279602,-.3188000023365021),
    vec3(.3910999894142151,.8611999750137329,-.179299995303154));
const RED_FROM_GREEN = array<vec3<f32>,3>(
    vec3(-.023399999365210533,-.11640000343322754,1.1720000505447388),
    vec3(-1.32669997215271,2.2283999919891357,0.),
    vec3(-2204.10009765625,4076.5,-1883.5999755859375));
fn product(a:f32,b:f32)->vec2<f32> {let high=fma(a,b,0.);return vec2(high,fma(a,b,-high));}
fn add(a:vec2<f32>,b:vec2<f32>)->vec2<f32> {
    let sum=fma(1.,a.x,b.x);let vb=fma(-1.,a.x,sum);
    let error=fma(1.,fma(-1.,fma(-1.,vb,sum),a.x),fma(-1.,vb,b.x));
    let low=fma(1.,error,fma(1.,a.y,b.y));let high=fma(1.,sum,low);
    return vec2(high,fma(-1.,fma(-1.,sum,high),low));
}
fn multiply(a:vec2<f32>,b:vec2<f32>)->vec2<f32> {return add(product(a.x,b.x),vec2(fma(a.x,b.y,a.y*b.x),a.y*b.y));}
fn divide(a:vec2<f32>,b:f32)->vec2<f32> {let q=a.x/b;let residual=fma(-q,b,a.x)+a.y;return add(vec2(q,0.),vec2(residual/b,0.));}
fn narrow(a:vec2<f32>)->f32 {return fma(1.,a.x,a.y);}
fn curves(ratio:vec2<f32>,fit:array<vec3<f32>,3>)->vec3<f32> {
    let square=multiply(ratio,ratio);var result:vec3<f32>;
    for(var i=0u;i<3u;i++) {result[i]=narrow(add(add(multiply(ratio,vec2(fit[i].y,0.)),vec2(fit[i].x,0.)),multiply(square,vec2(fit[i].z,0.))));}
    return result;
}
fn load(x:u32,y:u32)->vec3<f32> { let i=(y*p.extent.x+x)*3u;return vec3(source[i],source[i+1u],source[i+2u]); }
fn nearest_curve(candidates:vec3<f32>,observed:f32)->f32 {
    let d=abs(candidates-vec3(observed));
    var result=select(candidates.z,candidates.x,d.x<d.z);
    let second=select(candidates.z,candidates.y,d.y<d.z);
    if d.y<=d.x { result=second; }
    return result;
}
fn coefficient(i:u32)->f32 {return coefficients[i].x+coefficients[i].y;}
fn variance(mean:vec2<f32>,ch:u32)->vec2<f32> {
    return add(add(coefficients[ch],multiply(mean,coefficients[ch+3u])),multiply(multiply(mean,mean),coefficients[ch+6u]));
}
fn blend(original:f32,blue:f32,curve:f32,mean:vec2<f32>,ch:u32,strength:f32)->f32 {
    let estimate=fma(blue,curve,0.);
    let delta=fma(-1.,original,estimate);
    let gain=p.gains[ch];
    let scaled_variance=multiply(multiply(multiply(variance(mean,ch),vec2(strength,0.)),coefficients[9u+ch]),coefficients[9u+ch]);
    let weight=exp(-(delta*delta)/narrow(scaled_variance));
    return fma(estimate,weight,original*(1.-weight));
}
@compute @workgroup_size(8,8)
fn chromatic_ratio(@builtin(global_invocation_id) id:vec3<u32>) {
    if any(id.xy>=p.extent) { return; }
    let input=load(id.x,id.y);var result=input;
    if all(input>vec3(0.)) {
        let scaled=input*p.gains.xyz;
        let blue=max(scaled.z,1.);
        let ratio=scaled.x/blue;
        let gcurve=nearest_curve(curves(vec2(ratio,0.),GREEN_FROM_RED),scaled.y/blue);
        var mean_r=vec2(scaled.x,0.);var mean_g=vec2(scaled.y,0.);
        // The source preserves a three-pixel lower/right rim around its 5x5 fit.
        if id.x>=2u && id.y>=2u && id.x+3u<p.extent.x && id.y+3u<p.extent.y {
            mean_r=vec2(0.);mean_g=vec2(0.);
            for(var dy=-2;dy<=2;dy++) { for(var dx=-2;dx<=2;dx++) {
                let neighbor=load(u32(i32(id.x)+dx),u32(i32(id.y)+dy));
                mean_r=add(mean_r,vec2(neighbor.x,0.));mean_g=add(mean_g,vec2(neighbor.y,0.));
            } }
            mean_r=divide(multiply(mean_r,coefficients[9]),25.);
            mean_g=divide(multiply(mean_g,coefficients[10]),25.);
        }
        let average=((scaled.x+scaled.y)+scaled.z)/3.;
        let lower=floor(average);
        let index=u32(clamp(lower+select(0.,1.,average-lower>=.5),0.,14384.));
        let strength=intensity[index];
        let filtered_green=blend(scaled.y,scaled.z,gcurve,mean_g,1u,strength)/p.gains.y;
        var green=input.y;
        if filtered_green>=0. && filtered_green<=3.402823466e38 {green=filtered_green;}
        let gratio=divide(multiply(coefficients[10],vec2(green,0.)),blue);
        let rcurve=nearest_curve(curves(gratio,RED_FROM_GREEN),ratio);
        let red=blend(scaled.x,scaled.z,rcurve,mean_r,0u,strength)/p.gains.x;
        if green>=0. && green<=3.402823466e38 {result.y=green;}
        if red>=0. && red<=3.402823466e38 {result.x=red;}
    }
    let i=(id.y*p.extent.x+id.x)*3u;
    destination[i]=result.x;destination[i+1u]=result.y;destination[i+2u]=result.z;
}
