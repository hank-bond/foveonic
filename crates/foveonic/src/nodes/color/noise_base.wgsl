// Adapted from Foveonic filtering and local_mean components at e47af35.
@group(0) @binding(0) var<storage, read> input_pixels: array<f32>;
@group(0) @binding(1) var<uniform> params: NoiseParams;
@group(0) @binding(2) var<storage, read_write> output_pixels: array<f32>;
@group(0) @binding(3) var<storage, read> edges: array<f32>;
@group(0) @binding(4) var<storage, read_write> scalar_work: array<f32>;
@group(0) @binding(5) var<storage, read_write> prefix: array<f32>;
@group(0) @binding(6) var<storage, read> table: array<f32>;
@group(0) @binding(7) var<storage, read_write> radii: array<u32>;

fn rgb(index: u32) -> vec3<f32> {
    let p = index * 3u;
    return vec3(input_pixels[p], input_pixels[p + 1u], input_pixels[p + 2u]);
}
fn put(index: u32, value: vec3<f32>) {
    let p = index * 3u;
    output_pixels[p] = value.x;
    output_pixels[p + 1u] = value.y;
    output_pixels[p + 2u] = value.z;
}
@compute @workgroup_size(8, 8)
fn tile_input(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * tile_count(params) { return; }
    let p = atlas_position(id.xy, params);
    let xy = full_position(p, params);
    var value = rgb(xy.y * params.source.x + xy.x);
    if any(xy < params.area.xy) || any(xy >= params.area.zw) { value = vec3(1000.); }
    put(tile_pixel(p, params), value);
}

// Global reduced data uses native CopyImageToTile's first-edge extension, not
// the local raw-input second-edge repeats or working-rectangle fill.
@compute @workgroup_size(8,8)
fn global_tile(@builtin(global_invocation_id)id:vec3<u32>){
    if id.x>=params.tile.x||id.y>=params.tile.y*tile_count(params){return;}
    let p=atlas_position(id.xy,params);let core=params.source.xy/params.source.zw;let origin=native_tile(p.z,params)*core;
    let xy=vec2<u32>(clamp(vec2<i32>(origin+p.xy)-i32(params.tile.z),vec2(0),vec2<i32>(params.source.xy)-1));let source=xy.y*params.source.x+xy.x;let dest_index=tile_pixel(p,params);
    put(dest_index,rgb(source));
}
@compute @workgroup_size(8,8)
fn global_tile_scalar(@builtin(global_invocation_id)id:vec3<u32>){
    if id.x>=params.tile.x||id.y>=params.tile.y*tile_count(params){return;}
    let p=atlas_position(id.xy,params);let core=params.source.xy/params.source.zw;let origin=native_tile(p.z,params)*core;
    let xy=vec2<u32>(clamp(vec2<i32>(origin+p.xy)-i32(params.tile.z),vec2(0),vec2<i32>(params.source.xy)-1));scalar_work[tile_pixel(p,params)]=input_pixels[xy.y*params.source.x+xy.x];
}
// Preserve the native transposed F backing seed explicitly, including compact
// reduced LocalMean writes at its original allocation start. Global F is zero.
@compute @workgroup_size(8,8)
fn pattern_seed(@builtin(global_invocation_id)id:vec3<u32>){
    if id.x>=params.tile.x||id.y>=params.tile.y*tile_count(params){return;}let p=atlas_position(id.xy,params);var value=0.0;
    if params.geometry.w==0u{let reduced=vec2<u32>(vec2<f32>(params.tile.xy)*params.reduction.x);let index=p.x*params.tile.y+p.y;let x=index/reduced.y;let y=index%reduced.y;let margin=params.geometry.x;
        if x>=margin&&x+margin<reduced.x&&y>=margin&&y+margin<reduced.y{value=input_pixels[(p.z*reduced.y+y)*reduced.x+x];}
    }
    scalar_work[tile_pixel(p,params)]=value;
}
@compute @workgroup_size(8, 8)
fn block_mean(@builtin(global_invocation_id) id: vec3<u32>) {
    let grid = (params.tile.xy + 31u) / 32u;
    if id.x >= grid.x || id.y >= grid.y * tile_count(params) { return; }
    let tile = id.y / grid.y;
    let row = id.y % grid.y;
    let first = vec2<i32>(vec2(id.x * 32u, row * 32u)) - 1;
    let last = min(first + 32, vec2<i32>(params.tile.xy));
    var sum = 0.;
    for (var y = max(0, first.y); y < last.y; y++) {
        for (var x = max(0, first.x); x < last.x; x++) {
            let v = rgb(tile_pixel(vec3<u32>(u32(x), u32(y), tile), params));
            sum += min(v.x, min(v.y, v.z));
        }
    }
    let nominal = min(vec2(32u), params.tile.xy - vec2(id.x, row) * 32u);
    scalar_work[id.y * grid.x + id.x] = sum / f32(nominal.x * nominal.y);
}
@compute @workgroup_size(8, 8)
fn radius_map(@builtin(global_invocation_id) id: vec3<u32>) {
    let grid = (params.tile.xy + 31u) / 32u;
    if id.x >= grid.x || id.y >= grid.y * tile_count(params) { return; }
    let tile = id.y / grid.y;
    let row = id.y % grid.y;
    var value = 1e30;
    for (var y = max(0, i32(row) - 1); y <= min(i32(grid.y) - 1, i32(row) + 1); y++) {
        for (var x = max(0, i32(id.x) - 1); x <= min(i32(grid.x) - 1, i32(id.x) + 1); x++) {
            value = min(value, scalar_work[(tile * grid.y + u32(y)) * grid.x + u32(x)]);
        }
    }
    let high = params.edge.w > 400.;
    var radius = select(2u, 3u, high);
    if value < 700. { radius = select(3u, 5u, high); }
    if value < 500. { radius = select(4u, 6u, high); }
    if value < 300. { radius = select(5u, 7u, high); }
    if value < 150. { radius = 9u; }
    radii[id.y * grid.x + id.x] = radius;
}

@compute @workgroup_size(32, 8)
fn bilateral(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * tile_count(params) { return; }
    let p = atlas_position(id.xy, params);
    let pixel = tile_pixel(p, params);
    let center = rgb(pixel);
    let half = params.tile.w / 2u;
    var filtered = vec3(0.);
    let global=params.geometry.w!=0u;
    let last_row=select(params.tile.y-half,params.tile.y-4u,global);
    if p.x >= half && p.x < params.tile.x - half - 1u && p.y >= half && p.y < last_row {
        var radius=i32(half);var step=2;
        if !global{
            let grid = (params.tile.xy + 31u) / 32u;
            let row = (p.y + 1u) / 32u;
            let column = (p.x + 1u) / 32u;
            var radius_index = (p.z * grid.y + row) * grid.x + column;
            // Preserve the reference's carried radius at the beginning of each row.
            if column == 0u && p.y != half && (p.y + 1u) % 32u != 0u {
                radius_index = (p.z * grid.y + (p.y / 32u)) * grid.x + (params.tile.x - half - 1u) / 32u;
            }
            radius=i32(radii[radius_index]);step=select(1,2,radius>3);
        }
        let inverse = (2047. / (8. / params.filtering.x)) / variance(center, params);
        var sum = vec3(0.);
        var total = 0.;
        var horizontal_radius = radius;
        if i32(p.y) >= radius {
            for (var y = i32(p.y) - radius; y <= min(i32(params.tile.y) - 1, i32(p.y) + radius); y++) {
                if i32(p.x) >= horizontal_radius {
                    for (var x = i32(p.x) - horizontal_radius; x <= i32(p.x) + horizontal_radius; x += step) {
                        let local=u32(y)*params.tile.x+u32(x);var sample=vec3(0.0);
                        // Reduced B is a compact prefix of zeroed per-tile scratch, not the next tile's source.
                        if params.reduction.x==1.0||local<params.tile.x*params.tile.y{sample=rgb(p.z*params.tile.x*params.tile.y+local);}
                        let d = sample - center;
                        let distance = dot(d * d, inverse);
                        let index = u32(round(distance));
                        if index < 2048u {
                            let weight = table[index];
                            sum = fma(sample, vec3(weight), sum);
                            total += weight;
                        }
                    }
                }
                horizontal_radius = 2 * radius - (step - 1) - horizontal_radius;
            }
        }
        filtered = center;
        if total > 0. { filtered = sum / total; }
    }
    let border = clamp(p.xy, vec2(half), params.tile.xy - half - 1u);
    var original = rgb(tile_pixel(vec3(border, p.z), params));if global{original=center;}
    put(pixel, params.filtering.y * filtered + (1. - params.filtering.y) * original);
}

fn sample_at(p: vec3<u32>, delta: vec2<i32>) -> vec3<f32> {
    let position=vec2<i32>(p.xy)+delta;
    // Native rotating RGB rows leave both outside columns zero, without shift.
    if position.x<=0 || position.x>=i32(params.tile.x)-1{return vec3(0.0);}
    let xy=vec2<u32>(clamp(position,vec2(0),vec2<i32>(params.tile.xy)-1));
    return rgb(tile_pixel(vec3(xy,p.z),params))+params.edge.y;
}
fn median_cross(p: vec3<u32>, delta: vec2<i32>) -> vec3<f32> {
    let x=i32(p.x)+delta.x;if x<=0||x>=i32(params.tile.x)-1{return vec3(0.0);}
    var values = array<vec3<f32>, 5>(sample_at(p, delta), sample_at(p, delta + vec2(-1, 0)), sample_at(p, delta + vec2(1, 0)), sample_at(p, delta + vec2(0, -1)), sample_at(p, delta + vec2(0, 1)));
    for (var i = 1u; i < 5u; i++) {
        for (var j = i; j > 0u; j--) {
            let low = min(values[j - 1u], values[j]);
            values[j] = max(values[j - 1u], values[j]);
            values[j - 1u] = low;
        }
    }
    return values[2];
}
@compute @workgroup_size(8,8)
fn cross_medians(@builtin(global_invocation_id)id:vec3<u32>){
    if id.x>=params.tile.x||id.y>=params.tile.y*tile_count(params){return;}
    let p=atlas_position(id.xy,params);put(tile_pixel(p,params),median_cross(p,vec2(0)));
}
fn cached_median(p:vec3<u32>,delta:vec2<i32>)->vec3<f32>{
    let position=vec2<i32>(p.xy)+delta;
    if position.x<=0||position.x>=i32(params.tile.x)-1{return vec3(0.0);}
    let xy=vec2<u32>(clamp(position,vec2(0),vec2<i32>(params.tile.xy)-1));
    return rgb(tile_pixel(vec3(xy,p.z),params));
}
fn gradient(p: vec3<u32>, offset: vec2<i32>) -> f32 {
    let x=i32(p.x)+offset.x;if x<=0||x>=i32(params.tile.x)-1{return 0.0;}
    let center = cached_median(p, offset);
    let inverse = 1. / max(params.model[0].xyz + params.model[1].xyz * center, vec3(1e-12));
    let dx = (cached_median(p, offset + vec2(-1, 0)) - cached_median(p, offset + vec2(1, 0))) * inverse;
    let dy = (cached_median(p, offset + vec2(0, -1)) - cached_median(p, offset + vec2(0, 1))) * inverse;
    return sqrt((max(dx.x * dx.x, max(dx.y * dx.y, dx.z * dx.z)) + max(dy.x * dy.x, max(dy.y * dy.y, dy.z * dy.z))) * 0.5);
}
@compute @workgroup_size(8, 8)
fn edge_map(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * tile_count(params) { return; }
    let p = atlas_position(id.xy, params);
    var value = 0.;
    if p.x > 0u && p.x + 1u < params.tile.x && p.y >= 3u && p.y + 3u < params.tile.y {
        value = (gradient(p, vec2(0)) + gradient(p, vec2(-1, 0)) + gradient(p, vec2(1, 0)) + gradient(p, vec2(0, -1)) + gradient(p, vec2(0, 1))) * 0.2;
    }
    let margin=params.geometry.y;
    if p.x >= margin+1u && p.x+margin+1u < params.tile.x && p.y >= margin && p.y+margin < params.tile.y {
        let cap = 4. / params.edge.x;
        let index = u32(min(value, cap) * (2047. / cap));
        value = table[2048u + min(index, 2047u)];
    }
    scalar_work[tile_pixel(p, params)] = value;
}

// The recursive horizontal scan reads adjacent rows in adjacent lanes. Pack
// each tile transposed so those reads are contiguous, preserving every sample
// and the serial recurrence order. The vertical scan keeps its native layout.
fn transposed_pixel(p: vec3<u32>) -> u32 {
    return (p.z * params.tile.x + p.x) * params.tile.y + p.y;
}
@compute @workgroup_size(8, 8)
fn transpose_mean_input(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * tile_count(params) { return; }
    let p = atlas_position(id.xy, params);
    let src = tile_pixel(p, params);
    let dst = transposed_pixel(p);
    put(dst, rgb(src));
    scalar_work[dst] = edges[src];
}
@compute @workgroup_size(8, 8)
fn transpose_mean_output(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.tile.x || id.y >= params.tile.y * tile_count(params) { return; }
    let p = atlas_position(id.xy, params);
    put(transposed_pixel(p), rgb(tile_pixel(p, params)));
}
fn line_pixel(line: u32, position: u32, vertical: bool) -> u32 {
    let lines = select(params.tile.y, params.tile.x, vertical);
    let tile = line / lines;
    let row = line % lines;
    let xy = select(vec2(position, row), vec2(row, position), vertical);
    return tile_pixel(vec3(xy, tile), params);
}
@compute @workgroup_size(64)
fn local_mean(@builtin(global_invocation_id) id: vec3<u32>) {
    let vertical = params.diffusion.w > 0.;
    let lines = select(params.tile.y, params.tile.x, vertical);
    let length = select(params.tile.x, params.tile.y, vertical);
    if id.x >= lines * tile_count(params) { return; }
    let row = id.x % lines;
    let margin=params.geometry.x;
    if row < margin || row+margin >= lines {
        for(var x=0u;x<length;x++){let pixel=line_pixel(id.x,x,vertical);put(pixel,rgb(pixel));}
        return;
    }
    let first=margin;
    let last=length-margin-1u;
    // The backward pass already writes the interior. Copying it here would
    // overwrite the filtered values, so only the margins need copying.
    for(var x=0u;x<first;x++){let pixel=line_pixel(id.x,x,vertical);put(pixel,rgb(pixel));}
    for(var x=last+1u;x<length;x++){let pixel=line_pixel(id.x,x,vertical);put(pixel,rgb(pixel));}
    var sum = vec3(0.);
    var weight = 0.;
    for (var x = first; x <= last; x++) {
        let pixel = line_pixel(id.x, x, vertical);
        let e = select(edges[pixel], 0., x == first);
        sum = e * sum + rgb(pixel);
        weight = e * weight + 1.;
        let q = pixel * 3u;
        prefix[q] = sum.x; prefix[q + 1u] = sum.y; prefix[q + 2u] = sum.z;
        scalar_work[pixel] = weight;
    }
    sum = vec3(0.); weight = 0.;
    for (var x = i32(last); x >= i32(first); x--) {
        let pixel = line_pixel(id.x, u32(x), vertical);
        let e = select(edges[pixel], 0., u32(x) == last);
        let sample=rgb(pixel);
        sum = e * sum + sample;
        weight = e * weight + 1.;
        let q = pixel * 3u;
        var value = (vec3(prefix[q], prefix[q + 1u], prefix[q + 2u]) + sum - sample) / (scalar_work[pixel] + weight - 1.);
        if u32(x) == first { value = sum / weight; }
        if u32(x) == last { value = vec3(prefix[q], prefix[q + 1u], prefix[q + 2u]) / scalar_work[pixel]; }
        put(pixel, value);
    }
}
