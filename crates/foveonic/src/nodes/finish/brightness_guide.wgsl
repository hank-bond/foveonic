// Encoded brightness cells cover complete 2×2 image blocks, including odd edges.
fn brightness_cell_index(point: vec2<u32>, extent: vec2<u32>) -> u32 {
    let cell = point / 2u;
    return cell.y * ((extent.x + 1u) / 2u) + cell.x;
}

// Geometry is independent of the buffer that owns the compact scalar samples.
struct BrightnessGuideSampling {
    shape: vec4<u32>,
    bounds: vec4<u32>,
    mapping: array<vec4<f32>,2>,
};
struct BrightnessSample { indices: vec4<u32>, phase: vec2<f32> };
fn brightness_sample(point: vec2<f32>, guide: BrightnessGuideSampling) -> BrightnessSample {
    let p = vec3<f32>(point, 1.0);
    let sample = vec2<f32>(dot(guide.mapping[0].xyz,p), dot(guide.mapping[1].xyz,p));
    let extent = (guide.shape.xy+1u)/2u;
    let low = guide.bounds.xy/2u;
    let high = (guide.bounds.zw-1u)/2u;
    let cell = clamp((sample-0.5)*0.5, vec2<f32>(low), vec2<f32>(high));
    let lo = vec2<u32>(floor(cell));
    let hi = min(lo+1u,high);
    return BrightnessSample(vec4<u32>(lo.y*extent.x+lo.x, lo.y*extent.x+hi.x, hi.y*extent.x+lo.x, hi.y*extent.x+hi.x), fract(cell));
}
