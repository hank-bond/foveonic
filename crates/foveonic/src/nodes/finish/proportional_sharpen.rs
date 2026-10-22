//! Sharpens integral RGB codes from signed quarter-code detail and an adaptive amount table.
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

const DETAIL_TO_RGB_CODES: f32 = 0.25;
// Low-luma protection compares the original RGB luminance in integral codes.
const LOW_LUMA_CUTOFF: f32 = 48.;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    shape: [u32; 4],
    controls: [f32; 4],
    limits: [f32; 4],
}
#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
    table: Vec<f32>,
}
impl Prepared {
    /// Ordinary proportional sharpening derives its amount from the table. Detail
    /// is signed16 in quarter-code units. RGB uses unsigned16 integral codes.
    /// This contract covers source quality below two.
    pub(crate) fn new(
        extent: [u32; 2],
        table: Vec<f32>,
        normalization: f32,
        absolute_limit: f32,
        relative_limit: f32,
        amount_scale: f32,
        preserve_low_luma: bool,
    ) -> Result<Self> {
        let [w, h] = extent;
        if w == 0
            || h == 0
            || u64::from(w) * u64::from(h) > u64::from(u32::MAX) / 3
            || table.is_empty()
            || table.len() > u32::MAX as usize
            || table.iter().any(|v| !v.is_finite() || *v < 0.)
            || [normalization, absolute_limit, relative_limit, amount_scale]
                .iter()
                .any(|v| !v.is_finite())
            || normalization <= 0.
            || absolute_limit < 0.
            || relative_limit < 0.
            || amount_scale < 0.
        {
            return Err("invalid ordinary proportional sharpening controls".into());
        }
        Ok(Self {
            params: Params {
                shape: [w, h, table.len() as u32, u32::from(preserve_low_luma)],
                controls: [
                    normalization,
                    amount_scale,
                    DETAIL_TO_RGB_CODES,
                    LOW_LUMA_CUTOFF,
                ],
                limits: [absolute_limit / normalization, relative_limit, 0., 0.],
            },
            table,
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params).slice(&self.table)
    }
}
pub(crate) fn apply(
    gpu: &Gpu,
    rgb: &ReadGuard,
    detail: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [w, h] = prepared.extent();
    let cells = u64::from(w) * u64::from(h);
    if rgb.size() != cells * 12 || detail.size() != cells * 4 {
        return Err("proportional sharpening input geometry mismatch".into());
    }
    let mut output = gpu.storage("Proportional sharpening", rgb.size())?;
    let params = gpu.uniform(&prepared.params);
    let table = gpu.upload("Adaptive sharpening amounts", &prepared.table)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::ProportionalSharpen,
        &mut [
            (0, Read(rgb)),
            (1, Read(detail)),
            (2, Read(&params)),
            (3, Read(&table)),
            (4, Write(output.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
#[cfg(test)]
#[path = "proportional_sharpen_test.rs"]
mod tests;
