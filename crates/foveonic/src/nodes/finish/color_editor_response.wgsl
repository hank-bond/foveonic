// Independently expressed brightness response from reference B&W research.
const REFERENCE_LUMA: vec3<f32> = vec3(0.3328,0.6121,0.0551);
// Full range selection is reached at five percent guide chroma.
const NEUTRAL_CHROMA_LIMIT: f32 = 0.05;
const DARK_RESPONSE_KNEE: f32 = 0.125;
const DARK_RESPONSE_OFFSET: f32 = 0.03125;
@compute @workgroup_size(8,8)
fn color_editor_map(@builtin(global_invocation_id) id: vec3<u32>) {
    if !valid(id) { return; }
    let i=id.y*p.shape.x+id.x;
    let c=max(transform(rgb(i),p.guide_matrix),vec3(0.));
    let lo=min(c.x,min(c.y,c.z)); let hi=max(c.x,max(c.y,c.z));
    let mid=c.x+c.y+c.z-lo-hi;
    var lowest=0u; var highest=0u;
    if c.y<c.x { lowest=1u; } if c.z<c[lowest] { lowest=2u; }
    if c.y>c.x { highest=1u; } if c.z>c[highest] { highest=2u; }
    // Original six range weights sum to chroma; complements follow the lowest channel.
    let primary=array<u32,3>(0u,3u,5u); let secondary=array<u32,3>(4u,7u,2u);
    var weights=array<f32,8>(0.,0.,0.,0.,0.,0.,0.,0.);
    weights[primary[highest]]=hi-mid; weights[secondary[lowest]]=mid-lo;
    // Split Red/Yellow at Orange and Blue/Magenta at Purple. Migrated midpoint
    // coefficients preserve the old six-color response across both new boundaries.
    let orange=min(weights[0],weights[2]);
    weights[0]-=orange; weights[2]-=orange; weights[1]=2.*orange;
    let purple=min(weights[5],weights[7]);
    weights[5]-=purple; weights[7]-=purple; weights[6]=2.*purple;
    var adjustment=vec3(0.);
    for(var k=0u;k<8u;k++) { adjustment+=weights[k]*p.bands[k].xyz; }
    let neutral=dot(c,REFERENCE_LUMA);
    let floor_value=p.values.x*SIGNAL_FLOOR;
    let fade=smoothstep(floor_value,SHADOW_FADE*floor_value,neutral);
    var gain=p.master.z;
    // Pin neutral range amounts to zero rather than relying on a reciprocal/log
    // round trip. Hue/Saturation edits must not change untouched brightness.
    if adjustment.z!=0. {
        var desired=neutral+adjustment.z;
        if desired<neutral*DARK_RESPONSE_KNEE && neutral>0. {
            let response=desired/neutral; let span=DARK_RESPONSE_KNEE+DARK_RESPONSE_OFFSET;
            desired=max(0.,neutral*(span*span/(2.*DARK_RESPONSE_KNEE+DARK_RESPONSE_OFFSET-response)-DARK_RESPONSE_OFFSET));
        }
        gain+=clamp(log2(max(desired,floor_value)/max(neutral,floor_value)),
            -MAX_TRANSFER_STOPS,MAX_TRANSFER_STOPS)*fade;
    }
    // A common chroma gate protects nearly neutral subjects, including master edits.
    let chroma_gate=smoothstep(0.,NEUTRAL_CHROMA_LIMIT,(hi-lo)/max(hi,floor_value))*fade;
    let hs=(adjustment.xy/max(hi-lo,floor_value)+p.master.xy)*chroma_gate;
    store_adjustment(i,vec3(hs,gain));
}
