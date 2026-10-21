//! Converts compact histograms into tone controls and constrains estimated illuminants.
//! Thresholds, rounding, and polygon coordinates follow the reference implementation.
use super::{inverse, multiply, transform, Prepared};
use crate::nodes::finish::signed_response::{self, Controls};

pub(super) fn tone(histogram: &[i32], luma: &[i32], maxima: [f64; 3]) -> Vec<i32> {
    let root = signed_response::root();
    let exponential = signed_response::exponential();
    let criteria = [0.005, 0.02, 0.1, 0.5, 0.9, 0.98, 0.995];
    let targets = [60., 75., 95., 150., 205., 235., 255.];
    let comp = [0., 16., 16., 8., 4., 2., 1.];
    let contrast = [0., -1., -1., 0., 1., 1., 0.];
    let shadows = [6., -1., -1., 0., 1., 1., 0.];
    let highlights = [0., 1., 1., 0., -1., -1., 6.];
    let total = luma.iter().map(|x| f64::from(*x)).sum::<f64>();
    let mut quantiles = [0_usize; 7];
    let mut sum = 0.;
    let mut cursor = 0;
    for (i, value) in luma.iter().enumerate() {
        sum += f64::from(*value);
        while cursor < 7 && sum / total >= criteria[cursor] {
            quantiles[cursor] = i;
            cursor += 1;
        }
    }
    let source = quantiles.map(|q| f64::from(exponential[(q * 16) & 65535]));
    let target = targets.map(|q| f64::from(exponential[(q * 16.) as usize & 65535]));
    let dot = |a: [f64; 7], b: [f64; 7]| a.into_iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
    let ratio = dot(comp, target) / dot(comp, source);
    let ratio = if (0.0..=7.245789314111254).contains(&ratio) {
        ratio
    } else {
        7.245789314111254
    };
    let gain = ratio.powf(0.7);
    let difference = std::array::from_fn(|i| {
        targets[i] - f64::from(root[((source[i] * gain + 0.5) as usize) & 65535]) / 16.
    });
    let rounded = |x: f64| (x.clamp(-1.5, 1.5) * 10. + 0.5).floor() / 10.;
    let c = Controls {
        exposure: rounded(gain.log2()),
        contrast: rounded(dot(contrast, difference) * 0.005),
        shadows: rounded(dot(shadows, difference) * 0.004),
        highlights: rounded(dot(highlights, difference) * 0.003),
        ..Default::default()
    };
    // Preserve the reference axis order and its unmodified final planes.
    let mut hist = histogram.to_vec();
    for stride in [1, 32, 1024] {
        for r in 0..31 {
            for g in 0..31 {
                for b in 0..31 {
                    let i = r + g * 32 + b * 1024;
                    hist[i] += hist[i + stride];
                }
            }
        }
    }
    let mut weight = 0.;
    let mut sums = [0.; 3];
    for r in 0..32 {
        for g in 0..32 {
            for b in 0..32 {
                let n = hist[r + g * 32 + b * 1024];
                if n == 0 {
                    continue;
                }
                let w = (f64::from(n) + 1.).powf(0.125) - 1.;
                weight += w;
                for (i, v) in [r + 1, g + 1, b + 1].into_iter().enumerate() {
                    sums[i] += w * v as f64;
                }
            }
        }
    }
    let averages = std::array::from_fn::<_, 3, _>(|c| {
        (sums[c] / weight / 32. * maxima[c].max(1.) + 100.) * [0.96, 1., 1.04][c]
    });
    let minimum = averages.into_iter().fold(f64::INFINITY, f64::min);
    let gains = averages.map(|v| (v / minimum).powf(-0.5));
    let maximum = gains.into_iter().fold(0_f64, f64::max);
    let gains = gains.map(|v| 10_f64.powf(-((v / maximum).log10() * -100. + 0.3).trunc() / 100.));
    signed_response::prepare_tone_table(c, gains)
}

// Closed polygons use (R/G, B/G) coordinates multiplied by 10,000.
const LOW: &[[i32; 2]] = &[
    [7759, 8388],
    [8545, 4847],
    [8899, 3834],
    [9557, 3276],
    [13000, 3200],
    [14700, 5000],
    [13700, 11500],
    [9952, 16300],
    [8626, 16000],
    [7766, 13209],
    [7662, 8899],
    [7759, 8388],
];
const MID: &[[i32; 2]] = &[
    [9500, 9700],
    [10200, 7800],
    [10500, 7700],
    [11200, 8000],
    [11500, 9200],
    [11200, 12500],
    [10500, 14000],
    [9500, 14100],
    [9000, 12500],
    [9500, 9700],
];
const HIGH: &[[i32; 2]] = &[
    [9700, 9700],
    [10200, 8200],
    [10500, 8100],
    [10700, 8200],
    [10800, 9600],
    [10600, 11000],
    [10100, 11400],
    [9500, 11300],
    [9400, 10700],
    [9700, 9700],
];
const FLASH: &[[i32; 2]] = &[
    [9600, 10200],
    [9900, 10000],
    [10100, 10000],
    [10400, 10300],
    [10500, 11000],
    [10500, 12500],
    [10100, 14100],
    [9850, 14300],
    [9100, 14200],
    [9000, 13700],
    [9000, 12700],
    [9600, 10200],
];
fn inside(p: [i32; 2], polygon: &[[i32; 2]]) -> bool {
    let [x, y] = p.map(f64::from);
    let mut parity = false;
    for edge in polygon.windows(2) {
        let [ax, ay] = edge[0].map(f64::from);
        let [bx, by] = edge[1].map(f64::from);
        if (bx - ax) * (y - ay) == (by - ay) * (x - ax)
            && x >= ax.min(bx)
            && x <= ax.max(bx)
            && y >= ay.min(by)
            && y <= ay.max(by)
        {
            return true;
        }
        if (ay > y) != (by > y) && x < (bx - ax) * (y - ay) / (by - ay) + ax {
            parity = !parity;
        }
    }
    parity
}
fn project(p: [i32; 2], polygon: &[[i32; 2]], force: bool) -> [i32; 2] {
    if !force && inside(p, polygon) {
        return p;
    }
    let mut result = p;
    let mut minimum = f64::INFINITY;
    for edge in polygon.windows(2) {
        let [ax, ay] = edge[0];
        let [bx, by] = edge[1];
        let dx = f64::from(bx - ax);
        let dy = f64::from(by - ay);
        let dot = dx * f64::from(p[0] - ax) + dy * f64::from(p[1] - ay);
        let norm = dx * dx + dy * dy;
        let q = if dot <= 0. {
            [ax, ay]
        } else if dot >= norm {
            [bx, by]
        } else {
            [
                ax + (dot / norm * dx + 0.5) as i32,
                ay + (dot / norm * dy + 0.5) as i32,
            ]
        };
        let d = f64::from(p[0] - q[0]).powi(2) + f64::from(p[1] - q[1]).powi(2);
        if d < minimum {
            minimum = d;
            result = q;
        }
    }
    result
}
pub(super) fn constrain(p: &Prepared, candidate: [f64; 3]) -> [f64; 3] {
    let basis = [
        1.4032, -0.2231, -0.1016, -0.5263, 1.4816, 0.017, -0.0112, 0.0183, 0.9113,
    ];
    let matrix = multiply(basis, p.camera).map(|x| x * p.scale);
    let converted = transform(matrix, candidate);
    let target = if !p.constrained {
        let floored = converted.map(|v| v.max(0.1));
        let sum = floored.iter().sum::<f64>();
        let v = std::array::from_fn::<_, 3, _>(|i| (floored[i] * 3. / sum).max([0.6, 0.9, 0.5][i]));
        v.map(|x| x / v[1])
    } else {
        let point = [
            (converted[0] / converted[1] * 10000.) as i32,
            (converted[2] / converted[1] * 10000.) as i32,
        ];
        let ev = p.brightness;
        let adjusted = if p.flash {
            project(point, FLASH, false)
        } else if ev <= 4.9 {
            project(point, LOW, false)
        } else if ev <= 5.7 || (5.8 < ev && ev <= 8.3) {
            let (lower, upper, amount) = if ev <= 5.7 {
                (LOW, MID, (ev - 4.9) / 0.7999999999999998)
            } else {
                (MID, HIGH, (ev - 5.8) / 2.500000000000001)
            };
            if inside(point, upper) {
                point
            } else {
                let first = project(point, lower, true);
                let second = project(point, upper, true);
                let q = project(point, lower, false);
                let d = [second[0] - first[0], second[1] - first[1]];
                let norm = f64::from(d[0]).powi(2) + f64::from(d[1]).powi(2);
                let quotient = if norm != 0. {
                    ((f64::from(q[0] - first[0]) * f64::from(d[0])
                        + f64::from(q[1] - first[1]) * f64::from(d[1]))
                        / norm)
                        .trunc()
                } else {
                    0.
                };
                if quotient < amount {
                    std::array::from_fn(|i| {
                        (amount * f64::from(second[i]) + 0.5) as i32
                            + ((1. - amount) * f64::from(first[i]) + 0.5) as i32
                    })
                } else {
                    point
                }
            }
        } else {
            project(point, if ev <= 5.8 { MID } else { HIGH }, false)
        };
        [
            f64::from(adjusted[0]) / 10000.,
            1.,
            f64::from(adjusted[1]) / 10000.,
        ]
    };
    normalize(transform(inverse(matrix), target))
}
pub(super) fn normalize(v: [f64; 3]) -> [f64; 3] {
    let maximum = v.into_iter().fold(f64::NEG_INFINITY, f64::max);
    v.map(|x| x / maximum)
}
pub(super) fn finish(p: &Prepared, v: [f64; 3]) -> [f64; 3] {
    let v = constrain(p, v);
    if p.brightness <= 3.2 {
        return v;
    }
    let matrix = multiply(
        [
            1.4032, -0.2231, -0.1016, -0.5263, 1.4816, 0.017, -0.0112, 0.0183, 0.9113,
        ],
        p.camera,
    )
    .map(|v| v * p.scale);
    let mut working = transform(matrix, v);
    let t = ((p.brightness - 3.2) / 2.5).min(1.) as f32;
    working[0] *= f64::from(t.mul_add(-0.02, 1.));
    working[2] *= f64::from(t.mul_add(0.08, 1.));
    normalize(transform(inverse(matrix), working))
}
