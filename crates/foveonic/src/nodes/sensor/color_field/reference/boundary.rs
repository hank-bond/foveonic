//! Checks boundary evidence, independent tone equations, and native application around the pinned color solver.
use super::{
    color_field, highlight_color, tone_anchor, BrightnessField, Control, DngCtx, LocalRecovery,
    ReferenceResult,
};

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
fn onset(x: f64) -> f64 {
    let t = x.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn quantize(x: f64) -> u8 {
    (255. * x).round().clamp(0., 255.) as u8
}
fn result(
    m: [f64; 3],
    n: [f64; 3],
    amplitude: f64,
    strength: f64,
    available: bool,
) -> tone_anchor::Result {
    tone_anchor::Result {
        samples: if available {
            std::array::from_fn(|c| m[c] + strength * (n[c] * amplitude - m[c]))
        } else {
            m
        },
        amplitude,
        strength,
        available,
    }
}
fn tone(
    m: [f64; 3],
    original: [u8; 3],
    effective: [u8; 3],
    n: [f64; 3],
    y: [f64; 3],
) -> tone_anchor::Result {
    let ny = dot(n, y);
    let my = dot(m, y);
    if original == [255; 3] && effective != [255; 3] {
        let strength = onset(2. * (1. - f64::from(*effective.iter().min().unwrap()) / 255.));
        if my.is_finite() && ny > 1e-12 && m.iter().all(|v| *v >= 0.) {
            let layers = (0..3).map(|c| m[c] / n[c]).sum::<f64>() / 3.;
            if layers > 0. && layers.is_finite() {
                let held = (my / ny).max(0.);
                let trust = onset((held / layers - 0.25) / 0.5);
                return result(m, n, layers * (1. - trust) + held * trust, strength, true);
            }
        }
        if my > 0. && my.is_finite() && ny > 1e-12 {
            return result(m, n, my / ny, strength, true);
        }
        return original_tone(m, effective, n, y);
    }
    original_tone(m, original, n, y)
}
fn original_tone(m: [f64; 3], mask: [u8; 3], n: [f64; 3], y: [f64; 3]) -> tone_anchor::Result {
    let original = tone_anchor::recover_severe(m, mask, n, y);
    if original.available
        || mask == [255; 3]
        || m.iter().any(|v| !v.is_finite() || *v < 0.)
        || dot(n, y) <= 1e-12
    {
        return original;
    }
    let q = mask.map(|v| (f64::from(v) / 255.).powi(2));
    let support = q.iter().sum::<f64>();
    if support <= 0. {
        return original;
    }
    let amplitude = (0..3).map(|c| q[c] * m[c] / n[c]).sum::<f64>() / support;
    if !amplitude.is_finite() || amplitude <= 0. {
        return original;
    }
    result(
        m,
        n,
        amplitude,
        onset(2. * (1. - f64::from(*mask.iter().min().unwrap()) / 255.)),
        true,
    )
}

pub(crate) fn influences(width: usize, height: usize, raw: &[[f64; 3]]) -> Vec<f64> {
    let severe: Vec<_> = raw
        .iter()
        .map(|v| v.iter().filter(|v| **v >= f64::from(0.97_f32)).count() >= 2)
        .collect();
    (0..raw.len())
        .map(|p| {
            let x = (p % width) as isize;
            let y = (p / width) as isize;
            let mut nearest = 100_f64;
            for dy in -4_isize..=4 {
                for dx in -4_isize..=4 {
                    let xx = x + dx;
                    let yy = y + dy;
                    if xx >= 0
                        && yy >= 0
                        && xx < width as isize
                        && yy < height as isize
                        && severe[yy as usize * width + xx as usize]
                    {
                        nearest = nearest.min(((dx * dx + dy * dy) as f64).sqrt());
                    }
                }
            }
            1. - onset(nearest / 4.)
        })
        .collect()
}
pub(crate) fn evidence_masks(
    pixels: &[[f64; 3]],
    masks: &[[u8; 3]],
    influence: &[f64],
    n: [f64; 3],
    y: [f64; 3],
) -> Vec<[u8; 3]> {
    pixels
        .iter()
        .zip(masks)
        .zip(influence)
        .map(|((&m, &q), &distance)| {
            let weights = q.map(|v| (f64::from(v) / 255.).powi(2));
            let support = weights.iter().sum::<f64>();
            let ny = dot(n, y);
            let mut gate = 0.;
            if m.iter().all(|v| v.is_finite() && *v >= 0.) && ny > 1e-12 {
                let layers = if support > 0. {
                    (0..3).map(|c| weights[c] * m[c] / n[c]).sum::<f64>() / support
                } else {
                    (0..3).map(|c| m[c] / n[c]).fold(0_f64, f64::max)
                };
                if layers > 0. && layers.is_finite() {
                    gate = 1. - onset(((dot(m, y) / ny).max(0.) / layers - 0.25) / 0.5);
                }
            }
            let ceiling = quantize(1. - distance * gate);
            q.map(|v| v.min(ceiling))
        })
        .collect()
}

struct Row {
    diagonal: f64,
    rhs: f64,
    neighbors: Vec<(usize, f64)>,
}
fn product(rows: &[Row], x: &[f64]) -> Vec<f64> {
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            row.diagonal * x[i] - row.neighbors.iter().map(|&(j, w)| w * x[j]).sum::<f64>()
        })
        .collect()
}
fn scalar(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
fn solve(rows: &[Row]) -> ReferenceResult<Vec<f64>> {
    let mut x = vec![0.; rows.len()];
    let mut r: Vec<_> = rows.iter().map(|row| row.rhs).collect();
    let tolerance = 1e-10 * r.iter().map(|v| v.abs()).fold(1_f64, f64::max);
    let mut z: Vec<_> = r
        .iter()
        .zip(rows)
        .map(|(v, row)| v / row.diagonal)
        .collect();
    let mut p = z.clone();
    let mut rz = scalar(&r, &z);
    for _ in 0..4000 {
        if r.iter().all(|v| v.is_finite() && v.abs() <= tolerance) {
            let ax = product(rows, &x);
            if rows
                .iter()
                .zip(ax)
                .all(|(row, value)| (row.rhs - value).abs() <= tolerance * 2.)
            {
                return Ok(x);
            }
        }
        let ap = product(rows, &p);
        let denominator = scalar(&p, &ap);
        if denominator <= 0. || !denominator.is_finite() || !rz.is_finite() {
            return Err("boundary tone oracle breakdown".into());
        }
        let alpha = rz / denominator;
        for i in 0..x.len() {
            x[i] += alpha * p[i];
            r[i] -= alpha * ap[i];
            z[i] = r[i] / rows[i].diagonal;
        }
        let next = scalar(&r, &z);
        let beta = next / rz;
        for i in 0..p.len() {
            p[i] = z[i] + beta * p[i];
        }
        rz = next;
    }
    Err("boundary tone oracle did not converge".into())
}
struct ToneField {
    width: usize,
    tones: Vec<tone_anchor::Result>,
}
impl BrightnessField for ToneField {
    fn apply(
        &self,
        row: usize,
        col: usize,
        out: &mut tone_anchor::Result,
        m: [f64; 3],
        n: [f64; 3],
    ) {
        let t = &self.tones[row * self.width + col];
        *out = result(m, n, t.amplitude, t.strength, t.available);
    }
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    width: usize,
    height: usize,
    pixels: &[[f64; 3]],
    original: &[[u8; 3]],
    field_masks: &[[u8; 3]],
    influence: &[f64],
    repaired: &[bool],
    n: [f64; 3],
    y: [f64; 3],
    matrix: [f64; 9],
) -> ReferenceResult<Vec<[f64; 3]>> {
    let ny = dot(n, y);
    let mut tones: Vec<_> = pixels
        .iter()
        .zip(original)
        .zip(field_masks)
        .map(|((&m, &q), &f)| tone(m, q, f, n, y))
        .collect();
    let targets: Vec<_> = pixels
        .iter()
        .zip(&tones)
        .map(|(&m, t)| {
            let v = if t.available && t.strength > 0. {
                t.amplitude * ny
            } else {
                dot(m, y)
            };
            if v > 0. && v.is_finite() {
                Some(v.ln())
            } else {
                None
            }
        })
        .collect();
    let positions: Vec<_> = (0..pixels.len())
        .filter(|&i| original[i] != [255; 3] && tones[i].available && tones[i].strength > 0.)
        .collect();
    // The production operation bypasses recovery when no original tone unknowns exist.
    if positions.is_empty() {
        return Ok(pixels.to_vec());
    }
    let mut indices = vec![usize::MAX; pixels.len()];
    for (index, &p) in positions.iter().enumerate() {
        indices[p] = index;
    }
    let mut rows = Vec::with_capacity(positions.len());
    for &p in &positions {
        let mut row = Row {
            diagonal: 1. / 64.,
            rhs: 0.,
            neighbors: Vec::new(),
        };
        for (dx, dy) in [(0, -1), (-1, 0), (0, 1), (1, 0)] {
            let xx = (p % width) as isize + dx;
            let yy = (p / width) as isize + dy;
            if xx < 0 || yy < 0 || xx >= width as isize || yy >= height as isize {
                continue;
            }
            let j = yy as usize * width + xx as usize;
            let Some(target) = targets[j] else { continue };
            let mut w = 0.;
            let mut gradient = 0.;
            for c in 0..3 {
                let a = pixels[p][c];
                let b = pixels[j][c];
                if a > 0. && b > 0. && a.is_finite() && b.is_finite() {
                    let q = (f64::from(original[p][c].min(original[j][c])) / 255.).powi(2);
                    w += q;
                    gradient += q * (b.ln() - a.ln());
                }
            }
            row.diagonal += w;
            row.rhs += w * (target - targets[p].unwrap()) - gradient;
            if indices[j] != usize::MAX {
                row.neighbors.push((indices[j], w));
            }
        }
        rows.push(row);
    }
    let correction = solve(&rows)?;
    for (i, &p) in positions.iter().enumerate() {
        let t = &tones[p];
        tones[p] = result(
            pixels[p],
            n,
            (targets[p].unwrap() + correction[i]).exp() / ny,
            t.strength,
            t.available,
        );
    }
    let model = LocalRecovery {
        cols: width,
        masks: field_masks.to_vec(),
        repaired: repaired.to_vec(),
    };
    let gradient = ToneField { width, tones };
    let ctx = DngCtx {
        rows: height as i32,
        cols: width as i32,
        prior: n.as_ptr(),
        conv_matrix: matrix.as_ptr() as *mut _,
        camera_y: Some(y),
        gradient: Some(&gradient),
        pixels,
    };
    let field = unsafe { color_field::Field::build(&ctx, &model, &[], 0, Control::none()) }?;
    let mut output = Vec::with_capacity(pixels.len());
    for p in 0..pixels.len() {
        let mask = if original[p] == [255; 3] {
            [quantize(1. - influence[p]); 3]
        } else {
            original[p]
        };
        let mut t = tone(pixels[p], original[p], mask, n, y);
        if original[p] != [255; 3] {
            let solved = &gradient.tones[p];
            t = result(pixels[p], n, solved.amplitude, t.strength, solved.available);
        }
        if let Some(direction) = field.estimate(
            p / width,
            p % width,
            pixels[p],
            mask,
            repaired[p],
            t.amplitude * ny,
            y,
        ) {
            output.push(highlight_color::apply(pixels[p], &t, n, y, direction));
        } else {
            output.push(t.samples);
        }
    }
    Ok(output)
}

#[test]
fn unchanged_inputs_match_the_pinned_cpu_equations() {
    let n = [0.284206, 0.604436, 1.];
    let y = [-2.6575486, 3.3135306, -0.2476];
    let matrix = [
        4.45774, -1.90728, 0.581922, -4.22818, 4.30012, -0.701463, 3.1376, -5.81127, 3.31683,
    ];
    for (pixel, mask) in [([0.27, 0.60, 0.94], [255; 3]), ([1.; 3], [0; 3])] {
        let pixels = vec![pixel; 16];
        let masks = vec![mask; 16];
        let repaired = vec![false; 16];
        let influence = vec![0.; 16];
        let pinned = super::run_pinned(4, 4, &pixels, &masks, &repaired, n, y, matrix).unwrap();
        let actual = run(
            4, 4, &pixels, &masks, &masks, &influence, &repaired, n, y, matrix,
        )
        .unwrap();
        for (a, b) in actual.iter().flatten().zip(pinned.iter().flatten()) {
            assert!((a - b).abs() < 1e-12);
        }
    }
}
