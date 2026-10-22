struct Params {
    shape: vec4<u32>, counts: vec4<u32>, controls: vec4<u32>, gains: vec4<f32>
}
@group(0) @binding(0) var<storage, read> source: array<f32>;
@group(0) @binding(1) var<storage, read> prior_mask: array<u32>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read> jobs: array<vec4<u32>>;
@group(0) @binding(4) var<storage, read> marks: array<vec2<u32>>;
@group(0) @binding(5) var<storage, read_write> blue: array<f32>;
@group(0) @binding(6) var<storage, read_write> mask: array<u32>;
const NEIGHBORS: array<vec2<i32>, 8 >= array<vec2<i32>, 8>(vec2(-1, -1), vec2(0, -1), vec2(1, -1), vec2(-1, 0), vec2(1, 0), vec2(-1, 1), vec2(0, 1), vec2(1, 1));

fn plus(a: f32, b: f32)->f32 {
    return fma(1., a, b);
}

fn minus(a: f32, b: f32)->f32 {
    return fma(-1., b, a);
}

fn product(a: f32, b: f32)->f32 {
    return fma(a, b, 0.);
}

fn divide(a: f32, b: f32)->f32 {
    let q = a/b;
    if a == 0. || b == 0. || q != q || abs(q)>3.402823e38 {
        return q;
    }
    return plus(q, fma(-q, b, a)/b);
}

fn index(p: vec2<i32>)->u32 {
    return u32(p.y)*params.shape.x+u32(p.x);
}

fn sample(p: vec2<i32>)->f32 {
    return blue[index(p)];
}

var<workgroup> search_needed: u32;
var<workgroup> shared_job: vec4<u32>;
var<workgroup> search_scores: array<f32, 64>;
var<workgroup> search_choices: array<u32, 64>;

fn search(p: vec2<i32>, values: array<f32, 8>, valid: array<bool, 8>, fused: bool, lane: u32)->f32 {
    var best = 3.402823e38;
    var choice = 0xffffffffu;
    // Candidate order is row-major. Each lane preserves its own score order,
    // and the reduction breaks ties by the original candidate index.
    for (var candidate_index = lane; candidate_index < 441u; candidate_index += 64u) {
        let dy = i32(candidate_index / 21u) - 10;
        let dx = i32(candidate_index % 21u) - 10;
        let candidate = p+vec2(dx, dy);
        if (dx == 0 && dy == 0) || mask[index(candidate)] == 2u {
            continue;
        }
        var score = 0.;
        for (var k = 0u;k<8u;k++) {
            let d = minus(sample(candidate+NEIGHBORS[k]), values[k]);
            if valid[k] {
                if fused {
                    score = fma(d, d, score);
                } else {
                    score = plus(score, product(d, d));
                }
            }
        }
        if score<best {
            best = score;
            choice = candidate_index;
        }
    }
    search_scores[lane] = best;
    search_choices[lane] = choice;
    workgroupBarrier();
    for (var stride = 32u; stride > 0u; stride /= 2u) {
        if lane < stride {
            let other = lane + stride;
            if search_scores[other] < search_scores[lane] || (search_scores[other] == search_scores[lane] && search_choices[other] < search_choices[lane]) {
                search_scores[lane] = search_scores[other];
                search_choices[lane] = search_choices[other];
            }
        }
        workgroupBarrier();
    }
    let selected = search_choices[0];
    if selected == 0xffffffffu { return sample(p); }
    return sample(p + vec2(i32(selected % 21u) - 10, i32(selected / 21u) - 10));
}

fn directional(p: vec2<i32>, lane: u32) {
    let at = index(p);
    var values: array<f32, 8>;
    var valid: array<bool, 8>;
    for (var k = 0u;k<8u;k++) {
        values[k] = sample(p+NEIGHBORS[k]);
        valid[k] = true;
    }
    let a = values[0];
    let b = values[1];
    let c = values[2];
    let d = values[3];
    let f = values[4];
    let g = values[5];
    let h = values[6];
    let i = values[7];
    let first = plus(plus(plus(abs(minus(d, a)), abs(minus(d, g))), abs(minus(f, c))), abs(minus(f, i)));
    let second = plus(plus(plus(abs(minus(b, a)), abs(minus(b, c))), abs(minus(h, g))), abs(minus(h, i)));
    var result = product(plus(d, f), .5);
    if first <= second {
        result = product(plus(b, h), .5);
    }
    var total = 0.;
    for (var k = 0u;k<8u;k++) {
        total = plus(total, values[k]);
    }
    let mean = product(total, .125);
    let order = array<u32, 8>(7, 6, 5, 4, 3, 2, 0, 1);
    var deviation = 0.;
    for (var k = 0u;k<8u;k++) {
        deviation = plus(deviation, abs(minus(mean, values[order[k]])));
    }
    if lane == 0u { search_needed = u32(mean == 0. || divide(product(deviation, .125), mean)>.05); }
    if workgroupUniformLoad(&search_needed) != 0u {
        result = search(p, values, valid, true, lane);
    }
    if lane == 0u {
        blue[at] = result;
        mask[at] = 1u;
    }
}

fn dark_offset(p: vec2<i32>, flags: u32) {
    var maximum = 0.;
    for (var dy = -1;dy <= 1;dy++) {
        for (var dx = -1;dx <= 1;dx++) {
            let q = p+vec2(dx, dy);
            if mask[index(q)] == 0u {
                maximum = max(maximum, sample(q));
            }
        }
    }
    blue[index(p)] = min(maximum, fma(params.gains.x, f32(flags&0x1fffu), sample(p)));
}

fn general(p: vec2<i32>, lane: u32) {
    let at = index(p);
    if lane == 0u { search_needed = u32(mask[at] == 1u); }
    if workgroupUniformLoad(&search_needed) != 0u {
        return;
    }
    var values: array<f32, 8>;
    var valid: array<bool, 8>;
    var total = 0.;
    var count = 0u;
    var low = 3.402823e38;
    var high = -3.402823e38;
    for (var k = 0u;k<8u;k++) {
        let offset = NEIGHBORS[k];
        var eligibility = offset;
        if eligibility.y == 1 {
            eligibility.y = -1;
        }
        values[k] = sample(p+offset);
        valid[k] = mask[index(p+eligibility)] != 2u;
        if valid[k] {
            total = plus(total, values[k]);
            count++;
            low = min(low, values[k]);
            high = max(high, values[k]);
        }
    }
    // The finite reference contract requires a nonempty eligible neighborhood.
    if lane == 0u { search_needed = u32(count == 0u); }
    if workgroupUniformLoad(&search_needed) != 0u {
        return;
    }
    let mean = divide(total, f32(count));
    var result = mean;
    if count>2u {
        result = divide(minus(total, plus(low, high)), f32(count-2u));
    }
    var deviation = 0.;
    for (var k = 0u;k<8u;k++) {
        if valid[k] {
            deviation = plus(deviation, abs(minus(mean, values[k])));
        }
    }
    if lane == 0u { search_needed = u32(mean == 0. || divide(divide(deviation, f32(count)), mean)>.025); }
    if workgroupUniformLoad(&search_needed) != 0u {
        result = search(p, values, valid, false, lane);
    }
    if lane == 0u {
        blue[at] = result;
        mask[at] = 1u;
    }
}

@compute @workgroup_size(64)

fn ordered_blue_repair(@builtin(global_invocation_id) id: vec3<u32>, @builtin(workgroup_id) group: vec3<u32>, @builtin(local_invocation_index) lane: u32) {
    let phase = params.controls.x;
    let slot = id.y*params.counts.z+id.x;
    if phase == 0u {
        if slot >= params.controls.z { return; }
        blue[slot] = source[slot];
        if params.controls.w != 0u {
            mask[slot] = prior_mask[slot];
        }
        else {
            mask[slot] = 0u;
        }
        return;
    }
    if phase == 1u {
        if slot >= params.controls.z { return; }
        let mark = marks[slot];
        mask[mark.x] = mark.y;
        return;
    }
    let job_index = group.y*(params.counts.z/64u)+group.x;
    if job_index >= params.controls.z { return; }
    if lane == 0u { shared_job = jobs[params.controls.y+job_index]; }
    let job = workgroupUniformLoad(&shared_job);
    let p = vec2<i32>(i32(job.y), i32(job.x));
    if params.shape.z == 0u {
        directional(p, lane);
    }
    else if params.shape.z == 1u {
        if lane == 0u { dark_offset(p, job.z); }
    }
    else {
        general(p, lane);
    }
}
