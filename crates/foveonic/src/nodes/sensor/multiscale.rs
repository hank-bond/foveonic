//! Filters sensor-layer color differences with scale-aware supports and a local calibration basis.

use super::calibration::Calibration;
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

const PAD_MULTIPLE: u32 = 16;
const MINIMUM_ACTIVE_SIDE: u32 = 65;
const WORKGROUP_SIDE: u32 = 8;
const HORIZONTAL_WORKGROUP_WIDTH: u32 = 32;
const HORIZONTAL_WORKGROUP_HEIGHT: u32 = 4;
const MAXIMUM_TRACE_PASSES: u32 = 20;
const FLOAT_BYTES: u64 = std::mem::size_of::<f32>() as u64;
const LAYER_BYTES: u64 = std::mem::size_of::<[f32; 3]>() as u64;
const MOMENT_COMPONENTS: u32 = 4;
// Matches the update phase in the shader without changing its packed uniforms.
const UPDATE_PHASE: f32 = 1.;

#[derive(Clone, Copy)]
struct Stage {
    scale: u32,
    radius: u32,
    strength: f32,
}

const STAGES: [Stage; 3] = [
    Stage {
        scale: 16,
        radius: 4,
        strength: 4.,
    },
    Stage {
        scale: 4,
        radius: 4,
        strength: 4.,
    },
    Stage {
        scale: 1,
        radius: 3,
        strength: 1.,
    },
];
const VARIANCE_FLOOR: f32 = 1e-6;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    source: [u32; 4],
    area: [u32; 4],
    stage: [u32; 4],
    controls: [f32; 4],
    noise: [[f32; 4]; 3],
    neutral: [f32; 4],
    sampling: [u32; 4],
}

#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
    working: Option<(WorkingGrid, Arc<Calibration>)>,
}

impl Prepared {
    /// Inputs use uncalibrated sensor codes. The active rectangle is half-open.
    /// Noise coefficients are grouped by constant, linear and quadratic terms,
    /// each containing the three layers in their original storage order.
    pub(crate) fn new(
        extent: [u32; 2],
        area: [u32; 4],
        noise: [f32; 9],
        neutral: [f32; 3],
    ) -> Result<Self> {
        let sum = neutral.iter().sum::<f32>();
        if area[0] >= area[2]
            || area[1] >= area[3]
            || area[2] > extent[0]
            || area[3] > extent[1]
            || area[2] - area[0] < MINIMUM_ACTIVE_SIDE
            || area[3] - area[1] < MINIMUM_ACTIVE_SIDE
            || noise.iter().chain(&neutral).any(|v| !v.is_finite())
            || !sum.is_finite()
            || sum.abs() < VARIANCE_FLOOR
            || u64::from(extent[0]) * u64::from(extent[1])
                > u64::from(u32::MAX) / u64::from(MOMENT_COMPONENTS)
        {
            return Err("invalid multiscale sensor filter parameters".into());
        }
        let padded =
            [area[2] - area[0], area[3] - area[1]].map(|v| v.div_ceil(PAD_MULTIPLE) * PAD_MULTIPLE);
        Ok(Self {
            params: Params {
                source: [extent[0], extent[1], padded[0], padded[1]],
                area,
                stage: [0; 4],
                controls: [0., 0., VARIANCE_FLOOR, 0.],
                noise: std::array::from_fn(|i| {
                    [noise[i * 3], noise[i * 3 + 1], noise[i * 3 + 2], 0.]
                }),
                neutral: [neutral[0], neutral[1], neutral[2], sum],
                sampling: [1, 0, 0, 0],
            },
            working: None,
        })
    }

    /// Calibrated area means are normalized locally inside the filter. Native
    /// processing keeps its original code basis and operation order.
    pub(crate) fn on_grid(mut self, grid: WorkingGrid, calibration: Arc<Calibration>) -> Self {
        let d = grid.scale.divisor();
        self.params.source[..2].copy_from_slice(&grid.extent);
        self.params.area = grid.bounds(self.params.area);
        self.params.source[2] =
            (self.params.area[2] - self.params.area[0]).div_ceil(PAD_MULTIPLE) * PAD_MULTIPLE;
        self.params.source[3] =
            (self.params.area[3] - self.params.area[1]).div_ceil(PAD_MULTIPLE) * PAD_MULTIPLE;
        for row in &mut self.params.noise {
            for coefficient in &mut row[..3] {
                *coefficient /= (u64::from(d) * u64::from(d)) as f32;
            }
        }
        self.params.sampling = [d, grid.origin[0], grid.origin[1], 0];
        self.working = Some((grid, calibration));
        self
    }

    pub(crate) fn grid(&self) -> Option<WorkingGrid> {
        self.working.as_ref().map(|(grid, _)| *grid)
    }

    fn stages(&self) -> [Stage; 3] {
        let d = self.params.sampling[0];
        STAGES.map(|stage| {
            let scale = (stage.scale / d).max(1);
            Stage {
                scale,
                radius: (stage.radius * stage.scale)
                    .div_ceil(d.saturating_mul(scale))
                    .max(1),
                strength: stage.strength,
            }
        })
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.source[0], self.params.source[1]]
    }

    pub(crate) fn key(&self) -> Key {
        let key = Key::pod(&self.params);
        match &self.working {
            Some((_, calibration)) => key
                .with(&calibration.params)
                .slice(&calibration.grid_values),
            None => key,
        }
    }
}

fn workspace(gpu: &Gpu, extent: [u32; 2], components: u32) -> Result<ReadGuard> {
    gpu.storage(
        "Multiscale workspace",
        u64::from(extent[0]) * u64::from(extent[1]) * u64::from(components) * FLOAT_BYTES,
    )
}

pub(crate) enum Evidence<'a> {
    Native {
        original: &'a ReadGuard,
        black: &'a ReadGuard,
    },
    Calibrated {
        clipping: &'a ReadGuard,
    },
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    evidence: Evidence<'_>,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height] = prepared.extent();
    let bytes = u64::from(width) * u64::from(height) * LAYER_BYTES;
    let valid = match &evidence {
        Evidence::Native { original, black } => {
            prepared.working.is_none() && original.size() == bytes && black.size() == LAYER_BYTES
        }
        Evidence::Calibrated { clipping } => {
            prepared.working.is_some() && clipping.size() == bytes / 3
        }
    };
    if input.size() != bytes || !valid {
        return Err("multiscale sensor filter input geometry mismatch".into());
    }
    let full = [prepared.params.source[2], prepared.params.source[3]];
    let mut current = workspace(gpu, full, 3)?;
    let mut spare: Option<ReadGuard> = None;
    let mut params = prepared.params;
    let uniform = gpu.uniform(&params);
    let mut trace = gpu.trace(profile.mode.gpu(), MAXIMUM_TRACE_PASSES);
    let calibration = prepared
        .working
        .as_ref()
        .map(|(_, c)| -> Result<_> {
            Ok((
                gpu.uniform(&c.params),
                gpu.upload("Multiscale spatial calibration", &c.grid_values)?,
            ))
        })
        .transpose()?;
    let (prepare, mut bindings) = match &evidence {
        Evidence::Native { black, .. } => (
            Kernel::MultiscalePrepare,
            vec![
                (0, Read(&uniform)),
                (1, Read(input)),
                (5, Write(current.write())),
                (8, Read(black)),
            ],
        ),
        Evidence::Calibrated { .. } => {
            let (params, values) = calibration.as_ref().unwrap();
            (
                Kernel::MultiscalePrepareCalibrated,
                vec![
                    (0, Read(&uniform)),
                    (1, Read(input)),
                    (5, Write(current.write())),
                    (10, Read(params)),
                    (11, Read(values)),
                ],
            )
        }
    };
    gpu.dispatch(
        prepare,
        &mut bindings,
        [
            full[0].div_ceil(WORKGROUP_SIDE),
            full[1].div_ceil(WORKGROUP_SIDE),
        ],
        &mut trace,
    );
    drop(bindings);

    for Stage {
        scale,
        radius,
        strength,
    } in prepared.stages()
    {
        let coarse_extent = full.map(|v| v / scale);
        params.stage = [coarse_extent[0], coarse_extent[1], scale, radius];
        params.controls = [strength, 1. / (scale * scale) as f32, VARIANCE_FLOOR, 0.];
        let uniform = gpu.uniform(&params);
        let groups = [
            coarse_extent[0].div_ceil(WORKGROUP_SIDE),
            coarse_extent[1].div_ceil(WORKGROUP_SIDE),
        ];
        let horizontal_groups = [
            coarse_extent[0].div_ceil(HORIZONTAL_WORKGROUP_WIDTH),
            coarse_extent[1].div_ceil(HORIZONTAL_WORKGROUP_HEIGHT),
        ];
        let coarse = if scale == 1 {
            current.clone()
        } else {
            let mut coarse = workspace(gpu, coarse_extent, 3)?;
            gpu.dispatch(
                Kernel::MultiscaleDownsample,
                &mut [
                    (0, Read(&uniform)),
                    (1, Read(&current)),
                    (5, Write(coarse.write())),
                ],
                groups,
                &mut trace,
            );
            coarse
        };
        // Separate moment planes keep individual buffers small at full resolution.
        let mut moments_a = workspace(gpu, coarse_extent, MOMENT_COMPONENTS)?;
        let mut moments_b = workspace(gpu, coarse_extent, 3)?;
        let mut moments_c = workspace(gpu, coarse_extent, 3)?;
        gpu.dispatch(
            Kernel::MultiscaleMoments,
            &mut [
                (0, Read(&uniform)),
                (1, Read(&coarse)),
                (5, Write(moments_a.write())),
                (6, Write(moments_b.write())),
                (7, Write(moments_c.write())),
            ],
            horizontal_groups,
            &mut trace,
        );
        let mut a = workspace(gpu, coarse_extent, 3)?;
        let mut b = workspace(gpu, coarse_extent, 3)?;
        let mut weight = workspace(gpu, coarse_extent, 3)?;
        gpu.dispatch(
            Kernel::MultiscaleCoefficients,
            &mut [
                (0, Read(&uniform)),
                (2, Read(&moments_a)),
                (3, Read(&moments_b)),
                (4, Read(&moments_c)),
                (5, Write(a.write())),
                (6, Write(b.write())),
                (7, Write(weight.write())),
            ],
            groups,
            &mut trace,
        );
        // The coefficient pass submitted the last moment reads. Reuse their
        // storage on the ordered queue. The four-component plane has enough
        // room for the three-component averaged weights.
        let mut average_a = moments_b;
        let mut average_b = moments_c;
        let mut average_weight = moments_a;
        gpu.dispatch(
            Kernel::MultiscaleAverage,
            &mut [
                (0, Read(&uniform)),
                (2, Read(&a)),
                (3, Read(&b)),
                (4, Read(&weight)),
                (5, Write(average_a.write())),
                (6, Write(average_b.write())),
                (7, Write(average_weight.write())),
            ],
            horizontal_groups,
            &mut trace,
        );
        // The average pass consumed the coefficient planes. Delta can replace
        // one of them without aliasing any input of the following pass.
        let mut delta = a;
        drop((b, weight));
        gpu.dispatch(
            Kernel::MultiscaleApply,
            &mut [
                (0, Read(&uniform)),
                (1, Read(&coarse)),
                (2, Read(&average_a)),
                (3, Read(&average_b)),
                (4, Read(&average_weight)),
                (5, Write(delta.write())),
            ],
            groups,
            &mut trace,
        );
        drop((average_a, average_b, average_weight, coarse));
        params.controls[3] = UPDATE_PHASE;
        let update = gpu.uniform(&params);
        let mut next = match spare.take() {
            Some(buffer) => buffer,
            None => workspace(gpu, full, 3)?,
        };
        gpu.dispatch(
            Kernel::MultiscaleApply,
            &mut [
                (0, Read(&update)),
                (1, Read(&current)),
                (2, Read(&delta)),
                (3, Read(&delta)),
                (4, Read(&delta)),
                (5, Write(next.write())),
            ],
            [
                full[0].div_ceil(WORKGROUP_SIDE),
                full[1].div_ceil(WORKGROUP_SIDE),
            ],
            &mut trace,
        );
        spare = Some(std::mem::replace(&mut current, next));
    }
    let mut output = gpu.storage("Multiscale sensor layers", bytes)?;
    let uniform = gpu.uniform(&params);
    let (finish, mut bindings) = match &evidence {
        Evidence::Native { original, black } => (
            Kernel::MultiscaleFinish,
            vec![
                (0, Read(&uniform)),
                (1, Read(&current)),
                (5, Write(output.write())),
                (8, Read(black)),
                (9, Read(original)),
            ],
        ),
        Evidence::Calibrated { clipping } => {
            let (params, values) = calibration.as_ref().unwrap();
            (
                Kernel::MultiscaleFinishCalibrated,
                vec![
                    (0, Read(&uniform)),
                    (1, Read(&current)),
                    (5, Write(output.write())),
                    (9, Read(input)),
                    (10, Read(params)),
                    (11, Read(values)),
                    (12, Read(clipping)),
                ],
            )
        }
    };
    gpu.dispatch(
        finish,
        &mut bindings,
        [
            width.div_ceil(WORKGROUP_SIDE),
            height.div_ceil(WORKGROUP_SIDE),
        ],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "multiscale_test.rs"]
mod tests;
