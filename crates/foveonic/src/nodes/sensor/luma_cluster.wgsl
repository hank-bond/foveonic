struct Params {
    shape: vec4<u32>, candidates: vec4<u32>
}
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> points: array<vec4<u32>>;
@group(0) @binding(2) var<storage, read> candidates: array<u32>;
@group(0) @binding(3) var<storage, read_write> blue: array<f32>;
const CANDIDATE_LANES = 256u;
const PATCH_RADIUS = 2;
const PATCH_SIDE = 5u;
const PATCH_SAMPLES = PATCH_SIDE * PATCH_SIDE;
const ORIENTATION_COUNT = 6u;
const TARGET_TERMS = ORIENTATION_COUNT * PATCH_SAMPLES;
// Every candidate uses the same oriented target and support mask.
var<workgroup> target_terms: array<vec2<f32>, TARGET_TERMS>;
var<workgroup> scores: array<f32, CANDIDATE_LANES>;
var<workgroup> choices: array<u32, CANDIDATE_LANES>;
const TRANSFORMS: array<vec4<i32>, ORIENTATION_COUNT> = array<vec4<i32>, ORIENTATION_COUNT>(
    vec4(1, 0, 0, 1), vec4(0, -1, 1, 0), vec4(0, 1, 1, 0),
    vec4(1, 0, 0, -1), vec4(-1, 0, 0, 1), vec4(-1, 0, 0, -1),
);

fn position(i: u32, dx: i32, dy: i32)->u32 {
    return u32(i32(i)+dy*i32(params.shape.x)+dx);
}

fn patch_score(candidate: u32)->f32 {
    var best = 3.402823e38;
    for (var orientation = 0u;orientation<ORIENTATION_COUNT;orientation++) {
        var score = 0.;
        // Retain transformed patch order and the original Float32 additions.
        for (var sy = -PATCH_RADIUS;sy <= PATCH_RADIUS;sy++) {
            for (var sx = -PATCH_RADIUS;sx <= PATCH_RADIUS;sx++) {
                let patch_index = u32((sy + PATCH_RADIUS) * i32(PATCH_SIDE) + sx + PATCH_RADIUS);
                let term = target_terms[orientation * PATCH_SAMPLES + patch_index];
                let difference = abs(fma(term.x, blue[position(candidate, sx, sy)], -term.y));
                score = fma(1., score, difference);
            }
        }
        best = min(best, score);
    }
    return best;
}

@compute @workgroup_size(CANDIDATE_LANES)

fn luma_cluster(@builtin(local_invocation_index) lane: u32) {
    for (var p = 0u;p<params.shape.w;p++) {
        let point = points[params.shape.z+p];
        // Reload after each ordered repair so later points see its correction.
        if lane < TARGET_TERMS {
            let m = TRANSFORMS[lane / PATCH_SAMPLES];
            let determinant = m.x*m.w-m.y*m.z;
            let sx = i32(lane % PATCH_SIDE) - PATCH_RADIUS;
            let sy = i32((lane % PATCH_SAMPLES) / PATCH_SIDE) - PATCH_RADIUS;
            let dx = (m.w*sx-m.y*sy)*determinant;
            let dy = (-m.z*sx+m.x*sy)*determinant;
            let mask_index = u32((dy + PATCH_RADIUS) * i32(PATCH_SIDE) + dx + PATCH_RADIUS);
            let weight = f32((point.y >> mask_index) & 1u);
            let original = fma(weight, blue[position(point.x, dx, dy)], 0.);
            target_terms[lane] = vec2(weight, original);
        }
        workgroupBarrier();
        var best = 3.402823e38;
        var choice = 0xffffffffu;
        for (var candidate = lane;candidate<params.candidates.y;candidate += CANDIDATE_LANES) {
            let score = patch_score(candidates[params.candidates.x+candidate]);
            if score<best {
                best = score;
                choice = candidate;
            }
        }
        scores[lane] = best;
        choices[lane] = choice;
        workgroupBarrier();
        for (var stride = CANDIDATE_LANES / 2u; stride > 0u; stride /= 2u) {
            if lane < stride {
                let other = lane + stride;
                if scores[other] < scores[lane] || (scores[other] == scores[lane] && choices[other] < choices[lane]) {
                    scores[lane] = scores[other];
                    choices[lane] = choices[other];
                }
            }
            workgroupBarrier();
        }
        if lane == 0u {
            let selected = choices[0];
            if selected != 0xffffffffu {
                blue[point.x] = blue[candidates[params.candidates.x+selected]];
            } else {
                blue[point.x] = 0.;
            }
        }
        storageBarrier();
        workgroupBarrier();
    }
}
