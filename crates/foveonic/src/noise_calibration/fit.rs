//! Fits small native-layer transfer systems and ISO covariance priors from GPU-reduced cap profiles.

use super::{Condition, Result};
use crate::nodes::sensor::noise_model::{
    Model, RowPrior, COLUMN_REFERENCE_LENGTH as COLUMN_LENGTH, GAUSSIAN_SIGMAS, MODEL_COUNT,
    REFERENCE_OFFSET, ROW_PRIOR_SIGMA, SCALES, WEIGHTS_PER_MODEL,
};
use std::collections::BTreeMap;

pub(super) type Vector = [f64; 3];
const RIDGE: f64 = 0.1;
const COVARIANCE_RIDGE: f64 = 1e-3;
const NUMERICAL_FLOOR: f64 = 1e-12;
const PEDESTAL_THRESHOLD: f64 = 24.;

type Bands = [Vec<Vector>; SCALES];
#[derive(Default, Clone)]
struct System {
    gram: [[f64; 3]; 3],
    rhs: Vector,
}
impl System {
    fn observe(&mut self, x: Vector, y: f64, weight: f64) {
        for i in 0..3 {
            self.rhs[i] += x[i] * y * weight;
            for j in 0..3 {
                self.gram[i][j] += x[i] * x[j] * weight;
            }
        }
    }
    fn solve(&self) -> Vector {
        let scale = std::array::from_fn::<_, 3, _>(|i| self.gram[i][i].sqrt().max(NUMERICAL_FLOOR));
        let matrix: [f64; 9] = std::array::from_fn(|i| {
            self.gram[i / 3][i % 3] / scale[i / 3] / scale[i % 3]
                + if i / 3 == i % 3 { RIDGE } else { 0. }
        });
        let inverse = crate::color_space::inverse(matrix);
        std::array::from_fn(|i| {
            (0..3)
                .map(|j| inverse[i * 3 + j] * self.rhs[j] / scale[j])
                .sum::<f64>()
                / scale[i]
        })
    }
}

pub(super) struct FrameProfiles {
    pub active_rows: Vec<Vector>,
    pub active_columns: Vec<Vector>,
    pub right: Vec<Vector>,
    pub bottom: Vec<Vector>,
    pub black: Vector,
}

pub(super) struct Fitter {
    axis: Vector,
    rows: [System; SCALES],
    full_rows: [[System; SCALES]; 2],
    columns: [[System; SCALES]; 2],
    references: [Option<Bands>; 2],
    reference_counts: [usize; 2],
    covariances: BTreeMap<u32, ([[f64; 3]; 3], usize)>,
}
impl Fitter {
    pub fn new(model: &Model) -> Self {
        Self {
            axis: model.shield_color_axis().map(f64::from),
            rows: Default::default(),
            full_rows: Default::default(),
            columns: Default::default(),
            references: Default::default(),
            reference_counts: [0; 2],
            covariances: BTreeMap::new(),
        }
    }
    pub fn observe(&mut self, condition: &Condition, frames: &[FrameProfiles]) -> Result<()> {
        let mut black: Vec<_> = frames.iter().flat_map(|f| f.black).collect();
        black.sort_by(f64::total_cmp);
        let state = usize::from(black[black.len() / 2] >= PEDESTAL_THRESHOLD);
        let right: Vec<_> = frames.iter().map(|f| bands(&f.right)).collect();
        let rows: Vec<_> = frames.iter().map(|f| bands(&f.active_rows)).collect();
        let bottom: Vec<_> = frames.iter().map(|f| bands(&f.bottom)).collect();
        let columns: Vec<_> = frames.iter().map(|f| bands(&f.active_columns)).collect();
        if bottom[0][0].len() != COLUMN_LENGTH {
            return Err("unsupported calibration column geometry".into());
        }
        // Columns learn changes between captures. Their stable component is
        // retained separately so a fixed shield bias is not subtracted twice.
        let mean_bottom = average(&bottom);
        let mean_columns = average(&columns);
        for band in 0..SCALES {
            // Equal capture-condition weights keep a larger buffer or one
            // frequently sampled camera from dominating the shared model.
            let row_weight = 1. / (frames.len() * right[0][band].len()) as f64;
            let col_weight = 1. / (frames.len() * bottom[0][band].len()) as f64;
            for frame in 0..frames.len() {
                for (x, y) in right[frame][band].iter().zip(&rows[frame][band]) {
                    let y = dot(*y, self.axis);
                    self.rows[band].observe(*x, y, row_weight);
                    self.full_rows[state][band].observe(*x, y, row_weight);
                }
                for position in 0..COLUMN_LENGTH {
                    let x = std::array::from_fn(|c| {
                        bottom[frame][band][position][c] - mean_bottom[band][position][c]
                    });
                    let y = std::array::from_fn(|c| {
                        columns[frame][band][position][c] - mean_columns[band][position][c]
                    });
                    self.columns[state][band].observe(x, dot(y, self.axis), col_weight);
                }
            }
        }
        let reference = self.references[state]
            .get_or_insert_with(|| std::array::from_fn(|_| vec![[0.; 3]; COLUMN_LENGTH]));
        for b in 0..SCALES {
            for i in 0..COLUMN_LENGTH {
                for c in 0..3 {
                    reference[b][i][c] += mean_bottom[b][i][c];
                }
            }
        }
        self.reference_counts[state] += 1;
        let covariance = self.covariances.entry(condition.iso).or_default();
        let weight = 1. / (frames.len() * frames[0].active_rows.len()) as f64;
        for frame in frames {
            let smooth = blur(&frame.active_rows, ROW_PRIOR_SIGMA);
            for (value, low) in frame.active_rows.iter().zip(smooth) {
                let high = std::array::from_fn::<_, 3, _>(|c| value[c] - low[c]);
                for i in 0..3 {
                    for j in 0..3 {
                        covariance.0[i][j] += high[i] * high[j] * weight;
                    }
                }
            }
        }
        covariance.1 += 1;
        Ok(())
    }
    pub fn finish(self, mut model: Model) -> Result<Model> {
        let mut coefficients = [[[0.; 3]; SCALES]; MODEL_COUNT];
        coefficients[0] = self.rows.each_ref().map(System::solve);
        coefficients[0][SCALES - 1] = [0.; 3];
        write_coefficients(&mut model.shield_tables, 0, coefficients[0]);
        for state in 0..2 {
            if self.reference_counts[state] == 0 {
                continue;
            }
            coefficients[1 + state] = self.full_rows[state].each_ref().map(System::solve);
            write_coefficients(&mut model.shield_tables, 1 + state, coefficients[1 + state]);
            let mut column = self.columns[state].each_ref().map(System::solve);
            column[0] = [0.; 3];
            column[1] = [0.; 3];
            write_coefficients(&mut model.shield_tables, 3 + state, column);
            let reference = self.references[state].as_ref().unwrap();
            let mut curve = vec![0.; COLUMN_LENGTH];
            for (i, value) in curve.iter_mut().enumerate() {
                *value = (0..SCALES)
                    .map(|b| dot(reference[b][i], column[b]))
                    .sum::<f64>()
                    / self.reference_counts[state] as f64;
            }
            let mean = curve.iter().sum::<f64>() / COLUMN_LENGTH as f64;
            for (i, value) in curve.into_iter().enumerate() {
                model.shield_tables[REFERENCE_OFFSET + state * COLUMN_LENGTH + i] =
                    (value - mean) as f32;
            }
        }
        model.row_priors = self
            .covariances
            .into_iter()
            .map(|(iso, (cov, n))| {
                let mut covariance: [f64; 9] =
                    std::array::from_fn(|i| cov[i / 3][i % 3] / n as f64);
                let trace = (covariance[0] + covariance[4] + covariance[8]).max(NUMERICAL_FLOOR);
                for i in 0..3 {
                    covariance[i * 3 + i] += trace * COVARIANCE_RIDGE;
                }
                RowPrior {
                    iso,
                    inverse_covariance: crate::color_space::inverse(covariance).map(|v| v as f32),
                }
            })
            .collect();
        model.validate()?;
        Ok(model)
    }
}
fn write_coefficients(tables: &mut [f32], index: usize, bands: [Vector; SCALES]) {
    for b in 0..SCALES {
        for c in 0..3 {
            tables[index * WEIGHTS_PER_MODEL + b * 3 + c] =
                (bands[b][c] - if b == 0 { 0. } else { bands[b - 1][c] }) as f32;
        }
    }
}
fn dot(a: Vector, b: Vector) -> f64 {
    (0..3).map(|c| a[c] * b[c]).sum()
}
fn reflect(value: isize, n: usize) -> usize {
    let p = 2 * (n as isize - 1);
    let i = value.rem_euclid(p);
    i.min(p - i) as usize
}
fn blur(values: &[Vector], sigma: f64) -> Vec<Vector> {
    let radius = (sigma * 4. + 0.5) as isize;
    let kernel: Vec<_> = (-radius..=radius)
        .map(|i| (-0.5 * (i as f64 / sigma).powi(2)).exp())
        .collect();
    let sum = kernel.iter().sum::<f64>();
    (0..values.len())
        .map(|i| {
            std::array::from_fn(|c| {
                kernel
                    .iter()
                    .enumerate()
                    .map(|(j, w)| {
                        values[reflect(i as isize + j as isize - radius, values.len())][c] * w / sum
                    })
                    .sum()
            })
        })
        .collect()
}
fn bands(values: &[Vector]) -> Bands {
    let smooth = [
        values.to_vec(),
        blur(values, GAUSSIAN_SIGMAS[0] as f64),
        blur(values, GAUSSIAN_SIGMAS[1] as f64),
        blur(values, GAUSSIAN_SIGMAS[2] as f64),
        blur(values, GAUSSIAN_SIGMAS[3] as f64),
    ];
    std::array::from_fn(|b| {
        let mut band: Vec<Vector> = (0..values.len())
            .map(|i| {
                std::array::from_fn(|c| {
                    smooth[b][i][c]
                        - if b + 1 < SCALES {
                            smooth[b + 1][i][c]
                        } else {
                            0.
                        }
                })
            })
            .collect();
        let mean: Vector =
            std::array::from_fn(|c| band.iter().map(|v| v[c]).sum::<f64>() / band.len() as f64);
        for value in &mut band {
            for c in 0..3 {
                value[c] -= mean[c];
            }
        }
        band
    })
}
fn average(frames: &[Bands]) -> Bands {
    std::array::from_fn(|b| {
        (0..frames[0][b].len())
            .map(|i| {
                std::array::from_fn(|c| {
                    frames.iter().map(|v| v[b][i][c]).sum::<f64>() / frames.len() as f64
                })
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_ridge_solve_and_gaussian_conversion_match_known_equations() {
        let mut system = System::default();
        system.observe([2., 0., 0.], 6., 1.);
        system.observe([0., 4., 0.], -8., 1.);
        system.observe([0., 0., 8.], 4., 1.);
        for (actual, expected) in system.solve().into_iter().zip([3., -2., 0.5]) {
            assert!((actual - expected / (1. + RIDGE)).abs() < 1e-12);
        }
        let coefficients = [
            [1., 2., 3.],
            [4., 5., 6.],
            [7., 8., 9.],
            [10., 11., 12.],
            [13., 14., 15.],
        ];
        let mut tables = [0f32; 15];
        write_coefficients(&mut tables, 0, coefficients);
        let gaussians = [
            [11., 9., 7.],
            [8., 6., 5.],
            [4., 3., 2.],
            [2., 1., 0.5],
            [1., 0.5, 0.25],
        ];
        let direct: f64 = (0..SCALES)
            .map(|b| {
                dot(
                    std::array::from_fn(|c| {
                        gaussians[b][c]
                            - if b + 1 < SCALES {
                                gaussians[b + 1][c]
                            } else {
                                0.
                            }
                    }),
                    coefficients[b],
                )
            })
            .sum();
        let converted: f64 = (0..SCALES)
            .map(|b| {
                dot(
                    gaussians[b],
                    std::array::from_fn(|c| tables[b * 3 + c] as f64),
                )
            })
            .sum();
        assert_eq!(direct, converted);
        let constant = vec![[1., 2., 3.]; 20];
        assert!(bands(&constant)
            .iter()
            .flatten()
            .flatten()
            .all(|v| v.abs() < 1e-12));
    }
}
