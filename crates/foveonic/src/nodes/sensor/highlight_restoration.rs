//! Reconstructs saturated RGB through directional interpolation and inverse-camera neutralization.
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};
// Thresholds are linear sensor-code levels before normalization.
const CLIPPING_CEILING: f32 = 14000.;
const GREEN_MASK_LEVEL: f32 = 13000.;
const MINIMUM_CAMERA_LEVEL: f32 = 5000.;
const DIRECTIONAL_GREEN_DIFFERENCE: f32 = 9000.;
const NEUTRALIZATION_START: f32 = 11000.;
const NEUTRALIZATION_SPAN: f32 = 3500.;
const WORK_FLOATS_PER_PIXEL: u64 = 13;
// These tags are shared by the directional and channel-extension shaders.
const DIRECTIONAL_ONLY: u32 = 0;
const CHANNEL_EXTENSION: u32 = 1;
const CHANNEL_LIMITS_ONLY: u32 = 2;
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Params {
    shape: [u32; 4],
    rectangle: [u32; 4],
    gains: [f32; 4],
    thresholds: [f32; 4],
    neutralization: [f32; 4],
    ceilings: [f32; 4],
}
#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
    matrix: Vec<[f32; 2]>,
}
impl Prepared {
    pub(crate) fn new(
        extent: [u32; 2],
        rectangle: [u32; 4],
        normalization: f32,
        gains: [f32; 3],
        matrix: [f64; 9],
    ) -> Result<Self> {
        let [w, h] = extent;
        let [left, top, right, bottom] = rectangle;
        if w < 5
            || h < 5
            || left < 1
            || top < 1
            || right >= w
            || bottom >= h
            || left >= right
            || top >= bottom
            || !normalization.is_finite()
            || normalization <= 0.
            || gains.iter().any(|v| !v.is_finite() || *v <= 0.)
            || matrix.iter().any(|v| !v.is_finite())
        {
            return Err("unsupported directional highlight controls".into());
        }
        let inverse = reference_inverse(matrix)?;
        let coefficients = matrix
            .into_iter()
            .chain(inverse.map(f64::from))
            .map(|v| {
                let high = v as f32;
                [high, (v - f64::from(high)) as f32]
            })
            .collect();
        Ok(Self {
            params: Params {
                shape: [w, h, w * h, DIRECTIONAL_ONLY],
                rectangle,
                gains: [gains[0], gains[1], gains[2], normalization],
                thresholds: [
                    CLIPPING_CEILING / normalization,
                    (GREEN_MASK_LEVEL / normalization) * gains[1],
                    MINIMUM_CAMERA_LEVEL / normalization,
                    DIRECTIONAL_GREEN_DIFFERENCE / normalization,
                ],
                ceilings: [CLIPPING_CEILING / normalization; 4],
                neutralization: [
                    NEUTRALIZATION_START / normalization,
                    NEUTRALIZATION_SPAN / normalization,
                    CLIPPING_CEILING / normalization,
                    0.,
                ],
            },
            matrix: coefficients,
        })
    }
    /// Select channel-specific clipping and its optional spatial color inference.
    /// The flag describes numerical behavior rather than a camera family.
    pub(crate) fn with_channel_limits(
        mut self,
        ceilings: [f32; 3],
        green_level: f32,
        extend: bool,
    ) -> Result<Self> {
        if ceilings.iter().any(|v| !v.is_finite() || *v <= 0.)
            || !green_level.is_finite()
            || green_level <= 0.
        {
            return Err("invalid channel highlight limits".into());
        }
        self.params.shape[3] = if extend {
            CHANNEL_EXTENSION
        } else {
            CHANNEL_LIMITS_ONLY
        };
        self.params.ceilings[..3].copy_from_slice(&ceilings);
        self.params.thresholds[1] = green_level * self.params.gains[1];
        Ok(self)
    }
    pub(crate) fn has_recovery_mask(&self) -> bool {
        self.params.shape[3] != DIRECTIONAL_ONLY
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params).slice(&self.matrix)
    }
}
fn reference_inverse(matrix: [f64; 9]) -> Result<[f32; 9]> {
    // This helper deliberately narrows first and eliminates without row pivoting.
    let mut rows: [[f32; 6]; 3] = std::array::from_fn(|y| {
        std::array::from_fn(|x| {
            if x < 3 {
                matrix[y * 3 + x] as f32
            } else {
                u32::from(x - 3 == y) as f32
            }
        })
    });
    for pivot in 0..3 {
        let denominator = rows[pivot][pivot];
        for v in &mut rows[pivot] {
            *v /= denominator;
        }
        for y in 0..3 {
            if y == pivot {
                continue;
            }
            let factor = rows[y][pivot] / rows[pivot][pivot];
            let pivot_row = rows[pivot];
            for (x, value) in rows[y].iter_mut().enumerate() {
                *value = (-factor).mul_add(pivot_row[x], *value);
            }
        }
    }
    let result = std::array::from_fn(|i| rows[i / 3][i % 3 + 3]);
    if result.iter().any(|v| !v.is_finite()) {
        return Err("invalid source highlight neutralization inverse".into());
    }
    Ok(result)
}
pub(crate) struct Output {
    pub rgb: ReadGuard,
    pub mask: ReadGuard,
    pub recovery_mask: Option<ReadGuard>,
}
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<Output> {
    let [w, h] = prepared.extent();
    let cells = u64::from(w) * u64::from(h);
    if input.size() != cells * 12 {
        return Err("highlight RGB geometry mismatch".into());
    }
    let params = gpu.uniform(&prepared.params);
    let matrix = gpu.upload("Highlight camera matrices", &prepared.matrix)?;
    // Three RGB work planes and four scalar classifications share one allocation.
    let mut work = gpu.storage(
        "Directional highlight work",
        cells * WORK_FLOATS_PER_PIXEL * 4,
    )?;
    let mut rgb = gpu.storage("Restored RGB", cells * 12)?;
    let mut mask = gpu.storage("Highlight green mask", cells * 4)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 5);
    let recovery_mask = if prepared.has_recovery_mask() {
        Some(super::dynamic_range::prepare(
            gpu,
            input,
            &params,
            &matrix,
            &mut work,
            &mut mask,
            prepared.extent(),
            prepared.params.shape[3] == CHANNEL_EXTENSION,
            profile,
        )?)
    } else {
        gpu.dispatch(
            Kernel::HighlightPrepare,
            &mut [
                (0, Read(input)),
                (1, Read(&params)),
                (2, Read(&matrix)),
                (3, Write(work.write())),
                (5, Write(mask.write())),
            ],
            [w.div_ceil(8), h.div_ceil(8)],
            &mut trace,
        );
        None
    };
    gpu.dispatch(
        Kernel::HighlightClassify,
        &mut [(1, Read(&params)), (3, Write(work.write()))],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::HighlightHorizontal,
        &mut [(1, Read(&params)), (3, Write(work.write()))],
        [h.div_ceil(64), 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::HighlightVertical,
        &mut [(1, Read(&params)), (3, Write(work.write()))],
        [w.div_ceil(64), 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::HighlightFinish,
        &mut [
            (1, Read(&params)),
            (2, Read(&matrix)),
            (3, Read(&work)),
            (4, Write(rgb.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(Output {
        rgb,
        mask,
        recovery_mask,
    })
}
#[cfg(test)]
#[path = "highlight_restoration_test.rs"]
mod tests;
