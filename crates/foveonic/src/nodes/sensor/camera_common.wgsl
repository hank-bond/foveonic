// Ordered masked arithmetic shared by camera-list and synthetic reference replay.
// Adapted with permission from Foveonic repair.rs at e47af35.
const camera_offsets = array<vec2<i32>, 8>(vec2(-1,-1),vec2(0,-1),vec2(1,-1),vec2(-1,0),vec2(1,0),vec2(-1,1),vec2(0,1),vec2(1,1));
fn camera_color(p:u32)->vec3<f32>{return vec3(pixels[p*3u],pixels[p*3u+1u],pixels[p*3u+2u]);}
fn camera_anchor(original:vec3<f32>,sum:vec3<f32>,c:u32,a:u32,denominator:f32)->f32{return original[a]+(sum[c]-sum[a])/denominator;}
fn camera_correct(record:vec2<u32>,width:u32,height:u32){
    let p=record.x;let xy=vec2<i32>(i32(p%width),i32(p/width));let mask=record.y&255u;
    if xy.x<2 || xy.y<2 || xy.x>=i32(width)-1 || xy.y>=i32(height)-1 || mask==0u{return;}
    var sum=vec3(0.0);var low=vec3(0.0);var high=vec3(0.0);var count=0.0;
    for(var n=0u;n<8u;n++){
        if(mask&(1u<<n))==0u{continue;}
        let q=xy+camera_offsets[n];let v=camera_color(u32(q.y)*width+u32(q.x));
        low=select(min(low,v),v,all(low==vec3(0.0)));high=select(max(high,v),v,all(high==vec3(0.0)));
        sum+=v;count+=1.0;
    }
    var denominator=count;if count>2.0{sum-=high+low;denominator=count-2.0;}
    let original=camera_color(p);var result=original;
    switch record.y>>12u{
        case 1u:{result.z=(camera_anchor(original,sum,2u,0u,denominator)+camera_anchor(original,sum,2u,1u,denominator))*0.5;}
        case 2u:{result.y=(camera_anchor(original,sum,1u,0u,denominator)+camera_anchor(original,sum,1u,2u,denominator))*0.5;}
        case 3u:{result.y=camera_anchor(original,sum,1u,0u,denominator);result.z=camera_anchor(original,sum,2u,0u,denominator);}
        case 4u:{result.x=(camera_anchor(original,sum,0u,1u,denominator)+camera_anchor(original,sum,0u,2u,denominator))*0.5;}
        case 5u:{result.x=camera_anchor(original,sum,0u,1u,denominator);result.z=camera_anchor(original,sum,2u,1u,denominator);}
        case 6u:{result.x=camera_anchor(original,sum,0u,2u,denominator);result.y=camera_anchor(original,sum,1u,2u,denominator);}
        case 7u:{result=sum/denominator;}
        default:{}
    }
    if all(abs(result)<=vec3(3.402823e38)){pixels[p*3u]=result.x;pixels[p*3u+1u]=result.y;pixels[p*3u+2u]=result.z;}
}
