//! Filters native sensor-layer color differences at three scales while preserving their sum.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

const PAD_MULTIPLE: u32 = 16;
const MINIMUM_ACTIVE_SIDE: u32 = 65;
const WORKGROUP_SIDE: u32 = 8;
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
}

#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
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
            },
        })
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.source[0], self.params.source[1]]
    }

    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params)
    }
}

fn workspace(gpu: &Gpu, extent: [u32; 2], components: u32) -> Result<ReadGuard> {
    gpu.storage(
        "Multiscale workspace",
        u64::from(extent[0]) * u64::from(extent[1]) * u64::from(components) * FLOAT_BYTES,
    )
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    original: &ReadGuard,
    black: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height] = prepared.extent();
    let bytes = u64::from(width) * u64::from(height) * LAYER_BYTES;
    if input.size() != bytes || original.size() != bytes || black.size() != LAYER_BYTES {
        return Err("multiscale sensor filter input geometry mismatch".into());
    }
    let full = [prepared.params.source[2], prepared.params.source[3]];
    let mut current = workspace(gpu, full, 3)?;
    let mut params = prepared.params;
    let uniform = gpu.uniform(&params);
    let mut trace = gpu.trace(profile.mode.gpu(), MAXIMUM_TRACE_PASSES);
    gpu.dispatch(
        Kernel::MultiscalePrepare,
        &mut [
            (0, Read(&uniform)),
            (1, Read(input)),
            (5, Write(current.write())),
            (8, Read(black)),
        ],
        [
            full[0].div_ceil(WORKGROUP_SIDE),
            full[1].div_ceil(WORKGROUP_SIDE),
        ],
        &mut trace,
    );

    for Stage {
        scale,
        radius,
        strength,
    } in STAGES
    {
        let coarse_extent = full.map(|v| v / scale);
        params.stage = [coarse_extent[0], coarse_extent[1], scale, radius];
        params.controls = [strength, 1. / (scale * scale) as f32, VARIANCE_FLOOR, 0.];
        let uniform = gpu.uniform(&params);
        let groups = [
            coarse_extent[0].div_ceil(WORKGROUP_SIDE),
            coarse_extent[1].div_ceil(WORKGROUP_SIDE),
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
            groups,
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
        drop((moments_a, moments_b, moments_c));
        let mut average_a = workspace(gpu, coarse_extent, 3)?;
        let mut average_b = workspace(gpu, coarse_extent, 3)?;
        let mut average_weight = workspace(gpu, coarse_extent, 3)?;
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
            groups,
            &mut trace,
        );
        drop((a, b, weight));
        let mut delta = workspace(gpu, coarse_extent, 3)?;
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
        let mut next = workspace(gpu, full, 3)?;
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
        current = next;
    }
    let mut output = gpu.storage("Multiscale native sensor layers", bytes)?;
    let uniform = gpu.uniform(&params);
    gpu.dispatch(
        Kernel::MultiscaleFinish,
        &mut [
            (0, Read(&uniform)),
            (1, Read(&current)),
            (5, Write(output.write())),
            (8, Read(black)),
            (9, Read(original)),
        ],
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
