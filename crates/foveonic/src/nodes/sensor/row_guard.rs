//! Bounds row offsets with scale-aware image evidence, Gaussian supports and layer covariance.

use super::{calibration::Calibration, noise_model};
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    render::WorkingGrid,
    source::Result,
};
use std::sync::Arc;

const LAYERS: u64 = 3;
const FLOAT_BYTES: u64 = 4;
const SPANS: u32 = 12;
const SPAN_COMPONENTS: u64 = 9;
const ROW_COMPONENTS: u64 = 9;
const LINE_WORKGROUP: u32 = 64;
const IMAGE_WORKGROUP: u32 = 8;
const PASSES: u32 = 10;
const MINIMUM_SIDE: u32 = 65;

const GAUSSIANS: [(f64, usize); 4] = [
    (noise_model::ROW_PRIOR_SIGMA, 5),
    (3., 12),
    (8., 32),
    (4., 16),
];

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    source: [u32; 4],
    area: [u32; 4],
    noise: [[f32; 4]; 3],
    inverse_covariance: [[f32; 4]; 3],
    mean_pass: u32,
    reserved: [u32; 3],
    radii: [u32; 4],
    offsets: [u32; 4],
    sampling: [u32; 4],
    limits: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
    weights: Vec<f32>,
    working: Option<(WorkingGrid, Arc<Calibration>)>,
}

fn gaussian(sigma: f64, radius: usize) -> Vec<f32> {
    let kernel: Vec<f64> = (-(radius as isize)..=radius as isize)
        .map(|x| (-0.5 * (x as f64 / sigma).powi(2)).exp())
        .collect();
    let sum: f64 = kernel.iter().sum();
    kernel.into_iter().map(|v| (v / sum) as f32).collect()
}

fn highpass_energy(weights: &[f32]) -> f64 {
    weights
        .iter()
        .enumerate()
        .map(|(i, &w)| {
            let h = f64::from(w) - if i == weights.len() / 2 { 1. } else { 0. };
            h * h
        })
        .sum()
}

impl Prepared {
    /// Uses uncalibrated sensor codes and a half-open active rectangle.
    /// Noise terms are constant, linear and quadratic, each in layer order.
    pub(crate) fn new(
        extent: [u32; 2],
        area: [u32; 4],
        noise: [f32; 9],
        iso: u32,
        model: &noise_model::Model,
    ) -> Result<Self> {
        if area[0] >= area[2]
            || area[1] >= area[3]
            || area[2] > extent[0]
            || area[3] > extent[1]
            || area[2] - area[0] < MINIMUM_SIDE
            || area[3] - area[1] < MINIMUM_SIDE
            || noise.iter().any(|v| !v.is_finite())
            || iso == 0
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / LAYERS
        {
            return Err("invalid native row guard parameters".into());
        }
        let covariance = model.row_inverse_covariance(iso);
        let mut weights = Vec::new();
        let mut offsets = [0; 4];
        for (i, (sigma, radius)) in GAUSSIANS.into_iter().enumerate() {
            offsets[i] = weights.len() as u32;
            weights.extend(gaussian(sigma, radius));
        }
        Ok(Self {
            params: Params {
                source: [extent[0], extent[1], area[2] - area[0], area[3] - area[1]],
                area,
                noise: std::array::from_fn(|i| {
                    [noise[i * 3], noise[i * 3 + 1], noise[i * 3 + 2], 0.]
                }),
                inverse_covariance: std::array::from_fn(|i| {
                    [
                        covariance[i * 3],
                        covariance[i * 3 + 1],
                        covariance[i * 3 + 2],
                        0.,
                    ]
                }),
                mean_pass: 0,
                reserved: [0; 3],
                radii: GAUSSIANS.map(|(_, radius)| radius as u32),
                offsets,
                sampling: [1, SPANS, 0, 0],
                limits: [1e-5, 1e-6, 0.05, 1.],
            },
            weights,
            working: None,
        })
    }

    pub(crate) fn on_grid(mut self, grid: WorkingGrid, calibration: Arc<Calibration>) -> Self {
        let d = grid.scale.divisor();
        let native_energy = highpass_energy(&self.weights[..2 * GAUSSIANS[0].1 + 1]);
        self.params.source[..2].copy_from_slice(&grid.extent);
        self.params.area = grid.bounds(self.params.area);
        self.params.source[2] = self.params.area[2] - self.params.area[0];
        self.params.source[3] = self.params.area[3] - self.params.area[1];
        self.params.sampling = [
            d,
            SPANS.min(self.params.source[2]),
            grid.origin[0],
            grid.origin[1],
        ];
        let pixels = (u64::from(d) * u64::from(d)) as f32;
        for row in &mut self.params.noise {
            for coefficient in &mut row[..3] {
                *coefficient /= pixels;
            }
        }
        self.params.limits = [1e-5 / pixels, 1e-6 / pixels, 0.05 / d as f32, pixels];
        self.weights.clear();
        for (i, (sigma, radius)) in GAUSSIANS.into_iter().enumerate() {
            let radius = (radius as u32).div_ceil(d).max(1);
            self.params.radii[i] = radius;
            self.params.offsets[i] = self.weights.len() as u32;
            self.weights
                .extend(gaussian((sigma / f64::from(d)).max(0.5), radius as usize));
        }
        // Training measures high-pass row offsets. Horizontal averaging does
        // not reduce a coherent row. Vertical averaging reduces its variance
        // by d, with the discrete high-pass response converted as well.
        let energy = highpass_energy(&self.weights[..2 * self.params.radii[0] as usize + 1]);
        let inverse_factor = f64::from(d) * native_energy / energy;
        for row in &mut self.params.inverse_covariance {
            for value in &mut row[..3] {
                *value = (f64::from(*value) * inverse_factor) as f32;
            }
        }
        self.working = Some((grid, calibration));
        self
    }

    pub(crate) fn grid(&self) -> Option<WorkingGrid> {
        self.working.as_ref().map(|(grid, _)| *grid)
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.source[0], self.params.source[1]]
    }

    pub(crate) fn key(&self) -> Key {
        let key = Key::pod(&self.params).slice(&self.weights);
        match &self.working {
            Some((_, calibration)) => key
                .with(&calibration.params)
                .slice(&calibration.grid_values),
            None => key,
        }
    }
}

fn workspace(gpu: &Gpu, label: &str, floats: u64) -> Result<ReadGuard> {
    gpu.storage(label, floats * FLOAT_BYTES)
}

pub(crate) enum Inputs<'a> {
    Native {
        original: &'a ReadGuard,
        shield_corrected: &'a ReadGuard,
        black: &'a ReadGuard,
    },
    Calibrated {
        input: &'a ReadGuard,
        original: &'a ReadGuard,
        shield_corrected: &'a ReadGuard,
        black: &'a ReadGuard,
        clipping: &'a ReadGuard,
    },
}

/// Measurement always uses paired original/post-shield codes. Later calibrated
/// repairs remain on the numerical input and cannot become row evidence.
pub(crate) fn apply(
    gpu: &Gpu,
    inputs: Inputs<'_>,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height] = prepared.extent();
    let bytes = u64::from(width) * u64::from(height) * LAYERS * FLOAT_BYTES;
    let (original, shield_corrected, black) = match &inputs {
        Inputs::Native {
            original,
            shield_corrected,
            black,
        } => (*original, *shield_corrected, *black),
        Inputs::Calibrated {
            original,
            shield_corrected,
            black,
            input,
            clipping,
        } => {
            if input.size() != bytes
                || clipping.size() != bytes / LAYERS
                || prepared.working.is_none()
            {
                return Err("guarded row calibrated input geometry mismatch".into());
            }
            (*original, *shield_corrected, *black)
        }
    };
    if original.size() != bytes
        || shield_corrected.size() != bytes
        || black.size() != LAYERS * FLOAT_BYTES
    {
        return Err("row guard evidence geometry mismatch".into());
    }
    let rows = u64::from(prepared.params.source[3]);
    let mut spans = workspace(
        gpu,
        "Independent row spans",
        rows * u64::from(prepared.params.sampling[1]) * SPAN_COMPONENTS,
    )?;
    let mut stats = workspace(gpu, "Robust row statistics", rows * ROW_COMPONENTS)?;
    let mut field = workspace(gpu, "Row reliability field", rows * LAYERS)?;
    let mut highpass = workspace(gpu, "Row high-pass field", rows * LAYERS)?;
    let mut guarded = workspace(gpu, "Guarded row field", rows * LAYERS)?;
    let mut means = workspace(gpu, "Row field means", 2 * LAYERS)?;
    let mut output = gpu.storage("Guarded sensor layers", bytes)?;
    let weights = gpu.upload("Row Gaussian weights", &prepared.weights)?;
    let uniform = gpu.uniform(&prepared.params);
    let mut trace = gpu.trace(profile.mode.gpu(), PASSES);
    let clipping = match &inputs {
        Inputs::Calibrated { clipping, .. } => Some(*clipping),
        _ => None,
    };
    let line_groups = [prepared.params.source[3].div_ceil(LINE_WORKGROUP), 1];
    let evidence_floats = rows * u64::from(prepared.params.source[2]) * LAYERS;
    let mut residual = workspace(gpu, "Row evidence and signal workspace", evidence_floats)?;
    let mut variance = workspace(gpu, "Row pixel variances", evidence_floats)?;
    let mut evidence_bindings = vec![
        (0, Read(&uniform)),
        (1, Read(original)),
        (3, Read(black)),
        (4, Read(&weights)),
        (12, Write(residual.write())),
        (13, Write(variance.write())),
    ];
    if let Some(clipping) = clipping {
        evidence_bindings.push((17, Read(clipping)));
    }
    gpu.dispatch(
        if clipping.is_some() {
            Kernel::RowGuardEvidenceReduced
        } else {
            Kernel::RowGuardEvidence
        },
        &mut evidence_bindings,
        [
            prepared.params.source[2].div_ceil(LINE_WORKGROUP),
            prepared.params.source[3],
        ],
        &mut trace,
    );
    drop(evidence_bindings);
    gpu.dispatch(
        Kernel::RowGuardProfiles,
        &mut [
            (0, Read(&uniform)),
            (1, Read(original)),
            (2, Read(shield_corrected)),
            (3, Read(black)),
            (5, Write(spans.write())),
            (12, Read(&residual)),
            (13, Read(&variance)),
        ],
        [line_groups[0], prepared.params.sampling[1]],
        &mut trace,
    );
    drop(variance);
    // The span reduction has submitted its last residual read. The later
    // signal pass overwrites the same-sized workspace under its write guard.
    let mut guide = residual;
    gpu.dispatch(
        Kernel::RowGuardRobust,
        &mut [
            (0, Read(&uniform)),
            (5, Read(&spans)),
            (6, Write(stats.write())),
        ],
        line_groups,
        &mut trace,
    );
    gpu.dispatch(
        Kernel::RowGuardReliability,
        &mut [
            (0, Read(&uniform)),
            (4, Read(&weights)),
            (6, Read(&stats)),
            (7, Write(field.write())),
        ],
        line_groups,
        &mut trace,
    );
    gpu.dispatch(
        Kernel::RowGuardHighpass,
        &mut [
            (0, Read(&uniform)),
            (4, Read(&weights)),
            (7, Read(&field)),
            (8, Write(highpass.write())),
        ],
        line_groups,
        &mut trace,
    );
    gpu.dispatch(
        Kernel::RowGuardMean,
        &mut [
            (0, Read(&uniform)),
            (7, Read(&highpass)),
            (9, Write(means.write())),
        ],
        [LAYERS as u32, 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::RowGuardConfidence,
        &mut [
            (0, Read(&uniform)),
            (4, Read(&weights)),
            (5, Read(&spans)),
            (6, Read(&stats)),
            (7, Read(&highpass)),
            (8, Write(guarded.write())),
            (9, Read(&means)),
        ],
        line_groups,
        &mut trace,
    );
    let mut final_params = prepared.params;
    final_params.mean_pass = 1;
    let final_uniform = gpu.uniform(&final_params);
    gpu.dispatch(
        Kernel::RowGuardMean,
        &mut [
            (0, Read(&final_uniform)),
            (7, Read(&guarded)),
            (9, Write(means.write())),
        ],
        [LAYERS as u32, 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::RowGuardHorizontal,
        &mut [
            (0, Read(&uniform)),
            (1, Read(original)),
            (3, Read(black)),
            (4, Read(&weights)),
            (10, Write(guide.write())),
        ],
        [
            prepared.params.source[2].div_ceil(IMAGE_WORKGROUP),
            prepared.params.source[3].div_ceil(IMAGE_WORKGROUP),
        ],
        &mut trace,
    );
    let calibration = prepared
        .working
        .as_ref()
        .map(|(_, c)| -> Result<_> {
            Ok((
                gpu.uniform(&c.params),
                gpu.upload("Row guard spatial calibration", &c.grid_values)?,
            ))
        })
        .transpose()?;
    let mut apply_bindings = vec![
        (0, Read(&uniform)),
        (4, Read(&weights)),
        (8, Read(&guarded)),
        (9, Read(&means)),
        (10, Read(&guide)),
        (11, Write(output.write())),
    ];
    let kernel = match &inputs {
        Inputs::Native { .. } => {
            apply_bindings.extend([(1, Read(original)), (2, Read(shield_corrected))]);
            Kernel::RowGuardApply
        }
        Inputs::Calibrated {
            input, clipping, ..
        } => {
            let (params, values) = calibration.as_ref().unwrap();
            apply_bindings.extend([
                (15, Read(params)),
                (16, Read(values)),
                (17, Read(clipping)),
                (18, Read(input)),
            ]);
            Kernel::RowGuardApplyCalibrated
        }
    };
    gpu.dispatch(
        kernel,
        &mut apply_bindings,
        [
            width.div_ceil(IMAGE_WORKGROUP),
            height.div_ceil(IMAGE_WORKGROUP),
        ],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "row_guard_test.rs"]
mod tests;
