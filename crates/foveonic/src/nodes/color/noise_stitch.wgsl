// Project native processing tiles into owned whole-frame or regional images.
// Native blend geometry. Keep support in sync with Parameters::for_region.
const TILE_BLEND_SPAN: i32 = 8;
const TILE_BLEND_HALF: i32 = 4;
const TILE_BLEND_SUPPORT: u32 = 5;
const TILE_BLEND_WEIGHT_STEP: f32 = 0.125;

@group(0) @binding(0) var<storage, read> tiles: array<f32>;
@group(0) @binding(1) var<uniform> params: NoiseParams;
@group(0) @binding(2) var<storage, read_write> output_pixels: array<f32>;
fn value(tile: u32, xy: vec2<u32>, channel: u32, components: u32) -> f32 {
    return tiles[tile_pixel(vec3(xy, tile), params) * components + channel];
}
@compute @workgroup_size(8, 8)
fn stitch(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= params.region.zw - params.region.xy) { return; }
    let components = u32(params.weights.w);
    let core = params.source.xy / params.source.zw;
    let xy = id.xy + params.region.xy;
    // Geometry/ramps are shared across RGB components, with unchanged arithmetic.
    var result: array<f32, 3>;
    // Overlap weights depend on tile order. Reversing this traversal matches
    // the reference implementation's eight-pixel blend.
    // Only neighboring cores can contribute to the eight-pixel seam.
    // Five includes its asymmetric endpoint and preserves zero-weight writes.
    let seam_support = vec2(TILE_BLEND_SUPPORT);
    let first = max(params.window.xy, (xy - min(xy, seam_support)) / core);
    let last = min(params.window.xy + params.window.zw - 1u, (xy + seam_support) / core);
    for (var row = i32(last.y); row >= i32(first.y); row--) {
        for (var column = i32(last.x); column >= i32(first.x); column--) {
            let tx = u32(column);
            let ty = u32(row);
            let tile = (ty - params.window.y) * params.window.z + tx - params.window.x;
            let origin = vec2<i32>(vec2(tx, ty) * core);
            let local = vec2<i32>(xy) - origin + i32(params.tile.z);
            let first_x = origin.x - select(TILE_BLEND_HALF, 0, tx == 0u);
            let first_y = origin.y - select(TILE_BLEND_HALF, 0, ty == 0u);
            let last_x = origin.x + i32(core.x) - 1 - select(TILE_BLEND_HALF, 0, tx + 1u == params.source.z);
            let last_y = origin.y + i32(core.y) - 1 - select(TILE_BLEND_HALF, 0, ty + 1u == params.source.w);
            if ty + 1u < params.source.w && i32(xy.y) >= last_y && i32(xy.y) <= last_y + TILE_BLEND_SPAN && i32(xy.x) >= first_x && i32(xy.x) < last_x {
                let weight = f32(last_y + TILE_BLEND_SPAN - i32(xy.y)) * TILE_BLEND_WEIGHT_STEP;
                for (var c = 0u; c < components; c++) {
                    result[c] = weight * value(tile, vec2<u32>(local), c, components) + (1. - weight) * result[c];
                }
            }
            if i32(xy.y) >= first_y && i32(xy.y) <= last_y {
                if i32(xy.x) >= first_x && i32(xy.x) <= last_x {
                    for (var c = 0u; c < components; c++) {
                        result[c] = value(tile, vec2<u32>(local), c, components);
                    }
                }
                let boundary = origin.x + i32(core.x) - TILE_BLEND_HALF;
                if tx + 1u < params.source.z && i32(xy.x) >= boundary && i32(xy.x) <= boundary + TILE_BLEND_SPAN {
                    let weight = 1. - f32(i32(xy.x) - boundary) * TILE_BLEND_WEIGHT_STEP;
                    for (var c = 0u; c < components; c++) {
                        result[c] = weight * value(tile, vec2<u32>(local), c, components) + (1. - weight) * result[c];
                    }
                }
            }
        }
    }
    let p = (id.y * (params.region.z - params.region.x) + id.x) * components;
    for (var c = 0u; c < components; c++) {
        output_pixels[p + c] = result[c];
    }
}
