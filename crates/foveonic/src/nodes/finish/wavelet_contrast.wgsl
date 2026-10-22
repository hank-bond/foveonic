struct Params{shape:vec4<u32>,crop:vec4<u32>,controls:vec4<f32>}
@group(0) @binding(0) var<storage,read> packed:array<f32>;
@group(0) @binding(1) var<uniform> p:Params;
@group(0) @binding(2) var<storage,read> source:array<f32>;
@group(0) @binding(3) var<storage,read_write> destination:array<f32>;
@group(0) @binding(4) var<uniform> step:vec4<u32>;
@group(0) @binding(5) var<storage,read> bands:array<vec4<f32>>;
@group(0) @binding(6) var<storage,read> kernel:array<f32>;
fn product(a:f32,b:f32)->vec2<f32>{let high=fma(a,b,0.);return vec2(high,fma(a,b,-high));}
fn add_pair(a:vec2<f32>,b:vec2<f32>)->vec2<f32>{let sum=fma(1.,a.x,b.x);let vb=fma(-1.,a.x,sum);let ra=fma(-1.,fma(-1.,vb,sum),a.x);let rb=fma(-1.,vb,b.x);let error=fma(1.,fma(1.,ra,rb),fma(1.,a.y,b.y));let high=fma(1.,sum,error);return vec2(high,fma(-1.,fma(-1.,sum,high),error));}
fn multiply_pair(a:vec2<f32>,b:f32)->vec2<f32>{return add_pair(product(a.x,b),product(a.y,b));}

fn inverse_scale(v:f32)->f32{let result=add_pair(product(v,p.controls.z),product(v,p.controls.w));return fma(1.,result.x,result.y);}
@compute @workgroup_size(8,8)
fn dwt_extract(@builtin(global_invocation_id) id:vec3<u32>){if id.x>=p.shape.x||id.y>=p.shape.y{return;}let x=clamp(id.x,p.crop.x,p.crop.z-1u);let y=clamp(id.y,p.crop.y,p.crop.w-1u);destination[id.y*p.shape.x+id.x]=packed[(y*p.shape.z+x)*3u];}
fn line_index(x:u32,y:u32,coordinate:u32)->u32{return select(y*p.shape.x+coordinate,coordinate*p.shape.x+x,step.z==1u);}
fn predicted_odd(x:u32,y:u32,i:u32,last:u32)->f32{let even=source[line_index(x,y,i*2u)];let next=source[line_index(x,y,min(i+1u,last)*2u)];let odd=source[line_index(x,y,i*2u+1u)];let sum=fma(1.,even,next);let prediction=fma(0.5,sum,0.);return fma(-1.,prediction,odd);}
@compute @workgroup_size(8,8)
fn dwt_forward(@builtin(global_invocation_id) id:vec3<u32>){if id.x>=step.x||id.y>=step.y{return;}let coordinate=select(id.x,id.y,step.z==1u);let half_length=select(step.x,step.y,step.z==1u)/2u;let last=half_length-1u;var result=0.;
 if coordinate<half_length{let i=coordinate;let even=source[line_index(id.x,id.y,i*2u)];let a=predicted_odd(id.x,id.y,select(i-1u,0u,i==0u),last);let b=predicted_odd(id.x,id.y,i,last);let update=fma(0.25,fma(1.,a,b),0.);result=fma(1.,even,update)*p.controls.y;}
 else{result=inverse_scale(predicted_odd(id.x,id.y,coordinate-half_length,last));}
 destination[id.y*p.shape.x+id.x]=result;
}
fn scaled_odd(x:u32,y:u32,i:u32,half_length:u32)->f32{return source[line_index(x,y,half_length+i)]*p.controls.y;}
fn recovered_even(x:u32,y:u32,i:u32,half_length:u32)->f32{let even=inverse_scale(source[line_index(x,y,i)]);let before=scaled_odd(x,y,select(i-1u,0u,i==0u),half_length);let after=scaled_odd(x,y,i,half_length);let update=fma(0.25,fma(1.,before,after),0.);return fma(-1.,update,even);}
@compute @workgroup_size(8,8)
fn dwt_inverse(@builtin(global_invocation_id) id:vec3<u32>){if id.x>=step.x||id.y>=step.y{return;}let coordinate=select(id.x,id.y,step.z==1u);let half_length=select(step.x,step.y,step.z==1u)/2u;let i=coordinate/2u;var result=recovered_even(id.x,id.y,i,half_length);
 if coordinate%2u==1u{let next=recovered_even(id.x,id.y,min(i+1u,half_length-1u),half_length);let prediction=fma(0.5,fma(1.,result,next),0.);result=fma(1.,scaled_odd(id.x,id.y,i,half_length),prediction);}
 destination[id.y*p.shape.x+id.x]=result;
}
@compute @workgroup_size(8,8)
fn dwt_gain(@builtin(global_invocation_id) id:vec3<u32>){let x=id.x;let y=id.y;if x>=p.shape.x||y>=p.shape.y{return;}let index=y*p.shape.x+x;let lw=p.shape.x>>6u;let lh=p.shape.y>>6u;
 if x<lw&&y<lh{var total=vec2(0.);for(var ky=-1;ky<=1;ky++){for(var kx=-1;kx<=1;kx++){let sx=u32(clamp(i32(x)+kx,0,i32(lw)-1));let sy=u32(clamp(i32(y)+ky,0,i32(lh)-1));let term=fma(source[sy*p.shape.x+sx],kernel[u32((ky+1)*3+kx+1)],0.);total=add_pair(total,vec2(term,0.));}}destination[index]=fma(1.,total.x,total.y);return;}
 var result=source[index];for(var band=0u;band<6u;band++){let w=p.shape.x>>(6u-band);let h=p.shape.y>>(6u-band);if x<w*2u&&y<h*2u&&(x>=w||y>=h){let control=bands[band];let magnitude=abs(result);var amount=fma(control.x,magnitude,0.);let upper=fma(1.,magnitude,control.y);let lower=fma(-1.,control.y,magnitude);if amount<lower{amount=fma(-fma(-1.,amount,lower),control.z,lower);}if amount>upper{amount=fma(fma(-1.,upper,amount),control.z,upper);}result=select(-amount,amount,result>=0.);break;}}
 destination[index]=result;
}
@compute @workgroup_size(8,8)
fn dwt_copyback(@builtin(global_invocation_id) id:vec3<u32>){if id.x>=p.shape.z||id.y>=p.shape.w{return;}let i=(id.y*p.shape.z+id.x)*3u;var value=packed[i];if id.x>=p.crop.x&&id.y>=p.crop.y&&id.x<p.crop.z&&id.y<p.crop.w{let lower=fma(1.,1.,-p.controls.x)*value;let upper=fma(1.,1.,p.controls.x)*value;value=f32(u32(fma(1.,clamp(source[id.y*p.shape.x+id.x],lower,upper),0.5))&65535u);}destination[i]=value;destination[i+1u]=packed[i+1u];destination[i+2u]=packed[i+2u];}
