struct Params {shape:vec4<u32>,controls:vec4<u32>}
@group(0) @binding(0) var<storage,read_write> pixels:array<f32>;
@group(0) @binding(1) var<storage,read> mask:array<u32>;
@group(0) @binding(2) var<uniform> p:Params;
@group(0) @binding(3) var<storage,read> records:array<vec4<u32>>;
@group(0) @binding(4) var<storage,read> noise:array<f32>;
@group(0) @binding(5) var<storage,read> matrix:array<vec2<f32>>;
const OFFSETS=array<vec2<i32>,8>(vec2(-1,-1),vec2(0,-1),vec2(1,-1),vec2(-1,0),vec2(1,0),vec2(-1,1),vec2(0,1),vec2(1,1));
fn load(x:u32,y:u32)->vec3<f32>{let i=(y*p.shape.x+x)*3u;return vec3(pixels[i],pixels[i+1u],pixels[i+2u]);}
fn store(x:u32,y:u32,v:vec3<f32>){let i=(y*p.shape.x+x)*3u;pixels[i]=v.x;pixels[i+1u]=v.y;pixels[i+2u]=v.z;}
fn tolerances(v:vec3<f32>)->vec3<f32>{var result:vec3<f32>;for(var c=0u;c<3u;c++){let k=c*3u;let square=fma(v[c],v[c],0.);result[c]=sqrt(fma(square,noise[k+2u],fma(abs(v[c]),noise[k+1u],noise[k])));}return result;}
fn selected_channels(bits:u32)->vec3<bool>{if bits==0u{return vec3(true);}return vec3((bits&4u)!=0u,(bits&2u)!=0u,(bits&1u)!=0u);}
fn finite(v:vec3<f32>)->bool{return all(v==v)&&all(abs(v)<=vec3(3.402823466e38));}
// High/low products preserve the color projection's native Double intermediates
// until final Float32 storage. This is the same arithmetic used by UV conversion.
fn product(a:f32,b:f32)->vec2<f32>{let high=fma(a,b,0.);return vec2(high,fma(a,b,-high));}
fn add_pair(a:vec2<f32>,b:vec2<f32>)->vec2<f32>{let sum=fma(1.,a.x,b.x);let vb=fma(-1.,a.x,sum);let ra=fma(-1.,fma(-1.,vb,sum),a.x);let rb=fma(-1.,vb,b.x);let error=fma(1.,fma(1.,ra,rb),fma(1.,a.y,b.y));let high=fma(1.,sum,error);return vec2(high,fma(-1.,fma(-1.,sum,high),error));}
fn multiply_pair(a:vec2<f32>,b:f32)->vec2<f32>{return add_pair(product(a.x,b),product(a.y,b));}
fn color(v:vec3<f32>,base:u32)->vec3<f32>{var result:vec3<f32>;for(var c=0u;c<3u;c++){let row=base+c*3u;let first=add_pair(multiply_pair(matrix[row+1u],v.y),multiply_pair(matrix[row],v.x));let total=add_pair(first,multiply_pair(matrix[row+2u],v.z));result[c]=fma(1.,total.x,total.y);}return result;}
fn neighbors(x:u32,y:u32)->array<vec3<f32>,8>{var n:array<vec3<f32>,8>;for(var i=0u;i<8u;i++){n[i]=load(u32(i32(x)+OFFSETS[i].x),u32(i32(y)+OFFSETS[i].y));}return n;}
fn combined(original:vec3<f32>,minimum:vec3<f32>,maximum:vec3<f32>,sigma:vec3<f32>,chosen:vec3<bool>)->bool{
    var first=true;var high=0.;var low=0.;var bound=0.;for(var c=0u;c<3u;c++){if chosen[c]{if first{high=original[c]-maximum[c];low=minimum[c]-original[c];bound=sigma[c];first=false;}else{high=(original[c]+high)-maximum[c];low=(minimum[c]+low)-original[c];bound=bound+sigma[c];}}}return high<bound&&low<bound;
}
@compute @workgroup_size(64)
fn chroma_single(@builtin(global_invocation_id) id:vec3<u32>){
    let offset=id.y*p.shape.z+id.x;
    if offset>=p.controls.w{return;}
    let r=p.controls.z+offset;
        let record=records[r];let y=record.x;let x=record.y;let flags=record.z;let bits=flags&255u;
        if y<=1u||y>=p.shape.y-1u||x<=1u||x>=p.shape.x-1u||bits!=255u||mask[y*p.shape.x+x]!=0u{return;}
        if p.controls.y!=0u&&p.shape.w>((flags>>8u)&15u){return;}
        let n=neighbors(x,y);var vertical=0.;var horizontal=0.;
        var vertical_channels:vec3<f32>;var horizontal_channels:vec3<f32>;
        // ARM uses two SIMD lanes plus the blue lane: accumulate four terms
        // independently per channel, then (red+green)+blue. Ties choose vertical.
        for(var c=0u;c<3u;c++){
            var v=fma(1.,abs(fma(-1.,n[0][c],n[3][c])),abs(fma(-1.,n[5][c],n[3][c])));
            v=fma(1.,abs(fma(-1.,n[2][c],n[4][c])),v);v=fma(1.,v,abs(fma(-1.,n[7][c],n[4][c])));vertical_channels[c]=v;
            var h=fma(1.,abs(fma(-1.,n[0][c],n[1][c])),abs(fma(-1.,n[2][c],n[1][c])));
            h=fma(1.,h,abs(fma(-1.,n[5][c],n[6][c])));h=fma(1.,h,abs(fma(-1.,n[7][c],n[6][c])));horizontal_channels[c]=h;
        }
        vertical=fma(1.,fma(1.,vertical_channels.x,vertical_channels.y),vertical_channels.z);
        horizontal=fma(1.,fma(1.,horizontal_channels.x,horizontal_channels.y),horizontal_channels.z);
        var direction=(n[3]+n[4])*0.5;if vertical<=horizontal{direction=(n[1]+n[6])*0.5;}
        let original=load(x,y);let sigma=tolerances(original)*3.;var minimum=n[0];var maximum=n[0];for(var i=1u;i<8u;i++){minimum=min(minimum,n[i]);maximum=max(maximum,n[i]);}
        let channels=(flags>>12u)&7u;let chosen=selected_channels(channels);let count=u32(chosen.x)+u32(chosen.y)+u32(chosen.z);var result=direction;
        if count==1u {for(var c=0u;c<3u;c++){if chosen[c]{var other:array<u32,2>;var j=0u;for(var k=0u;k<3u;k++){if k!=c{other[j]=k;j++;}}result=original;result[c]=((original[other[0]]+(direction[c]-direction[other[0]]))+(original[other[1]]+(direction[c]-direction[other[1]])))*0.5;}}}
        if count==2u {result=original;var reference=0u;for(var c=0u;c<3u;c++){if !chosen[c]{reference=c;}}for(var c=0u;c<3u;c++){if chosen[c]{result[c]=original[reference]+(direction[c]-direction[reference]);}}}
        let force=p.controls.x!=0u&&(channels==0u||channels==1u||channels==3u||channels==5u||channels==7u);
        if combined(original,minimum,maximum,sigma,chosen)&&!force{return;}
        var projected=color(result,0u);var lower=color(n[0],0u);var upper=lower;for(var i=1u;i<8u;i++){let v=color(n[i],0u);lower=min(lower,v);upper=max(upper,v);}
        projected.y=clamp(projected.y,lower.y,upper.y);projected.z=clamp(projected.z,lower.z,upper.z);result=color(projected,9u);if finite(result){store(x,y,result);}
}
@compute @workgroup_size(64)
fn chroma_general(@builtin(global_invocation_id) id:vec3<u32>){
    let offset=id.y*p.shape.z+id.x;
    if offset>=p.controls.w{return;}
    let r=p.controls.z+offset;
        let record=records[r];let y=record.x;let x=record.y;let flags=record.z;let bits=flags&255u;
        if y<=1u||y>=p.shape.y-1u||x<=1u||x>=p.shape.x-1u||bits==0u||bits==255u||((flags>>8u)&15u)<p.shape.w||mask[y*p.shape.x+x]!=0u{return;}
        let n=neighbors(x,y);var sums=vec3(0.);var minimum=vec3(3.402823466e38);var maximum=-minimum;var count=0.;
        for(var i=0u;i<8u;i++){if (bits&(1u<<i))!=0u{sums=sums+n[i];minimum=select(min(minimum,n[i]),n[i],all(minimum==vec3(0.)));maximum=select(max(maximum,n[i]),n[i],all(maximum==vec3(0.)));count=count+1.;}}
        if count>2.{sums=sums-(minimum+maximum);count=count-2.;}
        let original=load(x,y);let sigma=tolerances(original);let channels=(flags>>12u)&7u;let chosen=selected_channels(channels);let selected=u32(chosen.x)+u32(chosen.y)+u32(chosen.z);var result=original;
        if selected==1u {for(var c=0u;c<3u;c++){if chosen[c]{let in_range=(original[c]-maximum[c])<sigma[c]&&(minimum[c]-original[c])<sigma[c];if !in_range||(p.controls.x!=0u&&c==2u){var other:array<u32,2>;var j=0u;for(var k=0u;k<3u;k++){if k!=c{other[j]=k;j++;}}result[c]=((original[other[0]]+(sums[c]-sums[other[0]])/count)+(original[other[1]]+(sums[c]-sums[other[1]])/count))*0.5;}}}}
        else if selected==2u {if !combined(original,minimum,maximum,sigma,chosen)||(p.controls.x!=0u&&channels!=6u){var reference=0u;for(var c=0u;c<3u;c++){if !chosen[c]{reference=c;}}for(var c=0u;c<3u;c++){if chosen[c]{result[c]=original[reference]+(sums[c]-sums[reference])/count;}}}}
        else if p.controls.x!=0u||!combined(original,minimum,maximum,sigma,chosen){result=sums/count;}
        if finite(result){store(x,y,result);}
}
