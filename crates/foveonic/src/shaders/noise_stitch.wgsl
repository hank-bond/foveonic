// Project reference processing tiles into owned full-sensor image outputs.
@group(0) @binding(0) var<storage, read> tiles: array<f32>;
@group(0) @binding(1) var<uniform> params: NoiseParams;
@group(0) @binding(2) var<storage, read_write> output_pixels: array<f32>;
fn value(tile: u32, xy: vec2<u32>, channel:u32, components: u32) -> f32 {
    return tiles[tile_pixel(vec3(xy,tile),params)*components+channel];
}
@compute @workgroup_size(8, 8)
fn stitch(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= params.source.xy) { return; }
    let components = u32(params.weights.w);
    let core = params.source.xy / params.source.zw;
    let count = params.source.z * params.source.w;
    // Geometry/ramps are shared across RGB components, with unchanged arithmetic.
    var result:array<f32,3>;
    if params.diffusion.w == 2. {
        let tile_xy = id.xy / core;
        for(var c=0u;c<components;c++){result[c]=value(tile_xy.y * params.source.z + tile_xy.x, id.xy % core + params.tile.z,c,components);}
    } else {
        // Reverse tile order preserves the reference's eight-pixel overlap ramps.
        for (var tile = i32(count) - 1; tile >= 0; tile--) {
            let tx = u32(tile) % params.source.z;
            let ty = u32(tile) / params.source.z;
            let origin = vec2<i32>(vec2(tx, ty) * core);
            let local = vec2<i32>(id.xy) - origin + i32(params.tile.z);
            let first_x = origin.x - select(4, 0, tx == 0u);
            let first_y = origin.y - select(4, 0, ty == 0u);
            let last_x = origin.x + i32(core.x) - 1 - select(4, 0, tx + 1u == params.source.z);
            let last_y = origin.y + i32(core.y) - 1 - select(4, 0, ty + 1u == params.source.w);
            if ty + 1u < params.source.w && i32(id.y) >= last_y && i32(id.y) <= last_y + 8 && i32(id.x) >= first_x && i32(id.x) < last_x {
                let weight = f32(last_y + 8 - i32(id.y)) * 0.125;
                for(var c=0u;c<components;c++){result[c]=weight*value(u32(tile),vec2<u32>(local),c,components)+(1.-weight)*result[c];}
            }
            if i32(id.y) >= first_y && i32(id.y) <= last_y {
                if i32(id.x) >= first_x && i32(id.x) <= last_x {for(var c=0u;c<components;c++){result[c]=value(u32(tile),vec2<u32>(local),c,components);}}
                let boundary = origin.x + i32(core.x) - 4;
                if tx + 1u < params.source.z && i32(id.x) >= boundary && i32(id.x) <= boundary + 8 {
                    let weight = 1. - f32(i32(id.x) - boundary) * 0.125;
                    for(var c=0u;c<components;c++){result[c]=weight*value(u32(tile),vec2<u32>(local),c,components)+(1.-weight)*result[c];}
                }
            }
        }
    }
    let p=(id.y*params.source.x+id.x)*components;
    for(var c=0u;c<components;c++){output_pixels[p+c]=result[c];}
}
