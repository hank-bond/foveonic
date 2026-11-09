//! Repairs sparse RGB defects with optional four-phase blue ghost and pattern passes.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::{Key, Shared},
    source::Result,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    shape: [u32; 4],
    offsets: [u32; 4],
    controls: [f32; 4],
    noise_rgb: [[f32; 4]; 3],
    noise_blue: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
    scratch_words: u32,
    lut: Shared<f32>,
}

impl Prepared {
    /// Selected single-image sparse grid: columns 16,32,... and rows 4,8,... .
    /// Noise triples describe constant, linear and quadratic variance terms.
    pub(crate) fn new(
        extent: [u32; 2],
        noise_rgb: [f32; 9],
        noise_blue: [f32; 3],
        scale: f32,
        preserve: bool,
    ) -> Result<Self> {
        let [width, height] = extent;
        let cells = u64::from(width) * u64::from(height);
        if cells > u64::from(u32::MAX) {
            return Err("sparse repair geometry exceeds shader indexing".into());
        }
        let buckets = u64::from(width.div_ceil(16)) * u64::from(height);
        let scratch_words = cells * 6 + buckets * 4 + 4;
        if width < 64
            || height < 20
            || scratch_words > u64::from(u32::MAX)
            || noise_rgb.iter().chain(&noise_blue).any(|v| !v.is_finite())
            || !scale.is_finite()
            || scale <= 0.
        {
            return Err("invalid sparse repair controls or geometry".into());
        }
        let n = cells as u32;
        let density = n * 6;
        let gate = density + buckets as u32 * 4;
        let lut: Vec<f32> = (0..1024)
            .map(|i| (1. + (f64::from((i as f32 / 1023.) * 3.) * -2.).tanh()) as f32)
            .collect();
        Ok(Self {
            params: Params {
                shape: [width, height, 0, u32::from(preserve)],
                offsets: [n, density, gate, width.div_ceil(16)],
                // Ghost-pattern strength and its second tanh table's endpoint.
                controls: [scale, 0.75, (1. + (-3_f64).tanh()) as f32, 0.],
                noise_rgb: std::array::from_fn(|i| {
                    [
                        noise_rgb[i * 3],
                        noise_rgb[i * 3 + 1],
                        noise_rgb[i * 3 + 2],
                        0.,
                    ]
                }),
                noise_blue: [noise_blue[0], noise_blue[1], noise_blue[2], 0.],
            },
            scratch_words: scratch_words as u32,
            lut: lut.into(),
        })
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    /// Select the native LOW three-component input model. Omitting a HIGH
    /// blue output must not change its six-component repair features.
    pub(crate) fn with_rgb_only(mut self) -> Self {
        self.params.controls[3] = 1.;
        self.scratch_words = self.params.offsets[0] * 2 + 4;
        self
    }
    pub(crate) fn rgb_only(&self) -> bool {
        self.params.controls[3] != 0.
    }
    pub(crate) fn blue_extent(&self) -> [u32; 2] {
        let factor = if self.rgb_only() { 1 } else { 2 };
        self.extent().map(|v| v * factor)
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params).join(self.lut.key())
    }

    fn last_phase(&self, requested: Requested) -> Phase {
        if self.rgb_only() {
            Phase::DirectionalClamp
        } else if requested.blue {
            Phase::SmoothResidual
        } else if requested.defects {
            Phase::Density
        } else {
            Phase::DirectionalClamp
        }
    }
}

/// RGB is needed internally for every repair. Side outputs can stop execution
/// before the later blue-only processing without changing the feature model.
#[derive(Clone, Copy, Default)]
pub(crate) struct Requested {
    pub blue: bool,
    pub defects: bool,
}

pub(crate) struct Output {
    pub(crate) rgb: ReadGuard,
    pub(crate) blue: Option<ReadGuard>,
    /// One unsigned byte-valued word per RGB pixel.
    pub(crate) defects: Option<ReadGuard>,
}

#[derive(Clone, Copy)]
enum Phase {
    Initialize,
    HotPixels,
    AugmentMask,
    CountEligible,
    Interpolation,
    SnapshotForClamp,
    DirectionalClamp,
    Density,
    SnapshotForPattern,
    PatternCorrection,
    MedianGuide,
    SmoothResidual,
}

pub(crate) fn apply(
    gpu: &Gpu,
    rgb: &ReadGuard,
    blue: &ReadGuard,
    prepared: &Prepared,
    requested: Requested,
    profile: &mut Profile,
) -> Result<Output> {
    let [width, height] = prepared.extent();
    let n = u64::from(width) * u64::from(height);
    let [bw, bh] = prepared.blue_extent();
    if rgb.size() != n * 12 || blue.size() != u64::from(bw) * u64::from(bh) * 4 {
        return Err("sparse repair inputs do not match their extents".into());
    }
    let mut output_rgb = gpu.storage("Sparse repaired RGB", n * 12)?;
    let blue_bytes = if prepared.rgb_only() { 4 } else { n * 16 };
    let mut work_blue = gpu.storage("Sparse repaired blue before smoothing", blue_bytes)?;
    let last_phase = prepared.last_phase(requested);
    let complete_blue = !prepared.rgb_only() && requested.blue;
    let mut final_blue = gpu.storage(
        "Sparse repaired blue",
        if complete_blue { blue_bytes } else { 4 },
    )?;
    let mut defects = gpu.storage("Sparse defect mask", n * 4)?;
    let mut scratch = gpu.storage(
        "Sparse repair phase snapshots and density",
        if !prepared.rgb_only() && !requested.blue && !requested.defects {
            // The first four phase snapshots and classification plane are the
            // complete RGB dependency. Density and the median guide are later.
            n * 5 * 4 + 4
        } else {
            u64::from(prepared.scratch_words) * 4 + 4
        },
    )?;
    // The response table is read only by blue pattern correction. Earlier
    // phases still require a binding, which can borrow an existing read input.
    let lut = if complete_blue {
        gpu.upload("Ghost pattern response", &prepared.lut)?
    } else {
        rgb.clone()
    };
    // The sparse grid separates interpolation stores from neighboring reads.
    // Density reconstructs the ordered mask bits from an immutable snapshot.
    let phases = [
        Phase::Initialize,
        Phase::HotPixels,
        Phase::AugmentMask,
        Phase::CountEligible,
        Phase::Interpolation,
        Phase::SnapshotForClamp,
        Phase::DirectionalClamp,
        Phase::Density,
        Phase::SnapshotForPattern,
        Phase::PatternCorrection,
        Phase::MedianGuide,
        Phase::SmoothResidual,
    ];
    let count = last_phase as usize + 1;
    let mut trace = gpu.trace(profile.mode.gpu(), count as u32);
    for phase in phases.into_iter().take(count) {
        let phase = phase as u32;
        let mut params = prepared.params;
        params.shape[2] = phase;
        let uniform = gpu.uniform(&params);
        gpu.dispatch(
            Kernel::SparseRepair,
            &mut [
                (0, Read(rgb)),
                (1, Read(blue)),
                (2, Read(&uniform)),
                (3, Write(output_rgb.write())),
                (4, Write(work_blue.write())),
                (5, Write(defects.write())),
                (6, Write(scratch.write())),
                (7, Read(&lut)),
                (8, Write(final_blue.write())),
            ],
            [width.div_ceil(8), height.div_ceil(8)],
            &mut trace,
        );
    }
    gpu.finish_trace(&trace, profile)?;
    Ok(Output {
        rgb: output_rgb,
        blue: if !requested.blue {
            None
        } else if prepared.rgb_only() {
            Some(blue.clone())
        } else {
            Some(final_blue)
        },
        defects: requested.defects.then_some(defects),
    })
}

#[cfg(test)]
#[path = "sparse_repair_test.rs"]
mod tests;
