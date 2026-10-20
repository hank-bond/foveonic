// Block-Jacobi preconditioned conjugate gradients. Reductions stay on device.
const WORKGROUP_SIZE:u32=256u;
struct SolverParams {size:vec4<u32>,metric:vec4<f32>,tolerance:vec4<f32>};
@group(0) @binding(0) var<uniform> solve_params:SolverParams;
struct Equation {block:vec4<f32>,rhs:vec4<f32>,neighbors:vec4<u32>,weights:vec4<f32>};
@group(0) @binding(1) var<storage,read> equations:array<vec4<u32>>;
@group(0) @binding(2) var<storage,read_write> x:array<f32>;
@group(0) @binding(3) var<storage,read_write> residual:array<f32>;
@group(0) @binding(4) var<storage,read_write> direction:array<f32>;
@group(0) @binding(5) var<storage,read_write> mapped:array<f32>;
@group(0) @binding(6) var<storage,read_write> partials:array<vec4<f32>>;
@group(0) @binding(7) var<storage,read_write> state:array<vec4<f32>>;
var<workgroup> reduction_scratch:array<vec4<f32>,WORKGROUP_SIZE>;
fn color()->bool{return solve_params.size.z==1u;}
// Scalar equations use 48 bytes. Color equations use 80 bytes to retain the
// original neutral coefficient independently of filtered color evidence.
fn equation(i:u32)->Equation{
    if color(){let j=5u*i;return Equation(bitcast<vec4<f32>>(equations[j]),bitcast<vec4<f32>>(equations[j+1u]),equations[j+2u],bitcast<vec4<f32>>(equations[j+3u]));}
    let j=3u*i;let d=bitcast<vec4<f32>>(equations[j]);
    return Equation(vec4(d.x,1.0,0.0,0.0),vec4(d.y,0.0,d.z,d.w),equations[j+1u],bitcast<vec4<f32>>(equations[j+2u]));
}
// Work vectors use only their active channels. Color iterates keep four
// components to retain their compensated rounding remainders.
fn load_x(i:u32)->vec4<f32>{if color(){let j=4u*i;return vec4(x[j],x[j+1u],x[j+2u],x[j+3u]);}return vec4(x[i],0.0,0.0,0.0);}
fn store_x(i:u32,v:vec4<f32>){if color(){let j=4u*i;x[j]=v.x;x[j+1u]=v.y;x[j+2u]=v.z;x[j+3u]=v.w;}else{x[i]=v.x;}}
// The update already computes z=M^-1 r for its reduction. Keep it beside r
// so the direction pass need not read the equation or precondition it again.
fn load_r(i:u32)->vec2<f32>{if color(){return vec2(residual[4u*i],residual[4u*i+1u]);}return vec2(residual[2u*i],0.0);}
fn load_z(i:u32)->vec2<f32>{if color(){return vec2(residual[4u*i+2u],residual[4u*i+3u]);}return vec2(residual[2u*i+1u],0.0);}
fn store_r(i:u32,r:vec2<f32>,z:vec2<f32>){if color(){let j=4u*i;residual[j]=r.x;residual[j+1u]=r.y;residual[j+2u]=z.x;residual[j+3u]=z.y;}else{residual[2u*i]=r.x;residual[2u*i+1u]=z.x;}}
fn load_p(i:u32)->vec2<f32>{if color(){return vec2(direction[2u*i],direction[2u*i+1u]);}return vec2(direction[i],0.0);}
fn store_p(i:u32,v:vec2<f32>){if color(){direction[2u*i]=v.x;direction[2u*i+1u]=v.y;}else{direction[i]=v.x;}}
fn load_ap(i:u32)->vec2<f32>{if color(){return vec2(mapped[2u*i],mapped[2u*i+1u]);}return vec2(mapped[i],0.0);}
fn store_ap(i:u32,v:vec2<f32>){if color(){mapped[2u*i]=v.x;mapped[2u*i+1u]=v.y;}else{mapped[i]=v.x;}}
fn precondition(e:Equation,r:vec2<f32>)->vec2<f32>{
    if !color(){return vec2(r.x/e.block.x,0.0);}
    let b=e.block.xyz;let d=b.x*b.y-b.z*b.z;return vec2(b.y*r.x-b.z*r.y,b.x*r.y-b.z*r.x)/d;
}
fn block_product(b:vec3<f32>,v:vec2<f32>)->vec2<f32>{
    if !color(){return vec2(b.x*v.x,0.0);}
    return vec2(b.x*v.x+b.z*v.y,b.z*v.x+b.y*v.y);
}
// Explicit fused operations retain the rounding remainder. Ordinary additions
// in the ratio product can lose these components after shader compilation.
fn compensated_add(value:vec4<f32>,increment:vec2<f32>)->vec4<f32>{
    let swap=abs(increment)>abs(value.xy);
    let big=select(value.xy,increment,swap);let small=select(increment,value.xy,swap);
    let sum=big+small;let remainder=fma(vec2(1.0),small,fma(vec2(-1.0),sum,big))+value.zw;
    let hi=sum+remainder;let lo=fma(vec2(1.0),remainder,fma(vec2(-1.0),hi,sum));
    return vec4(hi,lo);
}
fn product(i:u32,use_x:bool)->vec2<f32>{
    let e=equation(i);var v=vec2(0.0);var low=vec2(0.0);
    if use_x{let value=load_x(i);v=value.xy;low=value.zw;}else{v=load_p(i);}
    // Neighbor differences avoid subtracting large nearly equal products.
    var out=block_product(vec3(e.rhs.z,e.rhs.w,e.block.w),v);
    if color(){
        if e.block.w<0.0{out=v+low;}else{
            let q=vec3(e.block.w,e.rhs.z,e.rhs.w);
            let nw=bitcast<vec4<f32>>(equations[5u*i+4u]).x;
            let pair=fma(q.x,low.x,fma(-q.x,low.y,q.x*(v.x-v.y)));
            out=vec2(pair+q.y*v.x,-pair+q.z*v.y)+vec2(q.y*low.x,q.z*low.y)+nw*(block_product(solve_params.metric.xyz,v)+block_product(solve_params.metric.xyz,low));
        }
    }
    for(var k=0u;k<4u;k++){
        var n=vec2(0.0);var nl=vec2(0.0);let j=e.neighbors[k];
        if j!=0xffffffffu{if use_x{let value=load_x(j);n=value.xy;nl=value.zw;}else{n=load_p(j);}}
        out+=e.weights[k]*block_product(solve_params.metric.xyz,(v-n)+(low-nl));
    }
    return out;
}
fn maximum(v:vec2<f32>)->f32{if any(abs(v)>vec2(3.402823466e38))||any(v!=v){return 3.402823466e38;}return max(abs(v.x),abs(v.y));}
fn reduce(local:u32,value:vec4<f32>)->vec4<f32>{
    reduction_scratch[local]=value;workgroupBarrier();
    for(var s=WORKGROUP_SIZE/2u;s>0u;s/=2u){if local<s{let b=reduction_scratch[local+s];reduction_scratch[local]=vec4(reduction_scratch[local].x+b.x,max(reduction_scratch[local].y,b.y),max(reduction_scratch[local].z,b.z),max(reduction_scratch[local].w,b.w));}workgroupBarrier();}
    return reduction_scratch[0];
}
fn reduce_partials(local:u32)->vec4<f32>{
    var v=vec4(0.0);for(var i=local;i<solve_params.size.y;i+=WORKGROUP_SIZE){let p=partials[i];v=vec4(v.x+p.x,max(v.y,p.y),max(v.z,p.z),max(v.w,p.w));}return reduce(local,v);
}
@compute @workgroup_size(WORKGROUP_SIZE)
fn initialize(@builtin(global_invocation_id)id:vec3<u32>,@builtin(local_invocation_index)local:u32,@builtin(workgroup_id)group:vec3<u32>){
    let i=id.x;var part=vec4(0.0);
    if i<solve_params.size.x{
        let e=equation(i);let r=e.rhs.xy-product(i,true);let z=precondition(e,r);
        // Checkpoints preserve conjugacy. Restart only on first use or when
        // recursive convergence needs replacement by the true residual.
        if state[1].w==0.0||state[0].w!=0.0{store_r(i,r,z);store_p(i,z);}
        let determinant=e.block.x*e.block.y-e.block.z*e.block.z;
        let positive=e.block.x>0.0&&e.block.y>0.0&&determinant>0.0&&determinant<=3.402823466e38;
        part=vec4(dot(r,z),maximum(r),maximum(e.rhs.xy),select(0.0,1.0,!positive||maximum(z)==3.402823466e38));
    }
    let v=reduce(local,part);if local==0u{partials[group.x]=v;}
}
@compute @workgroup_size(WORKGROUP_SIZE)
fn initialize_state(@builtin(local_invocation_index)local:u32){let v=reduce_partials(local);if local==0u{
    let tolerance=solve_params.tolerance.x*max(max(v.z,solve_params.tolerance.y),1.0);let done=select(0.0,1.0,v.y<=tolerance);
    if state[1].w==0.0||state[0].w!=0.0{state[0]=vec4(v.x,0.0,0.0,done);}else{state[0].w=done;}
    state[1]=vec4(v.y,tolerance,max(state[1].z,v.w),1.0);
}}
@compute @workgroup_size(WORKGROUP_SIZE)
fn map_direction(@builtin(global_invocation_id)id:vec3<u32>,@builtin(local_invocation_index)local:u32,@builtin(workgroup_id)group:vec3<u32>){
    let i=id.x;var part=vec4(0.0);if i<solve_params.size.x&&state[0].w==0.0{let v=product(i,false);store_ap(i,v);part.x=dot(load_p(i),v);}
    let v=reduce(local,part);if local==0u{partials[group.x]=v;}
}
@compute @workgroup_size(WORKGROUP_SIZE)
fn alpha(@builtin(local_invocation_index)local:u32){let v=reduce_partials(local);if local==0u&&state[0].w==0.0{if v.x<=0.0||v.x!=v.x{state[1].z=1.0;state[0].w=1.0;}else{state[0].y=state[0].x/v.x;}}}
@compute @workgroup_size(WORKGROUP_SIZE)
fn step(@builtin(global_invocation_id)id:vec3<u32>,@builtin(local_invocation_index)local:u32,@builtin(workgroup_id)group:vec3<u32>){
    let i=id.x;var part=vec4(0.0);if i<solve_params.size.x&&state[0].w==0.0{
        let a=state[0].y;let value=load_x(i);let p=load_p(i);
        if color(){store_x(i,compensated_add(value,a*p));}else{store_x(i,vec4(value.x+a*p.x,0.0,0.0,0.0));}
        let r=load_r(i)-a*load_ap(i);let z=precondition(equation(i),r);store_r(i,r,z);
        part=vec4(dot(r,z),maximum(r),0.0,select(0.0,1.0,maximum(z)==3.402823466e38));
    }
    let v=reduce(local,part);if local==0u{partials[group.x]=v;}
}
@compute @workgroup_size(WORKGROUP_SIZE)
fn beta(@builtin(local_invocation_index)local:u32){let v=reduce_partials(local);if local==0u&&state[0].w==0.0{state[0].z=v.x/state[0].x;state[0].x=v.x;state[0].w=select(0.0,1.0,v.y<=state[1].y);state[1].x=v.y;state[1].z=max(state[1].z,v.w);}}
@compute @workgroup_size(WORKGROUP_SIZE)
fn update_direction(@builtin(global_invocation_id)id:vec3<u32>){let i=id.x;if i<solve_params.size.x&&state[0].w==0.0{store_p(i,load_z(i)+state[0].z*load_p(i));}}

// Local uses exactly 64 damped Jacobi sweeps, not a convergence claim.
fn local_value(i:u32,backward:bool)->f32{
    let e=equation(i);var sum=e.rhs.x;
    for(var k=0u;k<4u;k++){let j=e.neighbors[k];if j!=0xffffffffu{var v=0.0;if backward{v=mapped[j];}else{v=x[j];}sum+=e.weights[k]*v;}}
    var old=0.0;if backward{old=mapped[i];}else{old=x[i];}
    return old+0.75*(sum/e.block.x-old);
}
@compute @workgroup_size(WORKGROUP_SIZE)
fn local_forward(@builtin(global_invocation_id)id:vec3<u32>){let i=id.x;if i<solve_params.size.x{mapped[i]=local_value(i,false);}}
@compute @workgroup_size(WORKGROUP_SIZE)
fn local_backward(@builtin(global_invocation_id)id:vec3<u32>){let i=id.x;if i<solve_params.size.x{x[i]=local_value(i,true);}}
