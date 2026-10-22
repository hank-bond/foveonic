struct Params {
    shape: vec4<u32>
}
@group(0) @binding(0) var<storage, read> source: array<u32>;
@group(0) @binding(1) var<uniform> params: Params;
@group(0) @binding(2) var<storage, read_write> destination: array<u32>;

fn weights(coordinate: u32)->vec4<i32> {
    return select(vec4(-3, 29, 111, -9), vec4(-9, 111, 29, -3), (coordinate%2u) != 0u);
}

@compute @workgroup_size(8, 8)

fn byte_mask_double(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.shape.z || id.y >= params.shape.w {
        return;
    }
    let wx = weights(id.x);
    let wy = weights(id.y);
    let left = i32((id.x+1u)/2u)-2;
    let top = i32((id.y+1u)/2u)-2;
    var sum = 0;
    // Each phase coefficient is an exact integer divided by128. The separable
    // Double reference therefore has this exact numerator, bounded below6million
    // for valid byte inputs. Clamp before integer division, preserving truncation.
    for (var dy = 0u;dy<4u;dy++) {
        let y = u32(clamp(top+i32(dy), 0, i32(params.shape.y)-1));
        for (var dx = 0u;dx<4u;dx++) {
            let x = u32(clamp(left+i32(dx), 0, i32(params.shape.x)-1));
            sum += i32(source[y*params.shape.x+x])*wx[dx]*wy[dy];
        }
    }
    destination[id.y*params.shape.z+id.x] = u32(clamp(sum, 0, 255*16384)/16384);
}
