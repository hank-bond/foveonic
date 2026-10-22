struct Params {
    shape: vec4<u32>, candidates: vec4<u32>
}
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> points: array<vec4<u32>>;
@group(0) @binding(2) var<storage, read> candidates: array<u32>;
@group(0) @binding(3) var<storage, read_write> blue: array<f32>;
const CANDIDATE_LANES = 256u;
var<workgroup> target_patch: array<f32, 25>;
var<workgroup> scores: array<f32, CANDIDATE_LANES>;
var<workgroup> choices: array<u32, CANDIDATE_LANES>;
const TRANSFORMS: array<vec4<i32>, 6 >= array<vec4<i32>, 6>(vec4(1, 0, 0, 1), vec4(0, -1, 1, 0), vec4(0, 1, 1, 0), vec4(1, 0, 0, -1), vec4(-1, 0, 0, 1), vec4(-1, 0, 0, -1));

fn position(i: u32, dx: i32, dy: i32)->u32 {
    return u32(i32(i)+dy*i32(params.shape.x)+dx);
}

fn patch_score(point: vec4<u32>, candidate: u32)->f32 {
    var best = 3.402823e38;
    for (var orientation = 0u;orientation<6u;orientation++) {
        let m = TRANSFORMS[orientation];
        let determinant = m.x*m.w-m.y*m.z;
        var score = 0.;
        // Sum in transformed patch order, preserving the native orientation's
        // Float32 accumulation rather than merely permuting a finished sum.
        for (var sy = -2;sy <= 2;sy++) {
            for (var sx = -2;sx <= 2;sx++) {
                let dx = (m.w*sx-m.y*sy)*determinant;
                let dy = (-m.z*sx+m.x*sy)*determinant;
                let weight = f32((point.y>>u32((dy+2)*5+dx+2))&1u);
                let original = fma(weight, target_patch[u32((dy+2)*5+dx+2)], 0.);
                let difference = abs(fma(weight, blue[position(candidate, sx, sy)], -original));
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
        // Every candidate compares the same target patch. Reload after each
        // ordered repair so later points still see the preceding correction.
        if lane < 25u {
            target_patch[lane] = blue[position(point.x, i32(lane % 5u) - 2, i32(lane / 5u) - 2)];
        }
        workgroupBarrier();
        var best = 3.402823e38;
        var choice = 0xffffffffu;
        for (var candidate = lane;candidate<params.candidates.y;candidate += CANDIDATE_LANES) {
            let score = patch_score(point, candidates[params.candidates.x+candidate]);
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
