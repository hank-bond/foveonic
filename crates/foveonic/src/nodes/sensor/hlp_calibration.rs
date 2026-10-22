//! Applies sparse blue correction and ordered shading while retaining pre-color scratch samples.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

const BLUE_CAP: f32 = 14300.;
const LUT_ENTRIES: usize = 32768;
// The reference wrapper retains RGB at physical (16*x, 4*y), excluding zero.
const SCRATCH_STEP: [u32; 2] = [16, 4];

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Gain {
    pub ordinary: [f32; 4],
    pub mono: [f32; 4],
    pub red_green: [f32; 4],
    pub blue: [f32; 4],
    pub phases: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    shape: [u32; 4],
    special: [u32; 4],
    controls: [f32; 4],
}
#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
    lut: Vec<u32>,
    gains: Vec<Gain>,
}
impl Prepared {
    /// Gains are row-major over sparse positions. RGB gain pairs retain the
    /// high and low Float32 parts of the prepared double-precision gain.
    pub(crate) fn new(
        extent: [u32; 2],
        special: [u32; 4],
        lut: Vec<u32>,
        gains: Vec<Gain>,
        scale: f32,
    ) -> Result<Self> {
        let [w, h] = extent;
        let [x, y, dx, dy] = special;
        if w <= SCRATCH_STEP[0]
            || h <= SCRATCH_STEP[1]
            || dx == 0
            || dy == 0
            || x >= w
            || y >= h
            || !scale.is_finite()
            || scale <= 0.
            || lut.len() != LUT_ENTRIES
            || lut.iter().any(|&v| v > 65535)
        {
            return Err("invalid sparse calibration geometry or LUT".into());
        }
        let nx = (w - x).div_ceil(dx);
        let ny = (h - y).div_ceil(dy);
        if gains.len() != nx as usize * ny as usize
            || bytemuck::cast_slice::<Gain, f32>(&gains)
                .iter()
                .any(|v| !v.is_finite())
        {
            return Err("invalid sparse calibration gains".into());
        }
        Ok(Self {
            params: Params {
                shape: [w, h, nx, ny],
                special,
                controls: [scale, 1. / scale, BLUE_CAP, 0.],
            },
            lut,
            gains,
        })
    }
    pub(crate) fn with_blue_ceiling(mut self, ceiling: f32) -> Result<Self> {
        if !ceiling.is_finite() || ceiling <= 0. {
            return Err("invalid sparse blue clipping ceiling".into());
        }
        self.params.controls[2] = ceiling;
        Ok(self)
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn scratch_extent(&self) -> [u32; 2] {
        [
            (self.params.shape[0] - 1) / SCRATCH_STEP[0],
            (self.params.shape[1] - 1) / SCRATCH_STEP[1],
        ]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params).slice(&self.lut).slice(&self.gains)
    }
}
pub(crate) struct Output {
    pub rgb: ReadGuard,
    pub blue: ReadGuard,
    pub scratch: ReadGuard,
}
pub(crate) fn apply(
    gpu: &Gpu,
    rgb: &ReadGuard,
    blue: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<Output> {
    let [w, h] = prepared.extent();
    let cells = u64::from(w) * u64::from(h);
    if rgb.size() != cells * 12 || blue.size() != cells * 16 {
        return Err("sparse calibration input extent mismatch".into());
    }
    let mut out_rgb = gpu.storage("Sparse corrected RGB", cells * 12)?;
    let mut out_blue = gpu.storage("Sparse corrected blue", cells * 16)?;
    let [sw, sh] = prepared.scratch_extent();
    let mut scratch = gpu.storage(
        "Pre-color shading samples",
        u64::from(sw) * u64::from(sh) * 12,
    )?;
    let params = gpu.uniform(&prepared.params);
    let lut = gpu.upload("Sparse correction LUT", &prepared.lut)?;
    let gains = gpu.upload("Sparse shading gains", &prepared.gains)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::HlpCalibration,
        &mut [
            (0, Read(rgb)),
            (1, Read(blue)),
            (2, Read(&params)),
            (3, Read(&lut)),
            (4, Read(&gains)),
            (5, Write(out_rgb.write())),
            (6, Write(out_blue.write())),
            (7, Write(scratch.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(Output {
        rgb: out_rgb,
        blue: out_blue,
        scratch,
    })
}
#[cfg(test)]
#[path = "hlp_calibration_test.rs"]
mod tests;
