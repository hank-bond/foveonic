struct Params {
    shape: vec4<u32>, offsets: vec4<u32>, controls: vec4<f32>,
    noise_rgb: array<vec4<f32>,3>, noise_blue: vec4<f32>,
}
struct Feature { value: array<f32,6> }
@group(0) @binding(0) var<storage,read> input_rgb: array<f32>;
@group(0) @binding(1) var<storage,read> input_blue: array<f32>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage,read_write> rgb: array<f32>;
@group(0) @binding(4) var<storage,read_write> blue: array<f32>;
@group(0) @binding(5) var<storage,read_write> defects: array<u32>;
struct Scratch { eligible: atomic<u32>, values: array<f32> }
@group(0) @binding(6) var<storage,read_write> scratch: Scratch;
@group(0) @binding(7) var<storage,read> response: array<f32>;
@group(0) @binding(8) var<storage,read_write> final_blue: array<f32>;

fn plus(a:f32,b:f32)->f32 { return fma(1.,a,b); }
fn minus(a:f32,b:f32)->f32 { return fma(-1.,b,a); }
fn product(a:f32,b:f32)->f32 { return fma(a,b,0.); }
// Metal division may differ by one ULP. A fused residual restores the native
// rounded quotient. Preserve signed zero and nonfinite results before refining.
fn divide(a:f32,b:f32)->f32 {
    let q=a/b;
    if a==0. || b==0. || q!=q || abs(q)>3.402823e38 {return q;}
    return plus(q,fma(-q,b,a)/b);
}
fn point(x:u32,y:u32)->u32 { return y*params.shape.x+x; }
fn phase_position(p:vec2<u32>,phase:u32)->u32 { return (p.y*2u+phase/2u)*params.shape.x*2u+p.x*2u+phase%2u; }
fn selected(p:vec2<u32>)->bool { return p.x>=16u && p.x+1u<params.shape.x && p.y>=4u && p.x%16u==0u && p.y%4u==0u; }
fn interior(p:vec2<u32>)->bool { return p.x>0u && p.y>0u && p.x+1u<params.shape.x && p.y+1u<params.shape.y; }
fn current_rgb(i:u32)->vec3<f32> { return vec3(rgb[i*3u],rgb[i*3u+1u],rgb[i*3u+2u]); }
fn store_rgb(i:u32,v:vec3<f32>) { rgb[i*3u]=v.x;rgb[i*3u+1u]=v.y;rgb[i*3u+2u]=v.z; }
fn feature(p:vec2<u32>,snapshot:bool)->Feature {
    let i=point(p.x,p.y);var result:Feature;result.value[0]=rgb[i*3u];result.value[1]=rgb[i*3u+1u];
    for(var k=0u;k<4u;k++) {
        if snapshot { result.value[k+2u]=scratch.values[k*params.offsets.x+i]; }
        else { result.value[k+2u]=input_blue[phase_position(p,k)]; }
    }
    return result;
}
fn sigma(v:f32,coefficient:vec4<f32>,absolute:bool)->f32 {
    return sqrt(abs(fma(product(v,v),coefficient.z,fma(select(v,abs(v),absolute),coefficient.y,coefficient.x))));
}
fn range_clamp(v:f32,low:f32,high:f32,tolerance:f32)->f32 {
    if minus(v,high)>tolerance { return high; }
    if minus(low,v)>tolerance { return low; }
    return v;
}
fn saturation(i:u32)->bool {
    if i>=params.offsets.x { return false; }
    let threshold=14300./params.controls.x;
    return input_rgb[i*3u]>=threshold || input_rgb[i*3u+1u]>=threshold || input_rgb[i*3u+2u]>=threshold;
}
fn area(p:vec2<u32>)->bool {
    if params.shape.w!=0u { return false; }
    let i=point(p.x,p.y);
    if saturation(i) { return true; }
    if !interior(p) { return false; }
    for(var dy=-1;dy<=2;dy++) { for(var dx=-1;dx<=2;dx++) {
        if saturation(u32((i32(p.y)+dy)*i32(params.shape.x)+i32(p.x)+dx)) { return true; }
    }}
    return false;
}
fn hot_pixel(p:vec2<u32>) {
    if !selected(p) || !interior(p) { return; }
    let i=point(p.x,p.y);
    var low_rg=vec2<f32>(3.402823e38);var high_rg=vec2<f32>(-3.402823e38);
    var low_blue=vec4<f32>(3.402823e38);var high_blue=vec4<f32>(-3.402823e38);
    for(var dy=-1;dy<=1;dy++) { for(var dx=-1;dx<=1;dx++) { if dx==0 && dy==0 { continue; }
        let q=vec2<u32>(vec2<i32>(p)+vec2(dx,dy));let j=point(q.x,q.y);
        let rg=vec2(input_rgb[j*3u],input_rgb[j*3u+1u]);low_rg=min(low_rg,rg);high_rg=max(high_rg,rg);
        var b:vec4<f32>;for(var k=0u;k<4u;k++) { b[k]=input_blue[phase_position(q,k)]; }
        low_blue=min(low_blue,b);high_blue=max(high_blue,b);
    }}
    let original=vec2(input_rgb[i*3u],input_rgb[i*3u+1u]);var values:vec4<f32>;var peaks=0u;
    for(var k=0u;k<4u;k++) { values[k]=input_blue[phase_position(p,k)];peaks+=u32(values[k]>high_blue[k]); }
    scratch.values[params.offsets.x*4u+i]=f32(u32(all(original>high_rg) && peaks>=3u));
    var corrected:vec4<f32>;
    for(var k=0u;k<4u;k++) { corrected[k]=range_clamp(values[k],low_blue[k],high_blue[k],sigma(values[k],params.noise_blue,true)); }
    let saturated=area(p);
    if params.shape.w!=0u && saturated { corrected=high_blue;rgb[i*3u]=high_rg.x;rgb[i*3u+1u]=high_rg.y; }
    else if !saturated {
        rgb[i*3u]=range_clamp(original.x,low_rg.x,high_rg.x,sigma(original.x,params.noise_rgb[0],false));
        rgb[i*3u+1u]=range_clamp(original.y,low_rg.y,high_rg.y,sigma(original.y,params.noise_rgb[1],false));
    }
    if params.shape.w!=0u || !saturated { rgb[i*3u+2u]=product(plus(plus(plus(corrected.x,corrected.y),corrected.z),corrected.w),.25); }
    for(var k=0u;k<4u;k++) { blue[phase_position(p,k)]=corrected[k]; }
}
fn fill_mask(p:vec2<u32>) {
    let n=params.offsets.x;let i=point(p.x,p.y);var mask=u32(scratch.values[n*4u+i]);
    if p.y>=4u && p.y+4u<params.shape.y && p.x>0u && p.x+1u<params.shape.x && selected(p) {
        var count=0u;
        for(var dy=-16;dy<=16;dy+=4) { for(var dx=-16;dx<=16;dx+=16) {
            // The source casts to unsigned before clamping negative coordinates.
            let q=min(vec2<u32>(vec2<i32>(p)+vec2(dx,dy)),params.shape.xy-vec2(1u));
            count+=u32(scratch.values[n*4u+point(q.x,q.y)])&1u;
        }}
        if count>=2u { mask|=4u; }if count>=7u { mask|=2u; }
    }
    defects[i]=mask;
}
fn is_defect(p:vec2<u32>)->bool { return selected(p) && (defects[point(p.x,p.y)]&3u)!=0u; }
fn distance(a:Feature,b:Feature)->f32 {
    var delta:array<f32,6>;for(var k=0u;k<6u;k++) {delta[k]=minus(a.value[k],b.value[k]);}
    var sum=fma(delta[0],delta[0],product(delta[1],delta[1]));
    for(var k=2u;k<6u;k++) {sum=fma(delta[k],delta[k],sum);}
    return sum;
}
fn count_eligible(p:vec2<u32>) {
    if p.x<20u || p.y<20u || p.x+20u>=params.shape.x || p.y+20u>=params.shape.y || !is_defect(p) {return;}
    var neighbors=0u;
    for(var dy=-20;dy<=20;dy+=4) {for(var dx=-16;dx<=16;dx+=16) {
        neighbors+=u32(is_defect(vec2<u32>(vec2<i32>(p)+vec2(dx,dy))));
    }}
    if neighbors>5u {atomicAdd(&scratch.eligible,1u);}
}
fn ghost_interpolate(p:vec2<u32>) {
    if atomicLoad(&scratch.eligible)<=99u || !is_defect(p) {return;}
    // Sparse centers are separated by four rows and sixteen columns. Their
    // cardinal stores never intersect another center's radius-two reads.
    let cardinal=array<vec2<i32>,4>(vec2(0,-1),vec2(-1,0),vec2(1,0),vec2(0,1));
    let candidates=array<vec2<i32>,12>(vec2(0,-2),vec2(1,-1),vec2(-1,-1),vec2(-2,0),vec2(-1,1),vec2(-1,-1),vec2(2,0),vec2(1,1),vec2(1,-1),vec2(0,2),vec2(1,1),vec2(-1,1));
        for(var direction=0u;direction<4u;direction++) {
            let neighbor_position=vec2<u32>(vec2<i32>(p)+cardinal[direction]);let center=feature(neighbor_position,false);
            var samples:array<Feature,3>;var weights:vec3<f32>;
            for(var c=0u;c<3u;c++) {samples[c]=feature(vec2<u32>(vec2<i32>(p)+candidates[direction*3u+c]),false);weights[c]=divide(1.,plus(distance(center,samples[c]),1.));}
            let total=plus(weights.x,plus(weights.y,weights.z));var output:vec3<f32>;
            for(var k=0u;k<2u;k++) {output[k]=divide(plus(product(samples[0].value[k],weights.x),plus(product(samples[1].value[k],weights.y),product(samples[2].value[k],weights.z))),total);}
            var values:vec4<f32>;for(var k=0u;k<4u;k++) {
                values[k]=divide(fma(samples[0].value[k+2u],weights.x,fma(samples[2].value[k+2u],weights.z,product(samples[1].value[k+2u],weights.y))),total);
                blue[phase_position(neighbor_position,k)]=values[k];
            }
            output.z=product(plus(plus(plus(values.x,values.y),values.z),values.w),.25);store_rgb(point(neighbor_position.x,neighbor_position.y),output);
        }
}
fn rg_energy(a:Feature,b:Feature)->f32 {let r=minus(a.value[0],b.value[0]);let g=minus(a.value[1],b.value[1]);return fma(r,r,product(g,g));}
fn directional_energy(p:vec2<u32>,horizontal:bool)->f32 {
    var starts=array<vec2<i32>,4>(vec2(-1,0),vec2(-1,0),vec2(1,0),vec2(1,0));
    var ends=array<vec2<i32>,4>(vec2(-1,-1),vec2(-1,1),vec2(1,-1),vec2(1,1));
    if horizontal {starts=array<vec2<i32>,4>(vec2(0,-1),vec2(0,-1),vec2(0,1),vec2(0,1));ends=array<vec2<i32>,4>(vec2(-1,-1),vec2(1,-1),vec2(-1,1),vec2(1,1));}
    var a:array<Feature,4>;var b:array<Feature,4>;
    for(var edge=0u;edge<4u;edge++) {a[edge]=feature(vec2<u32>(vec2<i32>(p)+starts[edge]),true);b[edge]=feature(vec2<u32>(vec2<i32>(p)+ends[edge]),true);}
    var sum=plus(rg_energy(a[0],b[0]),rg_energy(a[1],b[1]));
    for(var k=2u;k<6u;k++) {for(var edge=0u;edge<2u;edge++) {let d=minus(a[edge].value[k],b[edge].value[k]);sum=fma(d,d,sum);}}
    for(var edge=2u;edge<4u;edge++) {sum=plus(sum,rg_energy(a[edge],b[edge]));}
    for(var k=2u;k<6u;k++) {for(var edge=2u;edge<4u;edge++) {let d=minus(a[edge].value[k],b[edge].value[k]);sum=fma(d,d,sum);}}
    return sum;
}
fn directional_clamp(p:vec2<u32>) {
    if atomicLoad(&scratch.eligible)<=99u || !is_defect(p) {return;}
    let vertical=directional_energy(p,false);let horizontal=directional_energy(p,true);
    var delta=vec2<u32>(0,1);if horizontal<=vertical {delta=vec2(1,0);}
    let a=p-delta;let b=p+delta;let i=point(p.x,p.y);
    store_rgb(i,min(current_rgb(i),product_vec(plus_vec(current_rgb(point(a.x,a.y)),current_rgb(point(b.x,b.y))),.5)));
    var values:vec4<f32>;for(var k=0u;k<4u;k++) {
        values[k]=min(scratch.values[k*params.offsets.x+i],product(plus(scratch.values[k*params.offsets.x+point(a.x,a.y)],scratch.values[k*params.offsets.x+point(b.x,b.y)]),.5));blue[phase_position(p,k)]=values[k];
    }
    rgb[i*3u+2u]=product(plus(plus(plus(values.x,values.y),values.z),values.w),.25);
}
fn plus_vec(a:vec3<f32>,b:vec3<f32>)->vec3<f32> {return fma(vec3(1.),a,b);}
fn product_vec(a:vec3<f32>,b:f32)->vec3<f32> {return fma(a,vec3(b),vec3(0.));}
fn density_coordinates(p:vec2<u32>)->vec2<u32> {return vec2(clamp((p.x/16u)*16u,32u,params.shape.x-32u),clamp((p.y/4u)*4u,8u,params.shape.y-8u));}
fn density_amount(density:vec4<f32>,p:vec2<u32>)->f32 {
    let c=density_coordinates(p);var a=density.x;var lower=density.z;
    if p.x>=c.x {let t=f32(p.x-c.x)/16.;let s=f32(c.x-p.x+16u)/16.;a=fma(s,a,product(t,density.y));lower=fma(s,lower,product(t,density.w));}
    if p.y>=c.y {a=fma(f32(c.y-p.y+4u)/4.,a,product(f32(p.y-c.y)/4.,lower));}
    return divide(a,9.);
}
// Pattern-mask writes contain 253 or zero. Both clear bit one, which is
// the only bit consulted by later density reads. Reconstruct that ordered bit
// from the original mask and the first pixel of the cached cell.
fn clears_density_bit(p:vec2<u32>)->bool {
    if !selected(p) || !interior(p) || p.x<48u || p.x>params.shape.x-32u {return false;}
    let c=density_coordinates(p);
    return c.y%4u==0u && p.y+4u>=c.y && p.y<=c.y+4u;
}
fn original_density_bit(p:vec2<u32>,event:vec2<u32>)->u32 {
    let original=u32(scratch.values[params.offsets.x*4u+point(p.x,p.y)]);
    let already_processed=p.y<event.y || (p.y==event.y && p.x<event.x);
    if already_processed && clears_density_bit(p) {return 0u;}
    return (original>>1u)&1u;
}
fn density_bucket(bucket:u32,y:u32) {
    let width=params.shape.x;let height=params.shape.y;let buckets=params.offsets.w;
    if bucket>=buckets || y==0u || y+1u>=height {return;}
    let representative=select(bucket*16u,1u,bucket==0u);
    if representative+1u>=width {return;}
    let c=density_coordinates(vec2(representative,y));
    let event=vec2(select(((c.x+15u)/16u)*16u,1u,c.x==32u),y);
    var cells:array<u32,16>;
    for(var row=0u;row<4u;row++) {for(var col=0u;col<4u;col++) {
        let q=min(c+vec2(col*16u-16u,row*4u-4u),params.shape.xy-vec2(1u));
        cells[row*4u+col]=original_density_bit(q,event);
    }}
    var density=vec4<f32>(0.);var footprint_enabled=false;
    for(var row=0u;row<3u;row++) {for(var col=0u;col<3u;col++) {footprint_enabled=footprint_enabled || cells[row*4u+col]!=0u;}}
    for(var k=0u;k<4u;k++) {var total=0u;for(var row=0u;row<3u;row++) {for(var col=0u;col<3u;col++) {total+=cells[(row+k/2u)*4u+col+k%2u];}}density[k]=f32(total);}
    let offset=params.offsets.y+(y*buckets+bucket)*4u;for(var k=0u;k<4u;k++) {scratch.values[offset+k]=density[k];}
    var first_bucket=bucket==0u;
    if bucket>0u {first_bucket=c.x!=density_coordinates(vec2((bucket-1u)*16u,y)).x;}
    if first_bucket && footprint_enabled && density_amount(density,event)>0. {
        let i=point(event.x,event.y);let original=u32(scratch.values[params.offsets.x*4u+i]);
        defects[i]=u32(product(product(minus(1.,params.controls.z),f32((original>>2u)&1u)),255.))&255u;
    }
}

fn high_pass(block:array<f32,9>)->f32 {var value=0.;for(var k=0u;k<4u;k++) {value=fma(block[k],-.125,value);}value=plus(value,block[4]);for(var k=5u;k<9u;k++) {value=fma(block[k],-.125,value);}return value;}
fn sorted(block:array<f32,9>)->array<f32,9> {var values=block;for(var i=1u;i<9u;i++) {let value=values[i];var j=i;loop {if j==0u || values[j-1u]>=value {break;}values[j]=values[j-1u];j--;}values[j]=value;}return values;}
fn phase_pattern(p:vec2<u32>) {
    if !interior(p) {return;}let offset=params.offsets.y+(p.y*params.offsets.w+p.x/16u)*4u;
    let amount=density_amount(vec4(scratch.values[offset],scratch.values[offset+1u],scratch.values[offset+2u],scratch.values[offset+3u]),p);if amount<=0. {return;}
    var blocks:array<array<f32,9>,7>;
    for(var dy=-1;dy<=1;dy++) {for(var dx=-1;dx<=1;dx++) {let q=vec2<u32>(vec2<i32>(p)+vec2(dx,dy));let i=point(q.x,q.y);let k=u32((dy+1)*3+dx+1);for(var channel=0u;channel<4u;channel++) {blocks[channel][k]=scratch.values[channel*params.offsets.x+i];}for(var channel=0u;channel<3u;channel++) {blocks[channel+4u][k]=rgb[i*3u+channel];}}}
    var ratios:array<f32,7>;for(var k=0u;k<7u;k++) {ratios[k]=divide(high_pass(blocks[k]),plus(blocks[k][4],.000001));}
    var square=product(ratios[0],ratios[0]);for(var k=1u;k<7u;k++) {square=fma(ratios[k],ratios[k],square);}
    let index=u32(round(clamp(product(product(square,1023.),1./3.),0.,1023.)));let weight=response[index];
    // The source retains five middle values but scales their sum by one-third.
    // That shared scale participates in the phase ratio and must retain rounding.
    var means:vec4<f32>;
    for(var k=0u;k<4u;k++) {let ordered=sorted(blocks[k]);let trim=plus(plus(plus(ordered[0],ordered[1]),ordered[7]),ordered[8]);var sum=0.;for(var j=0u;j<9u;j++) {sum=plus(sum,blocks[k][j]);}means[k]=product(minus(sum,trim),1./3.);}
    let alpha=min(1.,product(amount,params.controls.y));let other=minus(1.,alpha);
    for(var k=1u;k<4u;k++) {let value=blocks[k][4];let scaled=divide(product(means[0],value),plus(means[k],.00001));let corrected=fma(weight,scaled,product(value,minus(1.,weight)));let i=phase_position(p,k);blue[i]=fma(other,blue[i],product(alpha,corrected));}
}
fn median_blue(p:vec2<u32>) {
    if !interior(p) {return;}var block:array<f32,9>;
    for(var dy=-1;dy<=1;dy++) {for(var dx=-1;dx<=1;dx++) {let q=vec2<u32>(vec2<i32>(p)+vec2(dx,dy));var value=0.;for(var k=0u;k<4u;k++) {value=plus(value,product(blue[phase_position(q,k)],.25));}block[u32((dy+1)*3+dx+1)]=value;}}
    scratch.values[params.offsets.x*5u+point(p.x,p.y)]=sorted(block)[4];
}
fn smooth_blue(p:vec2<u32>) {
    var alpha=0.;if interior(p) {var block:array<f32,9>;for(var dy=-1;dy<=1;dy++) {for(var dx=-1;dx<=1;dx++) {let q=vec2<u32>(vec2<i32>(p)+vec2(dx,dy));block[u32((dy+1)*3+dx+1)]=scratch.values[params.offsets.x*5u+point(q.x,q.y)];}}alpha=min(1.,divide(abs(high_pass(block)),200.));}
    let other=minus(1.,alpha);for(var k=0u;k<4u;k++) {let i=phase_position(p,k);if interior(p) {final_blue[i]=fma(other,blue[i],product(alpha,scratch.values[k*params.offsets.x+point(p.x,p.y)]));}else {final_blue[i]=blue[i];}}
}
@compute @workgroup_size(8,8)
fn sparse_repair(@builtin(global_invocation_id) id:vec3<u32>) {
    let phase=params.shape.z;
    if phase==7u {density_bucket(id.x,id.y);return;}
    if any(id.xy>=params.shape.xy) {return;}let p=id.xy;let i=point(p.x,p.y);let n=params.offsets.x;
    if phase==0u {if i==0u {atomicStore(&scratch.eligible,0u);}for(var k=0u;k<3u;k++) {rgb[i*3u+k]=input_rgb[i*3u+k];}for(var k=0u;k<4u;k++) {let j=phase_position(p,k);blue[j]=input_blue[j];}scratch.values[n*4u+i]=0.;scratch.values[n*5u+i]=0.;defects[i]=0u;}
    else if phase==1u {hot_pixel(p);}
    else if phase==2u {fill_mask(p);}
    else if phase==3u {count_eligible(p);}
    else if phase==4u {ghost_interpolate(p);}
    else if phase==5u || phase==8u {for(var k=0u;k<4u;k++) {scratch.values[k*n+i]=blue[phase_position(p,k)];}if phase==5u {scratch.values[n*4u+i]=f32(defects[i]);}}
    else if phase==6u {directional_clamp(p);}
    else if phase==9u {phase_pattern(p);}
    else if phase==10u {median_blue(p);}
    else if phase==11u {smooth_blue(p);}
}
