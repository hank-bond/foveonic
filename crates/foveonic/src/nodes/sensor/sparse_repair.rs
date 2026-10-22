//! Repairs sparse RGB and four-phase blue defects through ordered ghost and pattern passes.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
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
    lut: Vec<f32>,
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
        let lut = (0..1024)
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
            lut,
        })
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params).slice(&self.lut)
    }
}

pub(crate) struct Output {
    pub(crate) rgb: ReadGuard,
    pub(crate) blue: ReadGuard,
    /// One unsigned byte-valued word per RGB pixel.
    pub(crate) defects: ReadGuard,
}

pub(crate) fn apply(
    gpu: &Gpu,
    rgb: &ReadGuard,
    blue: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<Output> {
    let [width, height] = prepared.extent();
    let n = u64::from(width) * u64::from(height);
    if rgb.size() != n * 12 || blue.size() != n * 16 {
        return Err("sparse repair inputs do not match their extents".into());
    }
    let mut output_rgb = gpu.storage("Sparse repaired RGB", n * 12)?;
    let mut work_blue = gpu.storage("Sparse repaired blue before smoothing", n * 16)?;
    let mut final_blue = gpu.storage("Sparse repaired blue", n * 16)?;
    let mut defects = gpu.storage("Sparse defect mask", n * 4)?;
    let mut scratch = gpu.storage(
        "Sparse repair phase snapshots and density",
        u64::from(prepared.scratch_words) * 4 + 4,
    )?;
    let lut = gpu.upload("Ghost pattern response", &prepared.lut)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 12);
    // The sparse grid separates interpolation stores from neighboring reads.
    // Density reconstructs the ordered mask bits from an immutable snapshot.
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
    for phase in [
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
    ] {
        let mut params = prepared.params;
        params.shape[2] = phase as u32;
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
        blue: final_blue,
        defects,
    })
}

#[cfg(test)]
#[path = "sparse_repair_test.rs"]
mod tests;
