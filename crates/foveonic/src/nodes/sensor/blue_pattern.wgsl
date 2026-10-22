struct Params {
    shape: vec4<u32>, controls: vec4<f32>
}
const NORMALIZE: u32 = 0u;
const DOWNSAMPLE: u32 = 1u;
const SMOOTH_BASELINE: u32 = 2u;
const CONTRAST: u32 = 3u;
const DETECT_CONTRAST: u32 = 4u;
const RESIZE_BLOCKERS: u32 = 5u;
const PHASE_FIELD: u32 = 6u;
const CLEAN_RUNS: u32 = 7u;
const REFINE_RUNS: u32 = 8u;
const REMOVE_ISOLATED: u32 = 9u;
const DILATE: u32 = 10u;
const ERODE: u32 = 11u;
const RESIZE_MASK: u32 = 12u;
const CORRECT_PAIRS: u32 = 13u;
const RESTORE_UNITS: u32 = 14u;
// Brightness is measured in sensor codes normalized to ISO 100.
const MIN_PATTERN_MEAN_CODE: f32 = 200.0;
const LOW_LIGHT_CUTOFF_CODE: f32 = 350.0;
const LOW_LIGHT_CONTRAST_LIMIT: f32 = 0.15;
const NORMAL_CONTRAST_LIMIT: f32 = 0.08;
const HORIZONTAL_PATTERN_RANGE: vec2<f32> = vec2<f32>(0.015, 0.4);
const VERTICAL_PATTERN_RANGE: vec2<f32> = vec2<f32>(0.02, 0.3);
const MAX_CONTRAST_GAP: u32 = 100u;
const MIN_RUN_SPAN: u32 = 4u;
const LONG_RUN_SPAN: u32 = 20u;
const BLOCKER_CONTEXT: i32 = 20;
const MIN_BLOCKER_COUNT: u32 = 30u;
const BLOCKER_DENSITY: f32 = 0.75;
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var<storage, read> source: array<f32>;
@group(0) @binding(2) var<storage, read> work: array<f32>;
@group(0) @binding(3) var<storage, read> mask: array<u32>;
@group(0) @binding(4) var<storage, read> auxiliary: array<u32>;
@group(0) @binding(5) var<storage, read_write> destination: array<f32>;
@group(0) @binding(6) var<storage, read_write> output_mask: array<u32>;
// Explicit FMA calls retain Float32 rounding between native arithmetic steps.
fn add(a: f32, b: f32) -> f32 {
    return fma(1.0, a, b);
}
fn mul(a: f32, b: f32) -> f32 {
    return fma(a, b, 0.0);
}
fn product(a: f32, b: f32) -> vec2<f32> {
    let high = mul(a, b);
    return vec2<f32>(high, fma(a, b, -high));
}
fn wide_add(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    let sum = add(a.x, b.x);
    let vb = add(sum, -a.x);
    let error = add(add(add(a.x, -add(sum, -vb)), add(b.x, -vb)), add(a.y, b.y));
    let high = add(sum, error);
    return vec2<f32>(high, add(error, -add(high, -sum)));
}
fn wide_mul(a: vec2<f32>, b: f32) -> vec2<f32> {
    return wide_add(product(a.x, b), product(a.y, b));
}
// Preserve Float32 division rounding in pair ratios and 3x3 mask means.
fn divide(a: f32, b: f32) -> f32 {
    let quotient = a/b;
    return fma(fma(-quotient, b, a), 1.0/b, quotient);
}
fn mean4(a: f32, b: f32, c: f32, d: f32) -> f32 {
    return mul(add(add(add(a, b), c), d), 0.25);
}
fn directional_mean(x: u32, y: u32, vertical: bool, baseline: bool) -> f32 {
    var values: array<f32, 4>;
    for(var k = 0u;k<4u;k++) {
        var point = vec2<u32>(x*4u+k, y);
        if vertical {
            point = vec2<u32>(x, y*4u+k);
        }
        let i = point.y*p.shape.x+point.x;
        values[k] = source[i];
        if baseline {
            values[k] = work[i];
        }
    }
    return mean4(values[0], values[1], values[2], values[3]);
}
fn compact_shape() -> vec2<u32> {
    if p.shape.w != 0u {
        return vec2<u32>(p.shape.x, p.shape.y/4u);
    }
    return vec2<u32>(p.shape.x/4u, p.shape.y);
}
fn field(x: u32, y: u32) -> f32 {
    let shape = compact_shape();
    let mean = directional_mean(x, y, p.shape.w != 0u, false);
    if p.shape.w == 0u {
        if y == 0u || y+1u == shape.y {
            return 0.0;
        }
        return add(add(mean, mean)/add(add(directional_mean(x, y-1u, false, false), directional_mean(x, y+1u, false, false)), 1.0), -1.0);
    }
    if x == 0u || x+1u == shape.x {
        return mean;
    }
    return add(add(mean, mean)/add(add(directional_mean(x-1u, y, true, false), directional_mean(x+1u, y, true, false)), 1.0), -1.0);
}
fn sign_value(v: f32) -> i32 {
    return select(select(0, 1, v>0.0), -1, v<0.0);
}
// Integer enlargement leaves two nonzero tent taps. Keep both accumulation
// passes wider than Float32, including the intermediate vertical samples.
fn interpolate_float(point: vec2<u32>, shape: vec2<u32>, factor: vec2<u32>) -> f32 {
    let center = (vec2<f32>(point)+vec2<f32>(0.5))/vec2<f32>(factor)-vec2<f32>(0.5);
    let base = vec2<i32>(floor(center));
    let weight = center-vec2<f32>(base);
    var columns: array<vec2<f32>, 2>;
    for(var x = 0;x<2;x++) {
        let xx = u32(clamp(base.x+x, 0, i32(shape.x)-1));
        let y0 = u32(clamp(base.y, 0, i32(shape.y)-1));
        let y1 = u32(clamp(base.y+1, 0, i32(shape.y)-1));
        columns[x] = wide_add(product(source[y0*shape.x+xx], 1.0-weight.y), product(source[y1*shape.x+xx], weight.y));
    }
    let value = wide_add(wide_mul(columns[0], 1.0-weight.x), wide_mul(columns[1], weight.x));
    return add(value.x, value.y);
}
fn interpolate_mask(point: vec2<u32>, shape: vec2<u32>, factor: vec2<u32>, offset: u32) -> u32 {
    let center = (vec2<f32>(point)+vec2<f32>(0.5))/vec2<f32>(factor)-vec2<f32>(0.5);
    let base = vec2<i32>(floor(center));
    let weight = center-vec2<f32>(base);
    var columns: array<f32, 2>;
    for(var x = 0;x<2;x++) {
        let xx = u32(clamp(base.x+x, 0, i32(shape.x)-1));
        let y0 = u32(clamp(base.y, 0, i32(shape.y)-1));
        let y1 = u32(clamp(base.y+1, 0, i32(shape.y)-1));
        columns[x] = f32(mask[offset+y0*shape.x+xx])*(1.0-weight.y)+f32(mask[offset+y1*shape.x+xx])*weight.y;
    }
    return u32(clamp(columns[0]*(1.0-weight.x)+columns[1]*weight.x, 0.0, 255.0));
}
fn line_position(line: u32, along: u32, vertical: bool) -> u32 {
    // Vertical phase correction refines horizontal rows. Horizontal phase
    // correction refines vertical columns.
    if vertical {
        return line*p.shape.x+along;
    }
    return along*(p.shape.x/4u)+line;
}
fn blocked(line: u32, along: u32, vertical: bool) -> u32 {
    var result = 0u;
    for(var k = 0u;k<4u;k++) {
        var point = vec2<u32>(line*4u+k, along);
        if vertical {
            point = vec2<u32>(along, line*4u+k);
        }
        result = max(result, mask[point.y*p.shape.x+point.x]);
    }
    return select(0u, 1u, result != 0u);
}
fn contrast_score(line: u32, along: u32, vertical: bool) -> f32 {
    let count = p.shape.x*p.shape.y/4u;
    var i = along*(p.shape.x/4u)+line;
    var offset = count*2u;
    if vertical {
        i = line*p.shape.x+along;
        offset = 0u;
    }
    let brightness = work[offset+count+i];
    return add(work[offset+i], -select(NORMAL_CONTRAST_LIMIT, LOW_LIGHT_CONTRAST_LIMIT, brightness <= LOW_LIGHT_CUTOFF_CODE));
}
fn mark_contrast(line: u32, start: u32, end: u32, vertical: bool) {
    if end-start <= 2u {
        return;
    }
    let count = p.shape.x*p.shape.y/4u;
    for(var i = start;i <= end;i++) {
        var at = count+i*(p.shape.x/4u)+line;
        if vertical {
            at = line*p.shape.x+i;
        }
        output_mask[at] = 1u;
    }
}
fn detect_contrast(line: u32, vertical: bool) {
    var length = p.shape.y;
    if vertical {
        length = p.shape.x;
    }
    var runs = 0u;
    var in_run = false;
    for(var i = 0u;i<length;i++) {
        let positive = contrast_score(line, i, vertical)>0.0;
        if positive && !in_run {
            runs++;
        }
        in_run = positive;
    }
    if runs <= 1u {
        return;
    }
    var start = 0u;
    var end = 0u;
    var have = false;
    in_run = false;
    for(var i = 0u;i<length;i++) {
        let positive = contrast_score(line, i, vertical)>0.0;
        if positive && !in_run {
            if !have {
                start = i;
                have = true;
            }
            else if i-end>MAX_CONTRAST_GAP {
                mark_contrast(line, start, end, vertical);
                start = i;
            }
        }
        if positive {
            end = i;
        }
        in_run = positive;
    }
    if have {
        mark_contrast(line, start, end, vertical);
    }
}
fn refine_run(line: u32, start: u32, end: u32, vertical: bool) {
    if end-start<MIN_RUN_SPAN {
        return;
    }
    var length = p.shape.y;
    if vertical {
        length = p.shape.x;
    }
    let left = u32(max(0, i32(start)-BLOCKER_CONTEXT));
    let right = min(length-1u, end+u32(BLOCKER_CONTEXT));
    var blockers = 0u;
    for(var i = left;i <= right;i++) {
        blockers+=blocked(line, i, vertical);
    }
    if blockers >= MIN_BLOCKER_COUNT && f32(blockers) >= f32(right-left+1u)*BLOCKER_DENSITY {
        return;
    }
    let multi = !vertical && line >= 2u && line+2u<p.shape.x/4u;
    for(var i = start;i <= end;i++) {
        var keep = end-start >= LONG_RUN_SPAN;
        if !keep && multi {
            var total = 0.0;
            for(var k = -2;k <= 2;k++) {
                if k != 0 {
                    total = add(total, work[line_position(u32(i32(line)+k), i, false)]);
                }
            }
            keep = blocked(line, i, false) == 0u || total >= 4.0;
        }
        else if !keep && blocked(line, i, vertical) == 0u {
            var neighbors = 0u;
            for(var k = -2;k <= 2;k++) {
                let at = i32(i)+k;
                if k != 0 && at >= 0 && at<i32(length) {
                    neighbors+=select(0u, 1u, work[line_position(line, u32(at), vertical)] != 0.0);
                }
            }
            keep = neighbors >= 3u;
        }
        if keep {
            output_mask[line_position(line, i, vertical)] = 1u;
        }
    }
}
fn refined_line(line: u32, vertical: bool) {
    var length = p.shape.y;
    if vertical {
        length = p.shape.x;
    }
    var in_run = false;
    var start = 0u;
    for(var i = 0u;i<length;i++) {
        let positive = work[line_position(line, i, vertical)]>0.0;
        if positive && !in_run {
            start = i;
        }
        if !positive && in_run {
            refine_run(line, start, i-1u, vertical);
        }
        in_run = positive;
    }
    if in_run {
        refine_run(line, start, length-1u, vertical);
    }
}
fn extended_mask(i: i32) -> u32 {
    let n = p.shape.x*p.shape.y;
    if i<0 {
        return 0u;
    }
    if u32(i)<n {
        return mask[u32(i)];
    }
    if p.shape.w == 0u && u32(i)-n<n/4u {
        return auxiliary[u32(i)-n];
    }
    return 0u;
}
@compute @workgroup_size(8, 8)
fn blue_pattern(@builtin(global_invocation_id) id: vec3<u32>) {
    let width = p.shape.x;
    let height = p.shape.y;
    let n = width*height;
    let stage = p.shape.z;
    let vertical = p.shape.w != 0u;
    if stage == DETECT_CONTRAST {
        if id.y != 0u {
            return;
        }
        if id.x<height/4u {
            detect_contrast(id.x, true);
        }
        else if id.x<height/4u+width/4u {
            detect_contrast(id.x-height/4u, false);
        }
        return;
    }
    if stage == CLEAN_RUNS {
        let shape = compact_shape();
        if vertical {
            if any(id.xy >= shape) {
                return;
            }
            destination[id.y*shape.x+id.x] = work[id.y*shape.x+id.x];
            return;
        }
        if id.y != 0u || id.x >= shape.x {
            return;
        }
        var previous2 = 0.0;
        var previous = 0.0;
        for(var y = 0u;y<shape.y;y++) {
            var value = work[y*shape.x+id.x];
            if y >= 2u && y+2u<shape.y && value>0.0 {
                let next = work[(y+1u)*shape.x+id.x];
                let next2 = work[(y+2u)*shape.x+id.x];
                if (previous2 <= 0.0 || previous <= 0.0) && (previous <= 0.0 || next <= 0.0) && (next <= 0.0 || next2 <= 0.0) {
                    value = 0.0;
                }
            }
            destination[y*shape.x+id.x] = value;
            previous2 = previous;
            previous = value;
        }
        return;
    }
    if stage == REFINE_RUNS {
        if id.y != 0u {
            return;
        }
        var lines = width/4u;
        if vertical {
            lines = height/4u;
        }
        if id.x<lines {
            refined_line(id.x, vertical);
        }
        return;
    }
    if stage == DOWNSAMPLE {
        if id.x >= width/2u || id.y >= height/2u {
            return;
        }
        let i = id.y*2u*width+id.x*2u;
        destination[id.y*(width/2u)+id.x] = mean4(source[i], source[i+1u], source[i+width], source[i+width+1u]);
        return;
    }
    if stage == CONTRAST {
        if id.x >= width || id.y >= height/4u {
            return;
        }
        let count = n/4u;
        let vi = id.y*width+id.x;
        let vm = directional_mean(id.x, id.y, true, false);
        let vs = directional_mean(id.x, id.y, true, true);
        destination[vi] = select(0.0, abs(add(vm, -vs))/add(vs, 1.0), vm >= MIN_PATTERN_MEAN_CODE);
        destination[count+vi] = vs;
        let hi = vi;
        let hx = hi%(width/4u);
        let hy = hi/(width/4u);
        let hm = directional_mean(hx, hy, false, false);
        let hs = directional_mean(hx, hy, false, true);
        destination[count*2u+hi] = select(0.0, abs(add(hm, -hs))/add(hs, 1.0), hm >= MIN_PATTERN_MEAN_CODE);
        destination[count*3u+hi] = hs;
        return;
    }
    if stage >= PHASE_FIELD && stage <= RESIZE_MASK && stage != CLEAN_RUNS && stage != REFINE_RUNS {
        let shape = compact_shape();
        if stage == RESIZE_MASK {
            if id.x >= width || id.y >= height {
                return;
            }
            var factor = vec2<u32>(4u, 1u);
            if vertical {
                factor = vec2<u32>(1u, 4u);
            }
            output_mask[id.y*width+id.x] = interpolate_mask(id.xy, shape, factor, 0u);
            return;
        }
        if any(id.xy >= shape) {
            return;
        }
        let i = id.y*shape.x+id.x;
        if stage == PHASE_FIELD {
            var seed = false;
            let value = field(id.x, id.y);
            if id.x>0u && id.y>0u && id.x+1u<shape.x && id.y+1u<shape.y {
                let s = sign_value(value);
                var along = vec2<u32>(1u, 0u);
                var across = vec2<u32>(0u, 1u);
                if vertical {
                    along = vec2<u32>(0u, 1u);
                    across = vec2<u32>(1u, 0u);
                }
                seed = s == sign_value(field(id.x-along.x, id.y-along.y)) && s == sign_value(field(id.x+along.x, id.y+along.y)) && s != sign_value(field(id.x-across.x, id.y-across.y)) && s != sign_value(field(id.x+across.x, id.y+across.y));
            }
            let mean = directional_mean(id.x, id.y, vertical, false);
            var low = HORIZONTAL_PATTERN_RANGE.x;
            var high = HORIZONTAL_PATTERN_RANGE.y;
            var selected = 1.0;
            if vertical {
                low = VERTICAL_PATTERN_RANGE.x;
                high = VERTICAL_PATTERN_RANGE.y;
                selected = abs(value);
            }
            destination[i] = select(0.0, selected, seed && abs(value) >= low && abs(value)<high && mean >= MIN_PATTERN_MEAN_CODE);
            return;
        }
        if stage == REMOVE_ISOLATED {
            var value = mask[i];
            if value != 0u && id.x>0u && id.y>0u && id.x+1u<shape.x && id.y+1u<shape.y {
                var neighbors = 0u;
                for(var dy = -1;dy <= 1;dy++) {
                    for(var dx = -1;dx <= 1;dx++) {
                        if dx != 0 || dy != 0 {
                            neighbors+=mask[u32(i32(id.y)+dy)*shape.x+u32(i32(id.x)+dx)];
                        }
                    }
                }
                if neighbors == 0u {
                    value = 0u;
                }
            }
            output_mask[i] = value;
            return;
        }
        var value = 0u;
        var inside = id.x>0u && id.x+1u<shape.x && id.y>0u && id.y+1u<shape.y;
        if vertical {
            inside = id.x+2u<shape.x && id.y>0u && id.y+1u<shape.y;
        }
        if inside {
            let a = mask[i-shape.x];
            let b = mask[i-1u];
            let c = mask[i];
            let d = mask[i+1u];
            let e = mask[i+shape.x];
            if stage == DILATE {
                value = select(0u, 1u, (a|b|c|d|e) != 0u);
            }
            else {
                value = select(0u, 1u, a != 0u && b != 0u && c != 0u && d != 0u && e != 0u);
            }
        }
        output_mask[i] = value;
        return;
    }
    if id.x >= width || id.y >= height {
        return;
    }
    let i = id.y*width+id.x;
    if stage == NORMALIZE {
        destination[i] = source[i]/p.controls.y;
        return;
    }
    if stage == SMOOTH_BASELINE {
        destination[i] = interpolate_float(id.xy, vec2<u32>(width/2u, height/2u), vec2<u32>(2u));
        return;
    }
    if stage == RESIZE_BLOCKERS {
        output_mask[i] = interpolate_mask(id.xy, vec2<u32>(width, height/4u), vec2<u32>(1u, 4u), 0u)+interpolate_mask(id.xy, vec2<u32>(width/4u, height), vec2<u32>(4u, 1u), n/4u);
        return;
    }
    if stage == CORRECT_PAIRS {
        if vertical && (id.x<2u || id.x+2u >= width || id.y<2u || id.y+2u >= height) {
            destination[i] = source[i];
            return;
        }
        var first = i-id.x%2u;
        var second = first+1u;
        if vertical {
            first = i-(id.y%2u)*width;
            second = first+width;
        }
        let total = mul(add(source[first], source[second]), 0.5);
        let baseline_mean = mul(add(work[first], work[second]), 0.5);
        let ratio = divide(baseline_mean, add(total, 1.0));
        var total_mask = 0u;
        for(var dy = -1;dy <= 1;dy++) {
            for(var dx = -1;dx <= 1;dx++) {
                total_mask+=extended_mask(i32(i)+dy*i32(width)+dx);
            }
        }
        let amount = mul(p.controls.x, divide(f32(total_mask), 9.0));
        var value = source[i];
        if amount>0.0 {
            value = add(mul(mul(value, ratio), amount), mul(add(1.0, -amount), value));
        }
        destination[i] = value;
        return;
    }
    if stage == RESTORE_UNITS {
        destination[i] = mul(source[i], p.controls.y);
    }
}
