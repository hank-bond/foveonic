// Encoded cells cover complete image blocks, including odd edges.
fn brightness_cell_index(point: vec2<u32>, extent: vec2<u32>, step: u32) -> u32 {
    let cell = point / step;
    return cell.y * ((extent.x + step - 1u) / step) + cell.x;
}

// shape.z is the guide cell step, shape.w is the incoming gain-anchor step.
struct BrightnessGuideSampling {
    shape: vec4<u32>,
    bounds: vec4<u32>,
    mapping: array<vec4<f32>,2>,
};
struct BrightnessSample { indices: vec4<u32>, phase: vec2<f32> };
fn brightness_sample(point: vec2<f32>, guide: BrightnessGuideSampling) -> BrightnessSample {
    // Affines map sample-index coordinates. Move from a gain anchor to its
    // block center before mapping, then from guide pixels to guide cell centers.
    let p = vec3<f32>(point + f32(guide.shape.w - 1u) * 0.5, 1.0);
    let sample = vec2<f32>(dot(guide.mapping[0].xyz,p), dot(guide.mapping[1].xyz,p));
    let step = guide.shape.z;
    let extent = (guide.shape.xy + step - 1u) / step;
    let low = guide.bounds.xy / step;
    let high = (guide.bounds.zw - 1u) / step;
    let cell = clamp((sample - f32(step - 1u) * 0.5) / f32(step), vec2<f32>(low), vec2<f32>(high));
    let lo = vec2<u32>(floor(cell));
    let hi = min(lo+1u,high);
    return BrightnessSample(vec4<u32>(lo.y*extent.x+lo.x, lo.y*extent.x+hi.x, hi.y*extent.x+lo.x, hi.y*extent.x+hi.x), fract(cell));
}
