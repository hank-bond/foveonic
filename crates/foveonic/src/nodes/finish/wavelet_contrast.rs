//! Adjusts packed-Lab lightness through six lifting levels and bounded detail gains.
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};
const LEVELS: u32 = 6;
const TRANSFORM_DIVISOR: u32 = 1 << LEVELS;
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    shape: [u32; 4],
    crop: [u32; 4],
    controls: [f32; 4],
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct BandControl {
    pub gain: f32,
    pub limit: f32,
    pub slope: f32,
}
#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
    gains: [[f32; 4]; 6],
    kernel: [f32; 9],
}
impl Prepared {
    /// Padded dimensions retain the crop's original phase. Each dimension must be
    /// divisible by64. The first packed component is lightness in unsigned16 codes.
    pub(crate) fn new(
        extent: [u32; 2],
        padded: [u32; 2],
        crop: [u32; 4],
        bands: [BandControl; 6],
        mut kernel: [f32; 9],
        bound: f32,
    ) -> Result<Self> {
        let [w, h] = extent;
        let [pw, ph] = padded;
        let [left, top, right, bottom] = crop;
        if w == 0
            || h == 0
            || pw < w
            || ph < h
            || pw % TRANSFORM_DIVISOR != 0
            || ph % TRANSFORM_DIVISOR != 0
            || left >= right
            || top >= bottom
            || right > w
            || bottom > h
            || u64::from(pw) * u64::from(ph) > u64::from(u32::MAX) / 3
            || kernel.iter().any(|v| !v.is_finite())
            || bands.iter().any(|b| {
                ![b.gain, b.limit, b.slope].iter().all(|v| v.is_finite())
                    || !(1. ..=10.).contains(&b.gain)
                    || b.limit < 0.
                    || b.slope < 0.
            })
            || !(0. ..=1.).contains(&bound)
        {
            return Err("invalid six-level wavelet contrast controls".into());
        }
        // The reference mutates a shared kernel in every lowest-band worker. Normalize
        // one owned copy for deterministic processing, including level6's oscillation.
        let sum = kernel.iter().fold(0_f32, |sum, v| sum + v);
        if sum > 0. && sum != 1. {
            kernel = kernel.map(|v| v / sum);
        }
        let inverse = 1_f64 / f64::from(std::f32::consts::SQRT_2);
        let high = inverse as f32;
        Ok(Self {
            params: Params {
                shape: [pw, ph, w, h],
                crop,
                controls: [
                    bound,
                    std::f32::consts::SQRT_2,
                    high,
                    (inverse - f64::from(high)) as f32,
                ],
            },
            gains: bands.map(|b| [b.gain, b.limit, b.slope, 0.]),
            kernel,
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[2], self.params.shape[3]]
    }
    /// Preserve global lifting phase and physical-edge padding in a cropped plane.
    pub(crate) fn on_region(mut self, rect: [u32; 4]) -> Result<Self> {
        let [left, top, right, bottom] = rect;
        let [pw, ph, w, h] = self.params.shape;
        if left >= right
            || top >= bottom
            || right > w
            || bottom > h
            || left % TRANSFORM_DIVISOR != 0
            || top % TRANSFORM_DIVISOR != 0
        {
            return Err("unaligned wavelet preview region".into());
        }
        let extent = [right - left, bottom - top];
        self.params.shape = [
            if right == w {
                pw - left
            } else {
                right.div_ceil(TRANSFORM_DIVISOR) * TRANSFORM_DIVISOR - left
            },
            if bottom == h {
                ph - top
            } else {
                bottom.div_ceil(TRANSFORM_DIVISOR) * TRANSFORM_DIVISOR - top
            },
            extent[0],
            extent[1],
        ];
        self.params.crop = [
            self.params.crop[0].saturating_sub(left).min(extent[0]),
            self.params.crop[1].saturating_sub(top).min(extent[1]),
            self.params.crop[2].saturating_sub(left).min(extent[0]),
            self.params.crop[3].saturating_sub(top).min(extent[1]),
        ];
        if self.params.crop[0] >= self.params.crop[2] || self.params.crop[1] >= self.params.crop[3]
        {
            return Err("regional wavelet crop misses active image".into());
        }
        Ok(self)
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params)
            .slice(&self.gains)
            .slice(&self.kernel)
    }
}
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [w, h] = prepared.extent();
    if input.size() != u64::from(w) * u64::from(h) * 12 {
        return Err("wavelet contrast packed geometry mismatch".into());
    }
    let [pw, ph, _, _] = prepared.params.shape;
    let bytes = u64::from(pw) * u64::from(ph) * 4;
    let mut a = gpu.storage("Wavelet first plane", bytes)?;
    let mut b = gpu.storage("Wavelet second plane", bytes)?;
    let mut output = gpu.storage("Wavelet local contrast", input.size())?;
    let params = gpu.uniform(&prepared.params);
    let gains = gpu.upload("Wavelet detail gains", &prepared.gains)?;
    let kernel = gpu.upload("Lowest-band sharpening kernel", &prepared.kernel)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 27);
    gpu.dispatch(
        Kernel::DwtExtract,
        &mut [(0, Read(input)), (1, Read(&params)), (3, Write(a.write()))],
        [pw.div_ceil(8), ph.div_ceil(8)],
        &mut trace,
    );
    // Columns precede rows. Each destination writes only the active low/low area;
    // coefficients outside it remain in the persistent first plane.
    for level in 0..LEVELS {
        let width = pw >> level;
        let height = ph >> level;
        let column = gpu.uniform(&[width, height, 1_u32, 0]);
        gpu.dispatch(
            Kernel::DwtForward,
            &mut [
                (1, Read(&params)),
                (2, Read(&a)),
                (3, Write(b.write())),
                (4, Read(&column)),
            ],
            [width.div_ceil(8), height.div_ceil(8)],
            &mut trace,
        );
        let row = gpu.uniform(&[width, height, 0_u32, 0]);
        gpu.dispatch(
            Kernel::DwtForward,
            &mut [
                (1, Read(&params)),
                (2, Read(&b)),
                (3, Write(a.write())),
                (4, Read(&row)),
            ],
            [width.div_ceil(8), height.div_ceil(8)],
            &mut trace,
        );
    }
    gpu.dispatch(
        Kernel::DwtGain,
        &mut [
            (1, Read(&params)),
            (2, Read(&a)),
            (3, Write(b.write())),
            (5, Read(&gains)),
            (6, Read(&kernel)),
        ],
        [pw.div_ceil(8), ph.div_ceil(8)],
        &mut trace,
    );
    // Inverse rows precede columns. The second plane now retains completed bands.
    for level in (0..LEVELS).rev() {
        let width = pw >> level;
        let height = ph >> level;
        let row = gpu.uniform(&[width, height, 0_u32, 0]);
        gpu.dispatch(
            Kernel::DwtInverse,
            &mut [
                (1, Read(&params)),
                (2, Read(&b)),
                (3, Write(a.write())),
                (4, Read(&row)),
            ],
            [width.div_ceil(8), height.div_ceil(8)],
            &mut trace,
        );
        let column = gpu.uniform(&[width, height, 1_u32, 0]);
        gpu.dispatch(
            Kernel::DwtInverse,
            &mut [
                (1, Read(&params)),
                (2, Read(&a)),
                (3, Write(b.write())),
                (4, Read(&column)),
            ],
            [width.div_ceil(8), height.div_ceil(8)],
            &mut trace,
        );
    }
    gpu.dispatch(
        Kernel::DwtCopyback,
        &mut [
            (0, Read(input)),
            (1, Read(&params)),
            (2, Read(&b)),
            (3, Write(output.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
#[cfg(test)]
#[path = "wavelet_contrast_test.rs"]
mod tests;
