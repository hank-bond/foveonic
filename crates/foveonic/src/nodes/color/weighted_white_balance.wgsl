// Image arithmetic stays on the GPU. Only histograms and a few neutral statistics leave it.
// The host mirrors this uniform layout. Unused fourth lanes keep 16-byte alignment.
struct Params {
    shape: vec4<u32>, // original width/height, working width/height
    sampling: vec4<u32>, // margins x/y, tone-sample width/height
    camera: array<vec4<f32>, 3>,
    tone: array<vec4<f32>, 3>,
    saturation: vec4<f32>,
    control: vec4<f32>, // exposure scale, chroma radius, unused, unused
    white: vec4<f32>, // candidate XYZ white point, normalized to Y=100
    minimum: vec4<i32>, // selected signed tone-code minima
    maximum: vec4<i32>, // selected signed tone-code maxima
    shields: array<vec4<u32>, 2>,
    drift: vec4<f32>,
    flare: vec4<f32>,
    polynomial: array<vec4<i32>, 9>,
    options: vec4<u32>, // estimate drift, apply polynomial, reduce input, unused
}
@group(0) @binding(0) var<storage,read> src:array<f32>;
@group(0) @binding(1) var<uniform> p:Params;
@group(0) @binding(2) var<storage,read> aux:array<f32>;
@group(0) @binding(3) var<storage,read_write> dst:array<f32>;
@group(0) @binding(4) var<storage,read> tables:array<i32>;
@group(0) @binding(5) var<storage,read_write> counters:array<atomic<i32>>;
fn pixel(i:u32,stride:u32)->vec3<f32> {
    let j=i*stride;
    return vec3(src[j],src[j+1u],src[j+2u]);
}
fn apply_matrix(m:array<vec4<f32>,3>,v:vec3<f32>)->vec3<f32> {
    var out=vec3(0.);
    for(var c=0u;c<3u;c++) {
        out[c]=v.g*m[c].g+v.r*m[c].r+v.b*m[c].b;
    }
    return out;
}
fn working(id:vec3<u32>)->bool {
    return id.x<p.shape.z&&id.y<p.shape.w;
}
fn wrap_signed(v:i32)->i32 {
    return (v<<16)>>16;
}
@compute @workgroup_size(1) fn black_statistics() {
    var mean=vec3(0.);
    var count=0u;
    for(var y=2u;y<p.shape.y;y+=4u) {
        for(var x=2u;x<p.shape.x;x+=4u) {
            mean+=pixel(y*p.shape.x+x,3u);
            count++;
        }
    }
    mean=mean/f32(max(1u,count))*p.flare.xyz;
    var drift=p.drift.xyz;
    if(p.options.x!=0u) {
        var means:array<vec3<f32>,2>;
        for(var j=0u;j<2u;j++) {
            let r=p.shields[j];
            var sum=vec3(0.);
            var n=0u;
            for(var y=r.y;y<r.w;y++) {
                for(var x=r.x;x<r.z;x++) {
                    sum+=pixel(y*p.shape.x+x,3u);
                    n++;
                }
            }
            means[j]=sum/f32(max(1u,n));
        }
        drift=(means[0]+means[1])*.5;
        if(abs(means[0].r)>4000.) {
            drift=vec3(0.);
        }
    }
    for(var c=0u;c<3u;c++) {
        dst[c]=mean[c]+drift[c];
    }
    dst[3]=0.;
}
@compute @workgroup_size(8,8) fn reduce(@builtin(global_invocation_id) id:vec3<u32>) {
    if(!working(id)) {
        return;
    }
    let i=id.y*p.shape.z+id.x;
    var value=vec3(0.);
    if(p.options.z!=0u) {
        for(var y=0u;y<8u;y++) {
            for(var x=0u;x<4u;x++) {
                value+=pixel((p.sampling.y+id.y*8u+y)*p.shape.x+p.sampling.x+id.x*4u+x,3u);
            }
        }
        value=value/32.-vec3(aux[0],aux[1],aux[2]);
        if(p.options.y!=0u) {
            let v=vec3<i32>(value+vec3(.5));
            let squares=bitcast<vec3<u32>>(v*v)>>vec3(14u);
            let terms=array<i32,9>(i32(squares.r),i32(squares.g),i32(squares.b),(v.r*v.g)>>14,(v.b*v.r)>>14,(v.b*v.g)>>14,(i32(squares.r)*v.r)>>14,(i32(squares.g)*v.g)>>14,(i32(squares.b)*v.b)>>14);
            var corrected=v;
            for(var c=0u;c<3u;c++) {
                var sum=0;
                for(var t=0u;t<9u;t++) {
                    sum+=terms[t]*p.polynomial[c*3u+t/3u][t%3u];
                }
                corrected[c]+=sum>>14;
            }
            value=vec3<f32>(corrected);
        }
    }
    else {
        value=pixel(i,3u);
    }
    let tone=apply_matrix(p.tone,value);
    for(var c=0u;c<3u;c++) {
        dst[i*8u+c]=value[c];
        dst[i*8u+3u+c]=f32(wrap_signed(i32(tone[c])));
        atomicMax(&counters[3u+c],bitcast<i32>(max(0.,value[c])));
    }
    dst[i*8u+6u]=0.;
    dst[i*8u+7u]=0.;
}
@compute @workgroup_size(8,8) fn sample_tone(@builtin(global_invocation_id) id:vec3<u32>) {
    if(id.x>=p.sampling.z||id.y>=p.sampling.w) {
        return;
    }
    let x=13u+4u*id.x;
    let y=19u+4u*id.y;
    let i=id.y*p.sampling.z+id.x;
    // The reference header includes a possible unpopulated fringe. Define it by
    // repeating the last complete sample, retaining its populated crop elsewhere.
    let xx=min(x,p.shape.z-5u);
    let yy=min(y,p.shape.w-21u);
    var v=vec3(0.);
    for(var k=0u;k<4u;k++) {
        let j=(yy*p.shape.z+xx+k)*8u;
        v+=vec3(src[j+3u],src[j+4u],src[j+5u]);
    }
    for(var c=0u;c<3u;c++) {
        let code=i32(v[c]/4.);
        dst[i*4u+c]=f32(code);
        atomicMax(&counters[c],code);
    }
    dst[i*4u+3u]=0.;
}
@compute @workgroup_size(8,8) fn auto_histogram(@builtin(global_invocation_id) id:vec3<u32>) {
    let w=p.sampling.z;
    let h=p.sampling.w;
    let mx=u32(f32(w)*.1);
    let my=u32(f32(h)*.1);
    if(id.x<mx||id.y<my||id.x>=w-mx||id.y>=h-my) {
        return;
    }
    let v=vec3<i32>(pixel(id.y*w+id.x,4u));
    let l=i32(f32(v.g)*.5+f32(v.r)*.3+f32(v.b)*.2);
    let b=u32(max(0,tables[u32(l)&65535u]/16));
    if(b<2048u) {
        atomicAdd(&counters[32768u+b],1);
    }
    let scale=vec3<u32>(536870911u)/vec3<u32>(p.maximum.xyz);
    let pos=scale*vec3<u32>(max(v,vec3(0)));
    let bin=(pos.r>>24u)+((pos.g>>19u)&4064u)+((pos.b>>14u)&130048u);
    if(bin<32768u) {
        atomicAdd(&counters[bin],1);
    }
}
@compute @workgroup_size(8,8) fn tone_image(@builtin(global_invocation_id) id:vec3<u32>) {
    if(!working(id)) {
        return;
    }
    let i=id.y*p.shape.z+id.x;
    for(var c=0u;c<3u;c++) {
        dst[i*8u+c]=src[i*8u+c];
        let code=i32(src[i*8u+3u+c]);
        let root=tables[u32(code)&65535u];
        dst[i*8u+3u+c]=f32(tables[65536u+c*65536u+(u32(root)&65535u)]);
    }
    dst[i*8u+6u]=0.;
    dst[i*8u+7u]=0.;
}
fn lab(v:f32)->f32 {
    if(v<=.008856) {
        return v*7.787+16./116.;
    }
    if(v<15.) {
        return aux[u32((v+1.)*625.)];
    }
    return pow(v,1./3.);
}
fn distance(v:vec3<f32>,white:vec3<f32>)->vec2<f32> {
    let xyz=apply_matrix(p.camera,v)*p.control.x*.0156/white;
    let l=vec3(lab(xyz.x),lab(xyz.y),lab(xyz.z));
    let a=(l.x-l.y)*500.;
    let b=(l.y-l.z)*200.;
    return vec2(a*a+b*b,116.*l.y-16.);
}
@compute @workgroup_size(1) fn scores() {
    let candidates=array<vec3<f32>,6>(vec3(94.9466,100.,138.4398),vec3(94.38,100.,117.898),vec3(95.0456,100.,108.9058),vec3(95.5475,100.,77.986),vec3(102.8524,100.,49.5528),vec3(108.854,100.,33.4394));
    // Candidate scoring uses the first contiguous quarter of the working plane.
    let n=p.shape.z*p.shape.w/4u;
    for(var c=0u;c<6u;c++) {
        var sum=0.;
        var compensation=0.;
        for(var i=0u;i<n;i++) {
            let term=sqrt(distance(pixel(i,8u),candidates[c]).x)-compensation;
            let total=sum+term;
            compensation=(total-sum)-term;
            sum=total;
        }
        dst[c]=sum/f32(n);
    }
}
@compute @workgroup_size(8,8) fn select_neutral(@builtin(global_invocation_id) id:vec3<u32>) {
    if(!working(id)) {
        return;
    }
    let i=id.y*p.shape.z+id.x;
    let v=pixel(i,8u);
    var d=vec2(-1.,0.);
    if(all(v<p.saturation.xyz)) {
        d=distance(v,p.white.xyz);
    }
    let selected=d.x>=0.&&d.x<p.control.y*p.control.y&&d.y>15.;
    dst[i]=select(-1.,d.x,selected);
    if(selected) {
        for(var c=0u;c<3u;c++) {
            let value=i32(src[i*8u+3u+c]);
            atomicMin(&counters[c],value);
            atomicMax(&counters[3u+c],value);
        }
        atomicAdd(&counters[6],1);
    }
}
fn bin_for(i:u32)->u32 {
    var bins=vec3<u32>();
    for(var c=0u;c<3u;c++) {
        bins[c]=u32((src[i*8u+3u+c]-f32(p.minimum[c]))*32./f32(p.maximum[c]-p.minimum[c]+1));
    }
    return bins.r*1024u+bins.g*32u+bins.b;
}
@compute @workgroup_size(8,8) fn neutral_histogram(@builtin(global_invocation_id) id:vec3<u32>) {
    if(working(id)) {
        let i=id.y*p.shape.z+id.x;
        if(aux[i]>=0.) {
            atomicAdd(&counters[bin_for(i)],1);
        }
    }
}
@compute @workgroup_size(1) fn accumulate() {
    // Compensated accumulation limits Float32 reduction error.
    var sum=vec3(0.);
    var compensation=vec3(0.);
    for(var i=0u;i<p.shape.z*p.shape.w;i++) {
        if(aux[i]<0.) {
            continue;
        }
        let at=u32(sqrt(aux[i])+.5);
        // Refinement replaces only weights 0..29. Rounded index 30 retains
        // its radius-45 coefficient from the first pass.
        let radius=select(p.control.y,45.,p.control.y==30.&&at>=30u);
        var coefficient=pow(1.-pow(f32(at)/radius,3.),3.);
        // The adjacent reference table entry read by rounded index 45 is -1.
        if(at==45u) {
            coefficient=-1.;
        }
        let weight=inverseSqrt(f32(atomicLoad(&counters[bin_for(i)])))*coefficient;
        let term=pixel(i,8u)*weight-compensation;
        let total=sum+term;
        compensation=(total-sum)-term;
        sum=total;
    }
    for(var c=0u;c<3u;c++) {
        dst[c]=sum[c];
    }
    dst[3]=0.;
}
// Stage preparation measures all original signed layers before its Q14 gain.
// Store integer row sums as bits, preserving their exact value through readback.
@compute @workgroup_size(64) fn row_means(@builtin(global_invocation_id) id:vec3<u32>) {
    if(id.x>=p.shape.y) {
        return;
    }
    let black=vec3<i32>(vec3(aux[0],aux[1],aux[2]));
    var sum=vec3<i32>(0);
    for(var x=0u;x<p.shape.x;x++) {
        sum+=vec3<i32>(pixel(id.x*p.shape.x+x,3u))-black;
    }
    for(var c=0u;c<3u;c++) {
        dst[id.x*3u+c]=bitcast<f32>(sum[c]);
    }
}
