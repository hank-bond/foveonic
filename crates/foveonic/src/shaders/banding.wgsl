// Native transposed smooth-surface fits, correlation rejection, row+1
// contribution traversal and WB-limited correction. Adapted with permission
// from Foveonic banding/patches.rs at e47af35. All image estimation is on GPU.
struct Params{source:vec4<u32>,area:vec4<u32>,values:vec4<f32>,gains:vec4<f32>};
struct Patch{geometry:vec4<u32>,storage:vec4<u32>};
struct Batch{value:vec4<u32>};
@group(0) @binding(0) var<storage,read> pixels:array<f32>;
@group(0) @binding(1) var<storage,read> patches:array<Patch>;
@group(0) @binding(2) var<uniform> params:Params;
@group(0) @binding(3) var<storage,read> tables:array<f32>;
@group(0) @binding(4) var<storage,read_write> coefficients:array<f32>;
@group(0) @binding(5) var<storage,read_write> residuals:array<f32>;
@group(0) @binding(6) var<storage,read_write> accepted:array<u32>;
@group(0) @binding(7) var<storage,read_write> means:array<vec4<f32>>;
@group(0) @binding(8) var<uniform> batch:Batch;
@group(0) @binding(9) var<storage,read> row_ranges:array<vec2<u32>>;
@group(0) @binding(10) var<storage,read> links:array<u32>;
@group(0) @binding(11) var<storage,read_write> offsets:array<vec4<f32>>;
@group(0) @binding(12) var<storage,read_write> output:array<f32>;
fn color(p:u32)->vec3<f32>{return vec3(pixels[p*3u],pixels[p*3u+1u],pixels[p*3u+2u])/params.values.x;}
fn coefficient(p:u32,k:u32)->vec3<f32>{let i=(p*6u+k)*3u;return vec3(coefficients[i],coefficients[i+1u],coefficients[i+2u]);}
fn residual(p:u32)->vec3<f32>{return vec3(residuals[p*3u],residuals[p*3u+1u],residuals[p*3u+2u]);}
@compute @workgroup_size(32)
fn fit(@builtin(global_invocation_id)id:vec3<u32>){
    let index=id.x;if index>=params.source.z{return;}let estimate=patches[index];let side=estimate.geometry.z;let n=estimate.geometry.w;
    // Reuse each normalized RGB load across independent coefficient recurrences.
    // Each coefficient still sees pixels in exactly the native x/y FMA order.
    var sums:array<vec3<f32>,6>;
    let start=estimate.storage.x+side*side*n;
    for(var x=0u;x<side;x++){for(var y=0u;y<side;y++){
        let p=(estimate.geometry.y+y)*params.source.x+estimate.geometry.x+x;let rgb=color(p);
        for(var k=0u;k<n;k++){let weight=tables[start+k*side*side+x*side+y];sums[k]=fma(vec3(weight),rgb,sums[k]);}
    }}
    for(var k=0u;k<n;k++){let destination=(index*6u+k)*3u;coefficients[destination]=sums[k].x;coefficients[destination+1u]=sums[k].y;coefficients[destination+2u]=sums[k].z;}
}
@compute @workgroup_size(64)
fn residual_field(@builtin(global_invocation_id)id:vec3<u32>){
    if id.y>=batch.value.y{return;}let index=batch.value.x+id.y;let estimate=patches[index];let side=estimate.geometry.z;let n=estimate.geometry.w;
    if id.x>=side*side{return;}let x=id.x%side;let y=id.x/side;var predicted=vec3(0.0);
    for(var k=0u;k<n;k++){let weight=tables[estimate.storage.x+(x*side+y)*n+k];predicted=fma(vec3(weight),coefficient(index,k),predicted);}
    let p=(estimate.geometry.y+y)*params.source.x+estimate.geometry.x+x;let value=color(p)-predicted;let q=(estimate.storage.y-patches[batch.value.x].storage.y+x*side+y)*3u;
    residuals[q]=value.x;residuals[q+1u]=value.y;residuals[q+2u]=value.z;
}
// Compensated float32 sums approximate the reference's double statistics while
// coefficient fitting and row means preserve its ordered float32 FMA/additions.
fn kahan(value:vec3<f32>,sum:ptr<function,vec3<f32>>,error:ptr<function,vec3<f32>>){
    let corrected=value-*error;let total=*sum+corrected;*error=(total-*sum)-corrected;*sum=total;
}
@compute @workgroup_size(32)
fn quality(@builtin(global_invocation_id)id:vec3<u32>){
    if id.x>=batch.value.y{return;}let index=batch.value.x+id.x;let estimate=patches[index];let side=estimate.geometry.z;let area=f32(side*side);let start=estimate.storage.y-patches[batch.value.x].storage.y;
    var original_sum=vec3(0.0);var original_error=vec3(0.0);var sum=vec3(0.0);var sum_error=vec3(0.0);
    for(var x=0u;x<side;x++){for(var y=0u;y<side;y++){
        let p=(estimate.geometry.y+y)*params.source.x+estimate.geometry.x+x;kahan(color(p),&original_sum,&original_error);
        kahan(residual(start+x*side+y),&sum,&sum_error);
    }}
    let mean=sum/area;var variance=vec3(0.0);var variance_error=vec3(0.0);var covariance=vec3(0.0);var covariance_error=vec3(0.0);
    for(var x=0u;x<side;x++){for(var y=0u;y<side;y++){
        let delta=residual(start+x*side+y)-mean;kahan(delta*delta,&variance,&variance_error);
        kahan(vec3(delta.y*delta.x,delta.x*delta.z,delta.z*delta.y),&covariance,&covariance_error);
    }}
    let deviation=sqrt(variance);let ratios=abs(covariance)/vec3(deviation.x*deviation.y,deviation.x*deviation.z,deviation.y*deviation.z);
    let other=select(ratios.y,ratios.z,ratios.z>ratios.y);let correlation=select(other,ratios.x,ratios.x>other);
    let limit=select(0.06,0.1,all(original_sum/area>vec3(600.0)));
    accepted[index]=select(1u,0u,correlation!=correlation || correlation>=limit);
}
@compute @workgroup_size(32)
fn row_means(@builtin(global_invocation_id)id:vec3<u32>){
    if id.y>=batch.value.y{return;}let index=batch.value.x+id.y;let estimate=patches[index];let side=estimate.geometry.z;let y=id.x;if y>=side{return;}
    var sum=vec3(0.0);let start=estimate.storage.y-patches[batch.value.x].storage.y;
    if accepted[index]!=0u{for(var x=0u;x<side;x++){sum+=residual(start+x*side+y);}}
    means[estimate.storage.z+y]=vec4(sum/f32(side),f32(accepted[index]));
}
@compute @workgroup_size(64)
fn reduce_rows(@builtin(global_invocation_id)id:vec3<u32>){
    let row=id.x;if row>=params.source.y{return;}let range=row_ranges[row];var sum=vec3(0.0);var count=0u;
    for(var i=0u;i<range.y;i++){let value=means[links[range.x+i]];sum+=value.xyz;count=(count+u32(value.w))&65535u;}
    let denominator=max(count,1u);var bias=vec3(0.0);
    if denominator>=20u{let mean=sum/f32(denominator);bias=select(mean,vec3(0.0),abs(mean)>params.gains.xyz*params.values.y);}
    offsets[row]=vec4(bias*params.values.x,f32(count));
}
@compute @workgroup_size(8,8)
fn apply(@builtin(global_invocation_id)id:vec3<u32>){
    let xy=id.xy;if xy.x>=params.source.x || xy.y>=params.source.y{return;}let p=xy.y*params.source.x+xy.x;var bias=vec3(0.0);
    // Preserve native asymmetric consumption, not a photographic crop fix:
    // row y reads accumulation y+1; columns end at active width, not right.
    if xy.y<params.area.w-params.area.y && xy.x>=params.area.x && xy.x<params.area.z-params.area.x{bias=offsets[xy.y+1u].xyz;}
    output[p*3u]=pixels[p*3u]-bias.x;output[p*3u+1u]=pixels[p*3u+1u]-bias.y;output[p*3u+2u]=pixels[p*3u+2u]-bias.z;
}
