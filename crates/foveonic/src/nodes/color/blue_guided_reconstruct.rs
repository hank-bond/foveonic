//! Reconstructs code RGB from color histories and blue at matching or doubled output resolution.
//!
//! Signed guide values retain the reference signed16 wrap in Float32 storage.
//! GPU chroma-gate exponentiation can differ from native expf by one gain ULP;
//! whole-frame comparisons measure the resulting integer-code error explicitly.
//! The doubled route's physical margins use the nonnegative resized guide and
//! unit residual, independently of native allocation aliases.
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};
// Kept in the parameter key so same-size and doubled reconstruction never alias.
// Shader entry points are selected on the host. The flag is not a camera mode.
const SAME_SIZE_OUTPUT: f32 = 1.;
// Minimum accepted low-resolution side for the 5×5 reconstruction neighborhood.
const MIN_SIDE: u32 = 6;
// Four output pixels per input cell, with three Float32 components per output.
const OUTPUT_COMPONENTS_PER_CELL: u32 = 4 * 3;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    shape: [u32; 4],
    /// Chroma suppression, adaptive range flag, reserved, same-size-output flag.
    controls: [f32; 4],
    noise: [f32; 4],
    noise_low: [f32; 4],
}
#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
    spatial: Vec<[f32; 2]>,
    blue_filter: Option<super::histogram_bilateral::Prepared>,
}
impl Prepared {
    pub(crate) fn new(
        extent: [u32; 2],
        suppression: f32,
        noise: [f64; 3],
        blue_filter: Option<super::histogram_bilateral::Prepared>,
    ) -> Result<Self> {
        let [w, h] = extent;
        if w < MIN_SIDE
            || h < MIN_SIDE
            || u64::from(w) * u64::from(h)
                > u64::from(u32::MAX) / u64::from(OUTPUT_COMPONENTS_PER_CELL)
            || !suppression.is_finite()
            || !(0. ..=1.).contains(&suppression)
            || blue_filter
                .as_ref()
                .is_some_and(|filter| filter.extent() != [w * 2, h * 2])
            || noise
                .iter()
                .any(|v| !v.is_finite() || !(*v as f32).is_finite() || *v < 0.)
        {
            return Err("invalid blue-guided reconstruction controls".into());
        }
        let mut spatial = vec![];
        for (px, py) in [(false, false), (false, true), (true, false), (true, true)] {
            for y in -2..=2 {
                for x in -2..=2 {
                    let dx = x as f64 + if px { -0.25 } else { 0.25 };
                    let dy = y as f64 + if py { -0.25 } else { 0.25 };
                    let distance = dx.hypot(dy) as f32;
                    spatial.push([distance, (-distance).exp()]);
                }
            }
        }
        Ok(Self {
            params: Params {
                shape: [w, h, w * 2, h * 2],
                controls: [suppression, 0., 0., 0.],
                noise: [noise[0] as f32, noise[1] as f32, noise[2] as f32, 0.],
                noise_low: [
                    (noise[0] - f64::from(noise[0] as f32)) as f32,
                    (noise[1] - f64::from(noise[1] as f32)) as f32,
                    (noise[2] - f64::from(noise[2] as f32)) as f32,
                    0.,
                ],
            },
            spatial,
            blue_filter,
        })
    }
    /// Average the doubled blue plane onto the color grid, then derive its
    /// residual against the guide and pack the existing color without upsampling.
    pub(crate) fn same_size(
        extent: [u32; 2],
        noise: [f64; 3],
        filter: Option<super::histogram_bilateral::Prepared>,
    ) -> Result<Self> {
        if filter
            .as_ref()
            .is_some_and(|filter| filter.extent() != extent)
        {
            return Err("same-size reconstruction filter geometry mismatch".into());
        }
        let mut value = Self::new(extent, 0., noise, None)?;
        value.params.controls[3] = SAME_SIZE_OUTPUT;
        value.blue_filter = filter;
        Ok(value)
    }
    pub(crate) fn with_adaptive_range(mut self, enabled: bool) -> Self {
        self.params.controls[1] = f32::from(u8::from(enabled));
        self
    }
    pub(crate) fn output_extent(&self) -> [u32; 2] {
        if self.is_same_size() {
            self.extent()
        } else {
            self.extent().map(|v| v * 2)
        }
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    fn is_same_size(&self) -> bool {
        self.params.controls[3] == SAME_SIZE_OUTPUT
    }
    pub(crate) fn with_extent(mut self, extent: [u32; 2]) -> Result<Self> {
        if extent.iter().any(|&v| v < MIN_SIDE)
            || u64::from(extent[0]) * u64::from(extent[1])
                > u64::from(u32::MAX) / u64::from(OUTPUT_COMPONENTS_PER_CELL)
        {
            return Err("invalid regional reconstruction extent".into());
        }
        self.params.shape = [extent[0], extent[1], extent[0] * 2, extent[1] * 2];
        let output_extent = self.output_extent();
        self.blue_filter = self
            .blue_filter
            .map(|f| f.with_extent(output_extent))
            .transpose()?;
        Ok(self)
    }
    pub(crate) fn key(&self) -> Key {
        let key = Key::pod(&self.params).slice(&self.spatial);
        match &self.blue_filter {
            Some(filter) => key.with(&1_u32).join(filter.key()),
            None => key.with(&0_u32),
        }
    }
}
pub(crate) struct Output {
    pub rgb: ReadGuard,
    pub residual: ReadGuard,
}
pub(crate) fn apply(
    gpu: &Gpu,
    guide: &ReadGuard,
    preprocessed: &ReadGuard,
    color: &ReadGuard,
    blue: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<Output> {
    let [w, h] = prepared.extent();
    let low = u64::from(w) * u64::from(h);
    let high = low * 4;
    if guide.size() != low * 12
        || preprocessed.size() != low * 12
        || color.size() != low * 12
        || blue.size() != high * 4
    {
        return Err("blue-guided reconstruction input geometry mismatch".into());
    }
    if prepared.is_same_size() {
        return apply_same_size(gpu, guide, color, blue, prepared, profile);
    }
    let uniform = gpu.uniform(&prepared.params);
    let spatial = gpu.upload("Quarter-cell spatial weights", &prepared.spatial)?;
    let mut vertical = gpu.storage("Double-word resized vertical guide", low * 2 * 3 * 8)?;
    let mut signed = gpu.storage("Signed code guide", high * 12)?;
    let mut repaired = gpu.storage("Blue outlier correction", high * 4)?;
    let mut rgb = gpu.storage("Reconstructed unsigned code RGB", high * 12)?;
    let mut residual = gpu.storage("Reconstructed residual gain", high * 4)?;
    let mut trace = gpu.trace(
        profile.mode.gpu(),
        if prepared.blue_filter.is_some() { 3 } else { 4 },
    );
    gpu.dispatch(
        Kernel::BlueGuideVertical,
        &mut [
            (0, Read(&uniform)),
            (1, Read(preprocessed)),
            (5, Write(vertical.write())),
        ],
        [w.div_ceil(8), (h * 2).div_ceil(8)],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::BlueOutliers,
        &mut [
            (0, Read(&uniform)),
            (4, Read(blue)),
            (7, Write(repaired.write())),
        ],
        [(w * 2).div_ceil(8), (h * 2).div_ceil(8)],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::BlueGuideHorizontal,
        &mut [
            (0, Read(&uniform)),
            (4, Read(&repaired)),
            (5, Read(&vertical)),
            (6, Write(signed.write())),
        ],
        [(w * 2).div_ceil(8), (h * 2).div_ceil(8)],
        &mut trace,
    );
    // The signed guide follows repaired original blue. Filtering changes only
    // the restoration residual, so retain both immutable blue histories.
    let filtered = if let Some(filter) = &prepared.blue_filter {
        gpu.finish_trace(&trace, profile)?;
        let output = super::histogram_bilateral::apply(gpu, &repaired, filter, profile)?;
        trace = gpu.trace(profile.mode.gpu(), 1);
        Some(output)
    } else {
        None
    };
    let restoration_blue = filtered.as_ref().unwrap_or(&repaired);
    gpu.dispatch(
        Kernel::BlueGuidedRestore,
        &mut [
            (0, Read(&uniform)),
            (1, Read(guide)),
            (2, Read(color)),
            (3, Read(&spatial)),
            (4, Read(restoration_blue)),
            (6, Read(&signed)),
            (8, Write(rgb.write())),
            (9, Write(residual.write())),
        ],
        [(w * 2).div_ceil(8), (h * 2).div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(Output { rgb, residual })
}
fn apply_same_size(
    gpu: &Gpu,
    guide: &ReadGuard,
    color: &ReadGuard,
    blue: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<Output> {
    let [w, h] = prepared.extent();
    let n = u64::from(w) * u64::from(h);
    let uniform = gpu.uniform(&prepared.params);
    let mut mean = gpu.storage("Averaged calibrated blue", n * 4)?;
    let mut rgb = gpu.storage("Same-resolution code RGB", n * 12)?;
    let mut residual = gpu.storage("Same-resolution detail residual", n * 4)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::ReducedBlue,
        &mut [
            (0, Read(&uniform)),
            (4, Read(blue)),
            (7, Write(mean.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    let filtered = prepared
        .blue_filter
        .as_ref()
        .map(|f| super::histogram_bilateral::apply(gpu, &mean, f, profile))
        .transpose()?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::ReducedRestore,
        &mut [
            (0, Read(&uniform)),
            (1, Read(guide)),
            (2, Read(color)),
            (4, Read(filtered.as_ref().unwrap_or(&mean))),
            (8, Write(rgb.write())),
            (9, Write(residual.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(Output { rgb, residual })
}

#[cfg(test)]
#[path = "blue_guided_reconstruct_test.rs"]
mod tests;
