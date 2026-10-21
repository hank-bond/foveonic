//! Reconstructs signed RGB from guided log residuals with caller-selected filtering controls.
//!
//! Pixel work uses Float32, including compensated edge statistics where the reference
//! implementation uses Float64. Four bounded native fixtures match all 9,315 words.
//! A complete SD15 boundary differs at 661 of 14,450,688 words, each by one code.
//! Those observations bound the tested inputs rather than promise bit-exact rendering.
//! Zero gradient variance uses a finite limiting sigmoid input. Exceptional native
//! scratch and floating-point behavior is not a portable processing requirement.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    source::Result,
};
// Log signal uses a recovered quantized inverse-exponential table. Keep its
// literal scale identical to the shader, rather than recomputing it from ln(2).
const LOG_TABLE_LEN: usize = 245760;
const LOG_CODE_SCALE: f32 = 23637.115;
const SIGNAL_CODE_UNIT: f32 = 4096.;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Params {
    shape: [u32; 4],
    controls: [f32; 4],
    extra: [f32; 4],
}
#[derive(Clone)]
pub struct Prepared {
    pub(crate) extent: [u32; 2],
    /// 0..=2: image-size-relative decay proportions for reference, edge reference,
    /// and edge-guided image filtering. 3: dimensionless edge locality.
    /// 4: sigmoid offset. 5..=6: dimensionless sigmoid slopes for the two guides.
    /// 7: normalized edge-map power. 8: color matrix green-row gain.
    /// 9: digital gain. The last two determine Q12 reconstruction strength.
    pub(crate) controls: [f32; 10],
    tables: Vec<f32>,
    range: u32,
}
impl Prepared {
    pub(crate) fn new(extent: [u32; 2], controls: [f32; 10]) -> Result<Self> {
        if extent.iter().any(|v| *v < 4)
            || controls.iter().any(|v| !v.is_finite())
            || controls[..4].iter().any(|v| *v <= 0.)
            || controls[9] <= 0.
        {
            return Err("invalid log reconstruction controls".into());
        }
        let range = 24000_u32.min(((9000. / controls[9]) * 4.) as u32);
        if range < 2 {
            return Err("log reconstruction range is empty".into());
        }
        let mut tables = Vec::with_capacity(range as usize * 3 + LOG_TABLE_LEN);
        tables.extend((0..range).map(|i| (i as f32 / range as f32).powf(controls[7])));
        tables.extend((0..range).map(|i| (i as f32 / range as f32 + 1e-12).ln()));
        tables.extend((0..LOG_TABLE_LEN).map(|i| {
            let x = (-(i as f32) / LOG_CODE_SCALE).exp() * range as f32;
            (x * 2.) as i32 as f32
        }));
        tables.extend((0..range).map(|i| (i as f32 / range as f32).powf(0.2)));
        Ok(Self {
            extent,
            controls,
            tables,
            range,
        })
    }
    pub(crate) fn key(&self) -> crate::recipe::Key {
        crate::recipe::Key::pod(&self.extent)
            .slice(&self.controls)
            .slice(&self.tables)
    }
    fn params(
        &self,
        extent: [u32; 2],
        channels: u32,
        flags: u32,
        decay: f32,
        slope: f32,
    ) -> Params {
        Params {
            shape: [extent[0], extent[1], channels, flags],
            controls: [decay, self.controls[3], self.controls[4], slope],
            extra: [
                self.range as f32,
                ((self.controls[8].min(7.) * self.controls[9]) * SIGNAL_CODE_UNIT) as i32 as f32,
                0.,
                0.,
            ],
        }
    }
    fn decay(&self, index: usize) -> f32 {
        1. / (std::f32::consts::LN_2
            / (((self.extent[0] + self.extent[1]) >> 2) as f32 * self.controls[index]))
            .exp()
    }
}
fn image(gpu: &Gpu, label: &str, extent: [u32; 2], channels: u32) -> Result<ReadGuard> {
    gpu.storage(
        label,
        u64::from(extent[0]) * u64::from(extent[1]) * u64::from(channels) * 4,
    )
}

/// Recursive passes keep complete rows in queue order. Camera identity never enters the worker.
fn recursive(
    gpu: &Gpu,
    input: &ReadGuard,
    guide: &ReadGuard,
    params: Params,
    seed: Option<&ReadGuard>,
    scratch: &mut ReadGuard,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let extent = [params.shape[0], params.shape[1]];
    let channels = params.shape[2];
    let mut output = image(gpu, "Recursive filtered transpose", extent, channels)?;
    if let Some(seed) = seed {
        gpu.copy(
            seed,
            output.write(),
            u64::from(extent[0]) * u64::from(extent[1]) * u64::from(channels) * 4,
        );
    }
    let uniform = gpu.uniform(&params);
    let mut trace = gpu.trace(profile.mode.gpu(), 2);
    gpu.dispatch(
        Kernel::LogForward,
        &mut [
            (0, Read(input)),
            (1, Read(guide)),
            (2, Read(&uniform)),
            (6, Write(scratch.write())),
        ],
        [extent[1].div_ceil(64), 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::LogBackward,
        &mut [
            (0, Read(input)),
            (1, Read(guide)),
            (2, Read(&uniform)),
            (4, Write(output.write())),
            (6, Read(scratch)),
        ],
        [extent[1].div_ceil(64), 1],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
fn edge(
    gpu: &Gpu,
    input: &ReadGuard,
    p: &Prepared,
    slope: f32,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [w, h] = p.extent;
    let mut gradient = image(gpu, "RGB gradient", p.extent, 1)?;
    let mut stats = gpu.storage("Compensated gradient row statistics", u64::from(h) * 16)?;
    let mut guide = image(gpu, "Sigmoid edge guidance", p.extent, 1)?;
    let mut deviation = gpu.storage("Gradient standard deviation", 16)?;
    let uniform = gpu.uniform(&p.params(p.extent, 3, 0, 0., slope));
    let mut trace = gpu.trace(profile.mode.gpu(), 4);
    gpu.dispatch(
        Kernel::LogGradient,
        &mut [
            (0, Read(input)),
            (2, Read(&uniform)),
            (4, Write(gradient.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::LogStats,
        &mut [
            (0, Read(&gradient)),
            (2, Read(&uniform)),
            (7, Write(stats.write())),
        ],
        [h.div_ceil(64), 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::LogReduce,
        &mut [
            (2, Read(&uniform)),
            (4, Write(deviation.write())),
            (7, Read(&stats)),
        ],
        [1, 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::LogGuide,
        &mut [
            (0, Read(&gradient)),
            (2, Read(&uniform)),
            (4, Write(guide.write())),
            (7, Read(&deviation)),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(guide)
}
pub(crate) fn apply(
    gpu: &Gpu,
    original: &ReadGuard,
    filtered: &ReadGuard,
    p: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [w, h] = p.extent;
    let groups = [w.div_ceil(8), h.div_ceil(8)];
    let tables = gpu.upload("Log reconstruction numerical tables", &p.tables)?;
    let uniform = gpu.uniform(&p.params(p.extent, 3, 0, 0., 0.));
    let mut power = image(gpu, "Power-domain reference RGB", p.extent, 3)?;
    let mut residual = image(gpu, "Log RGB residual", p.extent, 3)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::LogPrepare,
        &mut [
            (0, Read(original)),
            (1, Read(filtered)),
            (2, Read(&uniform)),
            (3, Read(&tables)),
            (4, Write(power.write())),
            (5, Write(residual.write())),
        ],
        groups,
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    let guide0 = edge(gpu, &power, p, p.controls[5], profile)?;
    // Recursive pairs run sequentially on the ordered queue. Each forward pass
    // overwrites every scratch element consumed by its backward pass.
    let mut recursive_scratch = image(gpu, "Recursive forward sums and weights", p.extent, 4)?;
    let temp = recursive(
        gpu,
        &power,
        &guide0,
        p.params([w, h], 3, 2, p.decay(0), 0.),
        None,
        &mut recursive_scratch,
        profile,
    )?;
    let smooth = recursive(
        gpu,
        &temp,
        &guide0,
        p.params([h, w], 3, 2, p.decay(0), 0.),
        Some(&power),
        &mut recursive_scratch,
        profile,
    )?;
    let guide1 = edge(gpu, &smooth, p, p.controls[6], profile)?;
    let mut guide = image(gpu, "Combined edge guidance", p.extent, 1)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::LogCombine,
        &mut [
            (0, Read(&guide0)),
            (1, Read(&guide1)),
            (2, Read(&uniform)),
            (4, Write(guide.write())),
        ],
        groups,
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    let temp = recursive(
        gpu,
        &residual,
        &guide,
        p.params([w, h], 3, 1, p.decay(2), 0.),
        None,
        &mut recursive_scratch,
        profile,
    )?;
    let residual = recursive(
        gpu,
        &temp,
        &guide,
        p.params([h, w], 3, 5, p.decay(2), 0.),
        None,
        &mut recursive_scratch,
        profile,
    )?;
    let temp = recursive(
        gpu,
        &guide,
        &guide,
        p.params([w, h], 1, 0, p.decay(1), 0.),
        None,
        &mut recursive_scratch,
        profile,
    )?;
    let guide = recursive(
        gpu,
        &temp,
        &guide,
        p.params([h, w], 1, 0, p.decay(1), 0.),
        None,
        &mut recursive_scratch,
        profile,
    )?;
    let mut output = image(gpu, "Log-residual reconstructed signed RGB", p.extent, 3)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::LogFinalize,
        &mut [
            (0, Read(original)),
            (1, Read(filtered)),
            (2, Read(&uniform)),
            (3, Read(&tables)),
            (4, Write(output.write())),
            (5, Read(&guide)),
            (6, Read(&residual)),
        ],
        groups,
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
#[cfg(test)]
#[path = "log_reconstruction_test.rs"]
mod tests;
