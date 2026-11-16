//! Preserves native highlight evidence and reconstructs saturated RGB through directional interpolation.
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
    controls: [u32; 4],
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
                controls: [0; 4],
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
    /// Numerical RGB and original recovery evidence share one padded grid.
    /// Native detection coverage prevents pooled boundaries from widening.
    pub(crate) fn on_grid(mut self, grid: crate::render::WorkingGrid) -> Result<Self> {
        if grid.native != self.extent() {
            return Err("highlight restoration working grid differs from source".into());
        }
        let [w, h] = grid.extent;
        let mut rectangle = grid.bounds(self.params.rectangle);
        rectangle[0] = rectangle[0].max(1).min(w - 2);
        rectangle[1] = rectangle[1].max(1).min(h - 2);
        rectangle[2] = rectangle[2].max(rectangle[0] + 1).min(w - 1);
        rectangle[3] = rectangle[3].max(rectangle[1] + 1).min(h - 1);
        self.params.shape[..3].copy_from_slice(&[w, h, w * h]);
        self.params.rectangle = rectangle;
        self.params.controls[0] = 1;
        Ok(self)
    }
    pub(crate) fn requires_evidence(&self) -> bool {
        self.params.controls[0] != 0
    }

    pub(crate) fn evidence(&self, grid: crate::nodes::sensor::pool::Prepared) -> Result<Evidence> {
        if self.extent() != grid.extent() || !grid.valid_output_geometry() {
            return Err("highlight evidence grid differs from native source".into());
        }
        Ok(Evidence {
            params: self.params,
            matrix: self.matrix.clone(),
            grid,
        })
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params).slice(&self.matrix)
    }
}

/// A compact recovery control, separate from the numerical camera image.
#[derive(Clone, Debug)]
pub struct Evidence {
    params: Params,
    matrix: Vec<[f32; 2]>,
    grid: crate::nodes::sensor::pool::Prepared,
}
impl Evidence {
    pub(crate) fn grid(&self) -> crate::nodes::sensor::pool::Prepared {
        self.grid
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params).with(&self.grid).slice(&self.matrix)
    }
    pub(crate) fn apply(
        &self,
        gpu: &Gpu,
        input: &ReadGuard,
        profile: &mut Profile,
    ) -> Result<ReadGuard> {
        let native = self.grid.extent();
        if input.size() != u64::from(native[0]) * u64::from(native[1]) * 12 {
            return Err("highlight evidence source geometry mismatch".into());
        }
        let [w, h] = self.grid.output_extent();
        let mut output = gpu.storage(
            "Pooled original highlight evidence",
            u64::from(w) * u64::from(h) * 32,
        )?;
        let params = gpu.uniform(&self.params);
        let grid = gpu.uniform(&self.grid);
        let matrix = gpu.upload("Highlight evidence camera matrices", &self.matrix)?;
        let [logical_width, logical_height] = self.grid.logical_extent();
        let mut trace = gpu.trace(profile.mode.gpu(), 1);
        gpu.dispatch(
            Kernel::HighlightEvidence,
            &mut [
                (0, Read(input)),
                (1, Read(&params)),
                (2, Read(&matrix)),
                (4, Write(output.write())),
                (9, Read(&grid)),
            ],
            [logical_width.div_ceil(8), logical_height.div_ceil(8)],
            &mut trace,
        );
        gpu.finish_trace(&trace, profile)?;
        Ok(output)
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
pub(crate) struct EvidenceInput<'a> {
    pub controls: &'a ReadGuard,
    pub unfiltered: &'a ReadGuard,
}
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    evidence: Option<EvidenceInput<'_>>,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<Output> {
    let [w, h] = prepared.extent();
    let cells = u64::from(w) * u64::from(h);
    if input.size() != cells * 12 {
        return Err("highlight RGB geometry mismatch".into());
    }
    if prepared.requires_evidence() != evidence.is_some()
        || evidence.as_ref().is_some_and(|evidence| {
            evidence.controls.size() != cells * 32 || evidence.unfiltered.size() != input.size()
        })
    {
        return Err("highlight clipping evidence geometry mismatch".into());
    }
    let (evidence, unfiltered) = evidence
        .map(|evidence| (evidence.controls, evidence.unfiltered))
        .unwrap_or((input, input));
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
            evidence,
            unfiltered,
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
                (8, Read(evidence)),
                (10, Read(unfiltered)),
            ],
            [w.div_ceil(8), h.div_ceil(8)],
            &mut trace,
        );
        None
    };
    gpu.dispatch(
        Kernel::HighlightClassify,
        &mut [
            (0, Read(input)),
            (1, Read(&params)),
            (3, Write(work.write())),
            (8, Read(evidence)),
            (10, Read(unfiltered)),
        ],
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
            (0, Read(input)),
            (1, Read(&params)),
            (2, Read(&matrix)),
            (3, Read(&work)),
            (4, Write(rgb.write())),
            (8, Read(evidence)),
            (10, Read(unfiltered)),
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
