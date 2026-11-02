// Shared subject-color classification for photographic edits and on-image picking.
const SIGNAL_FLOOR: f32 = 0.00001;
const SHADOW_FADE: f32 = 4.;
// Full range selection is reached at five percent guide chroma.
const NEUTRAL_CHROMA_LIMIT: f32 = 0.05;
fn range_weights(c: vec3<f32>) -> array<f32,8> {
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
    return weights;
}
