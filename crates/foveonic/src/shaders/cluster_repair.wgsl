// Stable, sequential masked camera-cluster patch matching. Metadata preparation
// supplies initial channel/known masks and the native stable quality ordering.
// Adapted with permission from Foveonic clusters.rs at e47af35.
struct Cluster { rect:vec4<u32>, indices:vec4<u32> };
struct Params { source:vec4<u32> };
@group(0) @binding(0) var<storage,read_write> pixels:array<f32>;
@group(0) @binding(1) var<storage,read> clusters:array<Cluster>;
@group(0) @binding(2) var<storage,read> members:array<vec4<u32>>;
@group(0) @binding(3) var<storage,read_write> masks:array<u32>;
@group(0) @binding(4) var<uniform> params:Params;
const mappings=array<array<u32,9>,6>(
    array<u32,9>(0,1,2,3,4,5,6,7,8),array<u32,9>(6,3,0,7,4,1,8,5,2),
    array<u32,9>(2,5,8,1,4,7,0,3,6),array<u32,9>(6,7,8,3,4,5,0,1,2),
    array<u32,9>(2,1,0,5,4,3,8,7,6),array<u32,9>(8,7,6,5,4,3,2,1,0));
const weight_cells=array<u32,9>(0,1,5,3,4,2,6,7,8);
fn color(p:u32)->vec3<f32>{return vec3(pixels[p*3u],pixels[p*3u+1u],pixels[p*3u+2u]);}
fn patch_pixel(cx:u32,cy:u32,i:u32)->u32{return (cy+i/3u-1u)*params.source.x+cx+i%3u-1u;}
@compute @workgroup_size(1)
fn main(){
    for(var k=0u;k<params.source.z;k++){
        let cluster=clusters[k];let left=cluster.rect.x;let top=cluster.rect.y;let pw=cluster.rect.z;let ph=cluster.rect.w;let mask_base=cluster.indices.z;
        for(var member=0u;member<cluster.indices.y;member++){
            let record=members[cluster.indices.x+member];let x=record.y%pw;let y=record.y/pw;
            var weights:array<vec3<f32>,9>;var negatives:array<vec3<f32>,9>;
            for(var i=0u;i<9u;i++){
                let cell=weight_cells[i];let m=masks[mask_base+(y+cell/3u-1u)*pw+x+cell%3u-1u];
                weights[i]=vec3(f32((m>>2u)&1u),f32((m>>1u)&1u),f32(m&1u));
                negatives[i]=-(color(patch_pixel(left+x,top+y,i))*weights[i]);
            }
            var best=vec3(0.0);var best_cost=1000000.0;
            for(var cy=1u;cy<ph-2u;cy++){for(var cx=1u;cx<pw-2u;cx++){
                var known=true;
                for(var i=0u;i<9u;i++){if (masks[mask_base+(cy+i/3u-1u)*pw+cx+i%3u-1u]&256u)==0u{known=false;break;}}
                if !known{continue;}
                var candidate:array<vec3<f32>,9>;
                for(var i=0u;i<9u;i++){candidate[i]=color(patch_pixel(left+cx,top+cy,i));}
                for(var mapping=0u;mapping<6u;mapping++){
                    var cost=0.0;
                    for(var i=0u;i<9u;i++){
                        let t=mappings[mapping][i];let v=abs(fma(candidate[i],weights[t],negatives[t]));
                        let error=(v.x+v.y)+v.z;cost+=error;
                    }
                    if cost<best_cost{best_cost=cost;best=candidate[4];}
                }
            }}
            pixels[record.x*3u]=best.x;pixels[record.x*3u+1u]=best.y;pixels[record.x*3u+2u]=best.z;
            masks[mask_base+record.y]=(masks[mask_base+record.y]&256u)|7u;
        }
    }
}
