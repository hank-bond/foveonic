// Native separable triangular polyphase resampling; source addressing is linear,
// including right taps entering the next row. Newly owned last four rows remain zero.
// Adapted with owner permission from Foveonic filtering/resample.rs at e47af35.
struct Params{source:vec4<u32>,output:vec4<u32>,advance:vec4<i32>}
@group(0) @binding(0) var<storage,read> input_pixels:array<f32>;
@group(0) @binding(1) var<storage,read> coefficients:array<f32>;
@group(0) @binding(2) var<uniform> params:Params;
@group(0) @binding(3) var<storage,read_write> output_pixels:array<f32>;
@group(0) @binding(4) var<storage,read> starts:array<i32>;
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id)id:vec3<u32>){
    let w=params.output.x;let h=params.output.y;if id.x>=w||id.y>=h*params.source.z{return;}let y=id.y%h;let tile=id.y/h;let components=params.source.w;let dest_index=(id.y*w+id.x)*components;var sum=vec3(0.0);
    if y<h-4u{let taps=params.output.z;let phases=params.output.w;let px=id.x%phases;let py=y%phases;let sx=starts[px]+i32(id.x/phases)*params.advance.x;let sy=starts[py]+i32(y/phases)*params.advance.x;
        for(var dy=0u;dy<taps;dy++){let yy=u32(max(sy+i32(dy),0));var horizontal=vec3(0.0);
            for(var dx=0u;dx<taps;dx++){let xx=u32(max(sx+i32(dx),0));let p=(tile*params.source.x*params.source.y+yy*params.source.x+xx)*components;var value=vec3(input_pixels[p]);if components==3u{value=vec3(input_pixels[p],input_pixels[p+1u],input_pixels[p+2u]);}horizontal+=value*coefficients[px*taps+dx];}
            sum+=horizontal*coefficients[py*taps+dy];
        }
    }
    output_pixels[dest_index]=sum.x;if components==3u{output_pixels[dest_index+1u]=sum.y;output_pixels[dest_index+2u]=sum.z;}
}
