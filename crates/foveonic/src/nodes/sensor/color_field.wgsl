// Parallel source evidence, sparse native equations, and coarse color graph.
@group(0) @binding(1) var<storage,read> calibrated:array<f32>;
@group(0) @binding(2) var<storage,read> original:array<f32>;
@group(0) @binding(3) var<storage,read> black:array<f32>;
@group(0) @binding(4) var<storage,read_write> masks:array<atomic<u32>>;
@group(0) @binding(5) var<storage,read_write> positions:array<u32>;
@group(0) @binding(6) var<storage,read_write> indices:array<u32>;
@group(0) @binding(7) var<storage,read_write> targets:array<f32>;
@group(0) @binding(8) var<storage,read_write> counters:array<atomic<u32>>;
@group(0) @binding(9) var<storage,read> repairs:array<u32>;
@group(0) @binding(10) var<storage,read_write> equations:array<vec4<u32>>;
@group(0) @binding(11) var<storage,read_write> solution:array<f32>;
@group(0) @binding(12) var<storage,read_write> cells:array<Cell>;
@group(0) @binding(13) var<storage,read_write> edges:array<vec2<f32>>;
@group(0) @binding(14) var<storage,read_write> roots:array<atomic<u32>>;
@group(0) @binding(15) var<storage,read_write> peaks:array<atomic<u32>>;
@group(0) @binding(16) var<storage,read_write> distances:array<atomic<u32>>;
@group(0) @binding(17) var<storage,read_write> output_pixels:array<f32>;
@group(0) @binding(18) var<storage,read> unrepaired:array<f32>;
@group(0) @binding(19) var<storage,read_write> row_blocks:array<u32>;
struct CompactParams { size:vec4<u32> }; // extent, counter slot, reserved, reserved
@group(0) @binding(20) var<uniform> compact_params:CompactParams;
var<workgroup> row_scan:array<u32,256>;

fn inclusive_rank(local:u32,value:u32)->u32 {
    row_scan[local]=value;
    workgroupBarrier();
    for(var stride=1u;stride<256u;stride*=2u){
        var preceding=0u;
        if local>=stride{preceding=row_scan[local-stride];}
        workgroupBarrier();
        row_scan[local]+=preceding;
        workgroupBarrier();
    }
    return row_scan[local];
}

// Integer ranks make CG reductions independent of invocation arrival order.
@compute @workgroup_size(256)
fn compact_rank(@builtin(global_invocation_id)id:vec3<u32>,@builtin(local_invocation_index)local:u32,@builtin(workgroup_id)group:vec3<u32>){
    let p=id.x;
    var eligible=0u;
    if p<compact_params.size.x{eligible=u32(indices[p]!=NONE);}
    let rank=inclusive_rank(local,eligible);
    if p<compact_params.size.x{indices[p]=select(NONE,rank-1u,eligible!=0u);}
    if local==255u{row_blocks[group.x]=rank;}
}

@compute @workgroup_size(256)
fn compact_prefix(@builtin(local_invocation_index)local:u32){
    let blocks=(compact_params.size.x+255u)/256u;
    let chunk=(blocks+255u)/256u;
    let begin=min(local*chunk,blocks);
    let end=min(begin+chunk,blocks);
    var total=0u;
    for(var i=begin;i<end;i++){
        let count=row_blocks[i];row_blocks[i]=total;total+=count;
    }
    let inclusive=inclusive_rank(local,total);
    let offset=inclusive-total;
    for(var i=begin;i<end;i++){row_blocks[i]+=offset;}
    if local==255u{atomicStore(&counters[compact_params.size.y],inclusive);}
}

@compute @workgroup_size(256)
fn compact_scatter(@builtin(global_invocation_id)id:vec3<u32>){
    let p=id.x;
    if p>=compact_params.size.x||indices[p]==NONE{return;}
    let rank=row_blocks[p/256u]+indices[p];
    indices[p]=rank;positions[rank]=p;
}
fn sample(p:u32)->vec3<f32>{let c=p*3u;return vec3(calibrated[c],calibrated[c+1u],calibrated[c+2u])/(params.gain.xyz*(params.white.xyz-vec3(black[0],black[1],black[2])));}
fn in_crop(xy:vec2<u32>)->bool{return all(xy>=params.crop.xy)&&all(xy<params.crop.zw);}
fn adjacent(p:u32,k:u32,width:u32,count:u32)->u32{
    switch k {case 0u:{if p>=width{return p-width;}} case 1u:{if p%width>0u{return p-1u;}} case 2u:{if p+width<count{return p+width;}} default:{if p%width+1u<width{return p+1u;}}}return NONE;
}
fn connection(i:u32,k:u32)->f32{
    switch k{case 0u:{return edges[i-params.grid.x].y;}case 1u:{return edges[i-1u].x;}case 2u:{return edges[i].y;}default:{return edges[i].x;}}
}
@compute @workgroup_size(256)
fn prepare_source(@builtin(global_invocation_id)id:vec3<u32>){
    let p=id.x;if p>=params.size.z{return;}let xy=vec2(p%params.size.x,p/params.size.x);
    let c=p*3u;let b=vec3(black[0],black[1],black[2]);let raw=(vec3(original[c],original[c+1u],original[c+2u])-b)/(params.white.xyz-b);
    let q=vec3<u32>(reliability(raw.x),reliability(raw.y),reliability(raw.z));var mask=q.x|(q.y<<8u)|(q.z<<16u);
    if params.options.z!=0u && any(vec3(calibrated[c],calibrated[c+1u],calibrated[c+2u])!=vec3(unrepaired[c],unrepaired[c+1u],unrepaired[c+2u])){mask|=0x1000000u;}
    atomicStore(&masks[p],mask);
    let m=sample(p);let t=recover(m,vec3<f32>(q)/255.0);var y=dot(params.luminance.xyz,m);
    let eligible=in_crop(xy)&&t.available!=0u&&t.strength>0.0;
    if eligible{y=t.amplitude*dot(params.luminance.xyz,params.neutral.xyz);}
    targets[p]=select(-INF,log(max(y,1e-30)),y>0.0&&finite(y));indices[p]=NONE;
    if eligible{indices[p]=0u;}
}
@compute @workgroup_size(256)
fn mark_repairs(@builtin(global_invocation_id)id:vec3<u32>){if id.x<params.size.w{atomicOr(&masks[repairs[id.x]],0x1000000u);}}
@compute @workgroup_size(256)
fn native_equations(@builtin(global_invocation_id)id:vec3<u32>){
    let i=id.x;if i>=atomicLoad(&counters[0]){return;}let p=positions[i];let m=sample(p);let q=unpack_mask(atomicLoad(&masks[p]));
    // Solve the correction to the anchors, not absolute log brightness.
    // This is the same linear system with much less float32 cancellation.
    var e=Equation(vec4(1.0/64.0,1.0,0.0,0.0),vec4(0.0,0.0,1.0/64.0,1.0),vec4(NONE),vec4(0.0));
    for(var k=0u;k<4u;k++){
        let j=adjacent(p,k,params.size.x,params.size.z);if j==NONE||targets[j]<=-INF{continue;}
        let n=sample(j);let r=unpack_mask(atomicLoad(&masks[j]));var weight=0.0;var gradient=0.0;
        for(var c=0u;c<3u;c++){if m[c]>0.0&&n[c]>0.0&&finite(m[c])&&finite(n[c]){let w=min(q[c],r[c]);weight+=w*w;gradient+=w*w*(log(n[c])-log(m[c]));}}
        e.block.x+=weight;e.rhs.x+=weight*(targets[j]-targets[p])-gradient;
        e.weights[k]=weight;
        if indices[j]!=NONE {e.neighbors[k]=indices[j];}
    }
    let base=3u*i;equations[base]=bitcast<vec4<u32>>(vec4(e.block.x,e.rhs.x,e.rhs.z,e.rhs.w));
    equations[base+1u]=e.neighbors;equations[base+2u]=bitcast<vec4<u32>>(e.weights);solution[i]=0.0;
}
@compute @workgroup_size(256)
fn commit_tone(@builtin(global_invocation_id)id:vec3<u32>){let i=id.x;if i<atomicLoad(&counters[0]){targets[positions[i]]+=solution[i];}}
@compute @workgroup_size(64)
fn collect_grid(@builtin(global_invocation_id)id:vec3<u32>){
    let i=id.x;if i>=params.grid.z{return;}let base=params.crop.xy+vec2(i%params.grid.x,i/params.grid.x)*params.grid.w;
    var n=Cell(vec4(0.0),vec4(0.0),vec4(0.0),vec4(0u));var count=0.0;var tones=0.0;var donors=0.0;var stressed=false;var second=vec3(0.0);var accepted=vec3(1.0);
    for(var dy=0u;dy<params.grid.w;dy++){for(var dx=0u;dx<params.grid.w;dx++){
        let xy=base+vec2(dx,dy);if !in_crop(xy){continue;}let p=xy.y*params.size.x+xy.x;let mask=atomicLoad(&masks[p]);let m=sample(p);let g=evidence(m,mask);
        let affected=(mask&0xffffffu)!=0xffffffu;let repaired=(mask&0x1000000u)!=0u;n.flags.y|=u32(affected);
        if targets[p]>log(1e-9){n.pair.w+=targets[p];tones+=1.0;}
        n.pair=vec4(n.pair.xyz+g.pair*g.q,n.pair.w);n.q=vec4(n.q.xyz+g.q,0.0);if params.neutral.w>0.0{second+=g.pair*g.pair*g.q;}count+=1.0;
        if !repaired {stressed=stressed||affected;if !affected&&all(m>vec3(0.02))&&finite3(m)&&dot(params.luminance.xyz,m)>1e-9{n.color=vec4(n.color.xy+g.pair.yz,0.0,0.0);donors+=1.0;}}
    }}
    n.pair.w=select(-INF,n.pair.w/max(tones,1.0),tones>0.0);
    for(var k=0u;k<3u;k++){if n.q[k]>0.0{let mean=n.pair[k]/n.q[k];
        if params.neutral.w>0.0{let sigma=sqrt(max(second[k]/n.q[k]-mean*mean,0.0));accepted[k]=1.0-smoothstep(params.neutral.w,2.0*params.neutral.w,sigma);}
        n.pair[k]=mean;}n.q[k]/=max(count,1.0);}
    n.flags.x=u32(!stressed&&donors>=8.0&&tones>0.0);if n.flags.x!=0u{n.flags.y=0u;}
    // Keep unfiltered guides for graph connectivity. Carry the three acceptance
    // weights in spare lanes until color equations consume them.
    if donors>0.0{n.color=vec4(n.color.xy/donors,0.0,0.0);}n.color=vec4(n.color.xy,accepted.xy);n.q.w=accepted.z;cells[i]=n;
    // Brightness no longer needs these maps. Reuse them for color unknowns.
    // Fixed rows are already eliminated from neighboring right-hand sides.
    if n.flags.y!=0u {indices[i]=0u;}
    else {indices[i]=NONE;if n.flags.x!=0u{let rhs=abs(n.color.xy-prior_uv());atomicMax(&counters[3],bitcast<u32>(max(rhs.x,rhs.y)));}}
    atomicStore(&roots[i],i);atomicStore(&peaks[2u*i],0u);atomicStore(&peaks[2u*i+1u],NONE);
    atomicStore(&distances[i],bitcast<u32>(select(INF,0.0,n.flags.x!=0u)));
}
@compute @workgroup_size(256)
fn connect_grid(@builtin(global_invocation_id)id:vec3<u32>){
    let i=id.x;if i>=params.grid.z{return;}var e=vec2(0.0);let a=guide(cells[i]);
    if i%params.grid.x+1u<params.grid.x{e.x=affinity(a,guide(cells[i+1u]));}
    if i+params.grid.x<params.grid.z{e.y=affinity(a,guide(cells[i+params.grid.x]));}edges[i]=e;
}
fn root_of(i:u32)->u32{var r=i;loop{let p=atomicLoad(&roots[r]);if p==r{break;}r=p;}return r;}
@compute @workgroup_size(256)
fn union_grid(@builtin(global_invocation_id)id:vec3<u32>){
    let i=id.x;if i>=params.grid.z||cells[i].flags.y==0u{return;}
    for(var k=2u;k<4u;k++){let j=adjacent(i,k,params.grid.x,params.grid.z);if j==NONE||connection(i,k)<=0.0||cells[j].flags.y==0u{continue;}
        let a=root_of(i);let b=root_of(j);if a!=b{let old=atomicMin(&roots[max(a,b)],min(a,b));if old>min(a,b){atomicOr(&counters[1],1u);}}
    }
    let r=root_of(i);atomicMin(&roots[i],r);
}
fn ordered_float(v:f32)->u32{let bits=bitcast<u32>(v);return bits^select(0xffffffffu,0x80000000u,v>=0.0);}
@compute @workgroup_size(256)
fn region_peaks(@builtin(global_invocation_id)id:vec3<u32>){let i=id.x;if i<params.grid.z&&cells[i].flags.y!=0u{let r=root_of(i);atomicStore(&roots[i],r);atomicMax(&peaks[2u*r],ordered_float(cells[i].pair.w));}}
@compute @workgroup_size(256)
fn peak_indices(@builtin(global_invocation_id)id:vec3<u32>){let i=id.x;if i<params.grid.z&&cells[i].flags.y!=0u{let r=atomicLoad(&roots[i]);if ordered_float(cells[i].pair.w)==atomicLoad(&peaks[2u*r]){atomicMin(&peaks[2u*r+1u],i);}}}
@compute @workgroup_size(256)
fn condition_grid(@builtin(global_invocation_id)id:vec3<u32>){
    let i=id.x;if i>=params.grid.z{return;}let a=cells[i];var edge=edges[i];
    for(var k=2u;k<4u;k++){let j=adjacent(i,k,params.grid.x,params.grid.z);if j==NONE{continue;}let b=cells[j];var gate=1.0;
        if (a.flags.x==0u&&a.flags.y==0u)||(b.flags.x==0u&&b.flags.y==0u){gate=0.0;}
        else if a.flags.y!=0u&&b.flags.x!=0u {let peak=atomicLoad(&peaks[2u*atomicLoad(&roots[i])+1u]);gate=agreement(guide(cells[peak]),guide(b),params.tuning.y,0.12*params.tuning.x/0.035);}
        else if b.flags.y!=0u&&a.flags.x!=0u {let peak=atomicLoad(&peaks[2u*atomicLoad(&roots[j])+1u]);gate=agreement(guide(cells[peak]),guide(a),params.tuning.y,0.12*params.tuning.x/0.035);}
        edge[3u-k]*=gate;
    }edges[i]=edge;
}
@compute @workgroup_size(256)
fn propagate_distance(@builtin(global_invocation_id)id:vec3<u32>){
    let i=id.x;if i>=params.grid.z{return;}var best=bitcast<f32>(atomicLoad(&distances[i]));
    for(var k=0u;k<4u;k++){let j=adjacent(i,k,params.grid.x,params.grid.z);if j==NONE{continue;}let w=connection(i,k);if w>0.0{best=min(best,bitcast<f32>(atomicLoad(&distances[j]))+f32(params.grid.w)/sqrt(w));}}
    let old=atomicMin(&distances[i],bitcast<u32>(best));if old>bitcast<u32>(best){atomicOr(&counters[1],1u);}
}
@compute @workgroup_size(256)
fn color_equations(@builtin(global_invocation_id)id:vec3<u32>){
    let index=id.x;if index>=atomicLoad(&counters[2]){return;}let i=positions[index];let n=cells[i];
    let original=guide(n);var g=original;g.q*=vec3(n.color.zw,n.q.w);
    // Selection must not increase the existing neutral prior when evidence is rejected.
    let nw=neutral_weight(original);let local_block=data_block(g)+nw*params.metric.xyz;
    // Preserve ratio weights, not an expanded cancellation-prone block.
    // The preconditioner still uses the complete expanded diagonal.
    var e=Equation(vec4(local_block,4.0*g.q.x),vec4(relative_rhs(g),4.0*g.q.yz),vec4(NONE),vec4(0.0));
    for(var k=0u;k<4u;k++){let j=adjacent(i,k,params.grid.x,params.grid.z);if j==NONE{continue;}let w=connection(i,k);if w<=0.0{continue;}
        e.block+=vec4(w*params.metric.xyz,0.0);e.weights[k]=w;
        if cells[j].flags.x!=0u{e.rhs+=vec4(w*metric_mul(cells[j].color.xy-prior_uv()),0.0,0.0);}else{e.neighbors[k]=indices[j];}
    }
    let base=5u*index;equations[base]=bitcast<vec4<u32>>(e.block);equations[base+1u]=bitcast<vec4<u32>>(e.rhs);
    equations[base+2u]=e.neighbors;equations[base+3u]=bitcast<vec4<u32>>(e.weights);
    equations[base+4u]=bitcast<vec4<u32>>(vec4(nw,0.0,0.0,0.0));
    for(var k=0u;k<4u;k++){solution[4u*index+k]=0.0;}
}
@compute @workgroup_size(256)
fn commit_color(@builtin(global_invocation_id)id:vec3<u32>){let i=id.x;if i<params.grid.z{
    var color=prior_uv();if cells[i].flags.x!=0u{color=(cells[i].color.xy-prior_uv())+prior_uv();}
    else if cells[i].flags.y!=0u{let j=4u*indices[i];let value=vec4(solution[j],solution[j+1u],solution[j+2u],solution[j+3u]);color=(value.xy+prior_uv())+value.zw;}
    cells[i].color=vec4(color,0.0,0.0);
}}
@compute @workgroup_size(256)
fn apply_field(@builtin(global_invocation_id)id:vec3<u32>){
    let p=id.x;if p>=params.size.z{return;}let xy=vec2(p%params.size.x,p/params.size.x);let mask=atomicLoad(&masks[p]);let c=p*3u;
    var out=vec3(calibrated[c],calibrated[c+1u],calibrated[c+2u]);
    if in_crop(xy)&&(mask&0xffffffu)!=0xffffffu{
        let m=sample(p);let t=recover(m,unpack_mask(mask));
        if t.available!=0u&&t.strength>0.0{
            // This is the same eligibility test used when targets were built.
            let amplitude=exp(targets[p])/dot(params.luminance.xyz,params.neutral.xyz);
            let tone_value=params.neutral.xyz*amplitude;let ty=dot(params.luminance.xyz,tone_value);var g=evidence(m,mask);g.tone=log(max(ty,1e-30));
            let coordinate=(vec2<f32>(xy-params.crop.xy)+0.5)/f32(params.grid.w)-0.5;let lower=vec2<i32>(floor(coordinate));var sum=vec2(0.0);var total=0.0;var distance=0.0;
            for(var dy=0;dy<2;dy++){for(var dx=0;dx<2;dx++){let q=lower+vec2(dx,dy);if any(q<vec2(0))||q.x>=i32(params.grid.x)||q.y>=i32(params.grid.y){continue;}
                let i=u32(q.y)*params.grid.x+u32(q.x);let n=cells[i];let d=bitcast<f32>(atomicLoad(&distances[i]));if d==INF&&n.flags.y==0u{continue;}
                let delta=abs(coordinate-vec2<f32>(q));let w=(1.0-delta.x)*(1.0-delta.y)*affinity(g,guide(n));if w<=0.0{continue;}
                sum+=w*n.color.xy;total+=w;distance+=w*d;
            }}
            var prior=prior_uv();distance=select(INF,distance/max(total,1e-30),total>1e-12);if total>1e-12{prior=sum/total;}
            // Retain the reference prior-coordinate round trip.
            prior=prior_uv()+(prior-prior_uv());
            let proposal=Guide(ratios(prior),vec3(1.0),g.tone);let confidence=affinity(g,proposal);
            let adjusted=prior_uv()+confidence*(prior-prior_uv());let weight=0.25+1.75*exp(-distance/256.0);
            let before=fit(g,prior,weight);let after=fit(g,adjusted,weight);let retained=retained(before,after,g);
            var d=direction(before+retained*(after-before));if !finite3(d)||any(d<=vec3(0.0)){d=direction(prior+retained*(adjusted-prior));}if !finite3(d)||any(d<=vec3(0.0)){d=direction(prior_uv());}
            var recovered=tone_value;if finite3(d)&&all(d>vec3(0.0))&&ty>0.0{recovered=d*ty;}
            if t.strength<1.0{recovered=m+t.strength*(recovered-m);}
            out=recovered*params.gain.xyz*(params.white.xyz-vec3(black[0],black[1],black[2]));
        }
    }
    output_pixels[c]=out.x;output_pixels[c+1u]=out.y;output_pixels[c+2u]=out.z;
}
