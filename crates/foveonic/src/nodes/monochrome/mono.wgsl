// Dedicated sensor-layer monochrome processing. Adapted from the reference implementation.
struct Parameters { source:vec4<u32>, mix:vec4<f32>, filtering:vec4<f32>, area:vec4<u32>, detail:vec4<f32> }
@group(0) @binding(0) var<storage,read> input_pixels:array<f32>;
@group(0) @binding(1) var<storage,read> neighbor_pixels:array<f32>;
@group(0) @binding(2) var<uniform> params:Parameters;
@group(0) @binding(3) var<storage,read_write> output_pixels:array<f32>;
@group(0) @binding(4) var<storage,read> indexes:array<u32>;
@group(0) @binding(5) var<storage,read> weights:array<f32>;
fn valid(id:vec3<u32>)->bool{return id.x<params.source.x && id.y<params.source.y;}
// The noise model has 1024 four-code bins. Recovered highlight headroom
// lies outside that model and must not become an undifferentiated last-bin mean.
const HISTOGRAM_MAX_CODE: f32 = 4095.;
fn bin(value:f32)->u32{return u32(clamp(i32(value),0,i32(HISTOGRAM_MAX_CODE)))>>2u;}
fn histogram_value(value:f32)->f32{return min(value,HISTOGRAM_MAX_CODE);}
@compute @workgroup_size(8,8)
fn mixing(@builtin(global_invocation_id)id:vec3<u32>){
    if !valid(id){return;}let p=id.y*params.source.x+id.x;
    var value=vec3(input_pixels[p*3u],input_pixels[p*3u+1u],input_pixels[p*3u+2u]);
    if id.x<params.area.x || id.x>=params.area.z || id.y<params.area.y || id.y>=params.area.w{value=vec3(1000.0);}
    value=clamp(value,vec3(0.0),vec3(32767.0));
    output_pixels[p]=fma(value.z,params.mix.z,fma(value.x,params.mix.x,value.y*params.mix.y));
}
@compute @workgroup_size(8,8)
fn directional(@builtin(global_invocation_id)id:vec3<u32>){
    if !valid(id){return;}let w=params.source.x;let p=id.y*w+id.x;let radius=params.source.z/2u;
    var value=0.0;
    if id.x>=radius && id.x+radius<w {
        let center=input_pixels[p];let level=u32(clamp(i32(center),0,4095));let offset=indexes[level*3u];let count=indexes[level*3u+2u];
        var sum=0.0;var total=0.0;
        for(var dx=-i32(radius);dx<i32(radius);dx++){
            let sample=input_pixels[u32(i32(p)+dx)];let distance=u32(abs(center-sample));
            if distance<count{let weight=weights[offset+distance];sum=fma(sample,weight,sum);total+=weight;}
        }
        value=sum/total;
    }
    output_pixels[p]=value;
}
@compute @workgroup_size(64)
fn column(@builtin(global_invocation_id)id:vec3<u32>){
    let w=params.source.x;let h=params.source.y;if id.x>=w{return;}
    let rows=h/params.source.w;let blocks=(rows-1u)/40u;
    let strip=id.y/blocks;let block=id.y%blocks;if strip>=params.source.w{return;}
    let start=strip*rows+block*40u;var end=start+40u;if block+1u==blocks{end=(strip+1u)*rows;}
    var original=0.0;var filtered=0.0;
    for(var y=start;y<end;y++){original+=input_pixels[y*w+id.x];filtered+=neighbor_pixels[y*w+id.x];}
    let denominator=f32(end-start+1u);let delta=original/denominator-filtered/denominator;
    let radius=(params.source.z-1u)/2u;
    for(var y=start;y<end;y++){
        let p=y*w+id.x;var value=input_pixels[p];
        if id.x>=radius && id.x+radius<w{let mask=select(0.0,1.0,abs(params.filtering.z*value)>abs(delta));value=clamp(fma(-mask,delta,value),0.0,32766.0);}
        output_pixels[p]=value;
    }
}
// Each lane owns one column's bins in bin-major GPU scratch. Neighboring lanes
// can access neighboring columns together without a large private histogram or
// an 8 KiB workgroup allocation for just one active lane. Reuse bins across the
// fixed strips, clearing them before each strip. Every Float32 recurrence stays ordered.
struct HistogramBin { count:u32, sum:f32 }
@group(0) @binding(6) var<storage,read_write> histogram_bins:array<HistogramBin>;
@compute @workgroup_size(64)
fn histogram(@builtin(global_invocation_id)id:vec3<u32>){
    let w=params.source.x;let h=params.source.y;let columns=w-18u;if id.x>=columns{return;}let x=id.x+9u;
    let rows=h/params.source.w;let down=id.x%2u==0u;
    for(var strip=0u;strip<params.source.w;strip++){
        let first=max(strip*rows,9u);let last=min((strip+1u)*rows-1u,h-10u);if first>last{continue;}
        var y=select(last,first,down);
        for(var k=0u;k<1024u;k++){let i=k*columns+id.x;histogram_bins[i].count=0u;histogram_bins[i].sum=0.0;}
        for(var yy=y-9u;yy<=y+9u;yy++){for(var xx=x-9u;xx<=x+9u;xx++){
            let sample=histogram_value(input_pixels[yy*w+xx]);let i=bin(sample)*columns+id.x;histogram_bins[i].sum=sample+histogram_bins[i].sum;histogram_bins[i].count+=1u;
        }}
        loop{
            let center=bin(input_pixels[y*w+x]);let offset=indexes[center*3u];let first_bin=indexes[center*3u+1u];let count=indexes[center*3u+2u];
            var total=0.0;var weight_sum=0.0;
            for(var k=0u;k<count;k++){let i=(first_bin+k)*columns+id.x;if histogram_bins[i].count!=0u{let weight=weights[offset+k];total=fma(weight,histogram_bins[i].sum,total);weight_sum=fma(weight,f32(histogram_bins[i].count),weight_sum);}}
            // Filter the modeled base and carry each pixel's excess signal through.
            // Bounding sums as well as bin indexes prevents bright neighbors
            // from contaminating pixels just below the histogram ceiling.
            output_pixels[y*w+x]=total/weight_sum+max(input_pixels[y*w+x]-HISTOGRAM_MAX_CODE,0.);
            if (down && y==last) || (!down && y==first){break;}
            let leaving=select(y+9u,y-9u,down);let entering=select(y-10u,y+10u,down);
            for(var xx=x-9u;xx<=x+9u;xx++){let sample=histogram_value(input_pixels[leaving*w+xx]);let i=bin(sample)*columns+id.x;histogram_bins[i].sum-=sample;histogram_bins[i].count-=1u;}
            for(var xx=x-9u;xx<=x+9u;xx++){let sample=histogram_value(input_pixels[entering*w+xx]);let i=bin(sample)*columns+id.x;histogram_bins[i].sum=sample+histogram_bins[i].sum;histogram_bins[i].count+=1u;}
            y=u32(i32(y)+select(-1,1,down));
        }
    }
}
fn bounds(x:u32,y:u32)->vec2<f32>{let w=params.source.x;let a=neighbor_pixels[(y-1u)*w+x];let b=neighbor_pixels[y*w+x];let c=neighbor_pixels[(y+1u)*w+x];return vec2(max(c,max(b,a)),min(c,min(b,a)));}
@compute @workgroup_size(8,8)
fn despeckle(@builtin(global_invocation_id)id:vec3<u32>){
    if !valid(id){return;}let w=params.source.x;let h=params.source.y;let p=id.y*w+id.x;var value=input_pixels[p];
    if id.x>=2u && id.x+3u<w && id.y>=2u && id.y+2u<h{
        let x=id.x;let y=id.y;var left_x=x-1u;var middle_x=x;if x<=3u{left_x=x-2u;}if x==2u{middle_x=x-1u;}
        let left=bounds(left_x,y);let middle=bounds(middle_x,y);let right=bounds(x+1u,y);
        let center=neighbor_pixels[p];let above=neighbor_pixels[p-w];let below=neighbor_pixels[p+w];
        var corrected=false;
        if center==middle.x && (center>right.x || center>left.x){let bound=max(below,max(above,max(right.x,left.x)));if center-bound>params.filtering.w{value=bound;corrected=true;}}
        if !corrected && center==middle.y && (center<right.y || center<left.y){let bound=min(below,min(above,min(right.y,left.y)));if center-bound<params.filtering.w{value=bound;}}
    }
    output_pixels[p]=value;
}
@compute @workgroup_size(8,8)
fn signal(@builtin(global_invocation_id)id:vec3<u32>){
    if !valid(id){return;}let w=params.source.x;let h=params.source.y;let p=id.y*w+id.x;var value=0.0;
    if id.x>=1u && id.x+1u<w && id.y>=1u && id.y+1u<h{
        var sum=0.0;for(var y=-1;y<=1;y++){for(var x=-1;x<=1;x++){sum+=input_pixels[u32(i32(p)+y*i32(w)+x)];}}
        let center=input_pixels[p]*params.mix.w;
        value=(input_pixels[p]+sum/(-9.0))*params.mix.w;
        value=clamp(clamp(value,-params.detail.y,params.detail.y),-center*0.35,center*0.35);
    }
    output_pixels[p]=value;
}
@compute @workgroup_size(8,8)
fn expand(@builtin(global_invocation_id)id:vec3<u32>){
    if !valid(id){return;}let p=id.y*params.source.x+id.x;let value=input_pixels[p]*params.mix.w;
    output_pixels[p*3u]=value;output_pixels[p*3u+1u]=value;output_pixels[p*3u+2u]=value;
}
