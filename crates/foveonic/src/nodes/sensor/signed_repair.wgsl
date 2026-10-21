// Preserve record order and processed-row history while retaining original horizontal neighbors.
struct Params { source:vec4<u32>,origin:vec4<i32> };
@group(0) @binding(0) var<storage,read> original:array<f32>;
@group(0) @binding(1) var<uniform> p:Params;
@group(0) @binding(2) var<storage,read_write> image:array<f32>;
@group(0) @binding(3) var<storage,read> records:array<u32>;
fn rgb(x:u32,y:u32,prior:bool)->vec3<i32>{
    let i=(y*p.source.x+x)*3u;
    if prior {return vec3<i32>(i32(image[i]),i32(image[i+1u]),i32(image[i+2u]));}
    return vec3<i32>(i32(original[i]),i32(original[i+1u]),i32(original[i+2u]));
}
fn luma(v:vec3<i32>)->i32{return v.x+2*v.y+v.z;}
@compute @workgroup_size(1)
fn repair(){
    let dx=array<i32,8>(-1,0,1,-1,1,-1,0,1);
    let dy=array<i32,8>(-1,-1,-1,0,0,1,1,1);
    for(var n=0u;n<p.source.z;n++){
        let r=records[n];let mask=r&255u;
        let x=i32((r>>8u)&4095u)+p.origin.x;let y=i32(r>>20u)+p.origin.y;
        if mask==0u || x<=1 || y<=1 || x>=i32(p.source.x)-1 || y>=i32(p.source.y)-1 {continue;}
        var sum=vec3<i32>(0);var count=0;
        for(var b=0u;b<8u;b++){if (mask&(1u<<b))!=0u{sum+=rgb(u32(x+dx[b]),u32(y+dy[b]),true);count++;}}
        let base=(u32(y)*p.source.x+u32(x))*3u;
        for(var c=0u;c<3u;c++){image[base+c]=f32(sum[c]/count);}
    }
}

// A 32-row batch needs 32 columns of history on each side. Shrinking the
// active halo each row preserves the ordered vertical sweep without a global
// barrier per row. Only the central 192 columns are written by each group.
var<workgroup> previous:array<vec3<i32>,256>;
@compute @workgroup_size(256)
fn despeckle(@builtin(local_invocation_index) lane:u32,@builtin(workgroup_id) group:vec3<u32>){
    let column=i32(group.x*192u+lane)-32+1;
    let x=u32(clamp(column,0,i32(p.source.x)-1));
    previous[lane]=rgb(x,p.source.z-1u,true);
    workgroupBarrier();
    let count=min(32u,p.source.y-1u-p.source.z);
    for(var row=0u;row<count;row++){
        let y=p.source.z+row;
        var next=rgb(x,y,false);
        if lane>row && lane+row+1u<256u && column>0 && column+1<i32(p.source.x) {
            let v=rgb(x,y,false);
            var ns=array<vec3<i32>,8>(rgb(x-1u,y,false),rgb(x+1u,y,false),previous[lane],rgb(x,y+1u,false),previous[lane-1u],previous[lane+1u],rgb(x-1u,y+1u,false),rgb(x+1u,y+1u,false));
            var out=v;var selected=false;var high=false;
            if p.source.w!=0u {
                var lo=ns[0];var hi=ns[0];for(var n=1u;n<8u;n++){lo=min(lo,ns[n]);hi=max(hi,ns[n]);}
                out=clamp(v,lo,hi);selected=true;
            }else{
                let current=luma(v);let left=luma(ns[0]);let right=luma(ns[1]);
                high=current>max(left,right);let low=current<min(left,right);
                if high || low {
                    var a=luma(ns[0]);var b=luma(ns[4]);
                    for(var n=1u;n<4u;n++){
                        a=select(min(a,luma(ns[n])),max(a,luma(ns[n])),high);
                        b=select(min(b,luma(ns[n+4u])),max(b,luma(ns[n+4u])),high);
                    }
                    if high {a=max(5,a);b=max(5,b);}
                    let extreme=max(v.x,max(v.y,v.z))>(current>>1u)+10;
                    let accepted=select(current<min(a,b),((a<current && extreme)||2*a<current)&&((b<current && extreme)||2*b<current),high);
                    if accepted {
                        let diagonal=select(b<a,a<b,high);let start=select(0u,4u,diagonal);let limit=select(a,b,diagonal);
                        out=ns[start+3u];
                        for(var n=0u;n<4u;n++){if luma(ns[start+n])==limit {out=ns[start+n];break;}}
                        for(var c=0u;c<3u;c++){
                            if (high && v[c]<out[c]) || (!high && out[c]<v[c]){
                                var delta=v[c]-out[c];if c==1u{delta=2*v[c]+select(0,1,v.x<0)-2*out[c];}
                                let q=delta>>2u;out-=vec3<i32>(q);out[c]=v[c]-q;
                            }
                        }
                        selected=true;
                    }
                }
            }
            next=v;
            if selected {next=(out<<vec3<u32>(16u))>>vec3<u32>(16u);}
            if lane>=32u && lane<224u {
                let i=(y*p.source.x+x)*3u;
                for(var c=0u;c<3u;c++){image[i+c]=f32(next[c]);}
            }
        }
        workgroupBarrier();
        previous[lane]=next;
        workgroupBarrier();
    }
}
