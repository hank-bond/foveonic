//! Maps signed linear codes into root space and extracts detail with a signed separable filter.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    source::Result,
};
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Params {
    /// Width, height, horizontal-first flag, reserved.
    pub shape: [u32; 4],
    /// Visible left/top inclusive and right/bottom exclusive sensor coordinates.
    pub crop: [u32; 4],
    /// Q12 center, first-neighbor and second-neighbor taps, then Q14 green weight.
    pub filter: [i32; 4],
}
#[derive(Clone)]
pub struct Prepared {
    pub params: Params,
    pub tables: Vec<i32>,
}
impl Prepared {
    /// Buffers retain sensor geometry. Detail borders clamp to the visible crop.
    pub(crate) fn new(
        extent: [u32; 2],
        crop: [u32; 4],
        green_weight: f64,
        green_limit: i32,
        scale: f64,
    ) -> Result<Self> {
        if extent.contains(&0)
            || crop[0] >= crop[2]
            || crop[1] >= crop[3]
            || crop[2] > extent[0]
            || crop[3] > extent[1]
            || !green_weight.is_finite()
            || !scale.is_finite()
            || scale <= 0.
            || !(0..=32767).contains(&green_limit)
        {
            return Err("invalid root/detail geometry or controls".into());
        }
        let t = (scale * 0.25).min(0.5);
        // The symmetric five-tap kernel sums to 4096 (Q12 unity).
        let q2 = (t * t * 4096. + 0.5) as i32;
        let q1 = ((t - 2. * t * t) * 2. * 4096. + 0.5) as i32;
        let mut tables = super::signed_response::root();
        // Append 1000 Float32 bit patterns after the 65536 signed-code root entries.
        // The reference green-correction ramp has midpoint code 160 and scale 40.
        let half_limit = green_limit / 2;
        tables.extend(
            (0..1000)
                .map(|i| (((i as f64 / 40. - 4.).tanh() + 1.) * f64::from(half_limit)) as f32)
                .map(|v| v.to_bits() as i32),
        );
        Ok(Self {
            params: Params {
                shape: [extent[0], extent[1], 0, 0],
                crop,
                filter: [4096 - 2 * (q1 + q2), q1, q2, (green_weight * 16384.) as i32],
            },
            tables,
        })
    }
    /// Select horizontal then vertical rounding while retaining sensor coordinates.
    /// A recipe can use this when presentation transposes the image axes.
    pub(crate) fn with_horizontal_first(mut self) -> Self {
        self.params.shape[2] = 1;
        self
    }
    pub(crate) fn key(&self) -> crate::recipe::Key {
        crate::recipe::Key::pod(&self.params).slice(&self.tables)
    }
}
pub(crate) struct Output {
    pub rgb: ReadGuard,
    pub detail: ReadGuard,
}
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    p: &Prepared,
    profile: &mut Profile,
) -> Result<Output> {
    let [w, h, _, _] = p.params.shape;
    let pixels = u64::from(w) * u64::from(h);
    let mut rgb = gpu.storage("Root-space signed RGB", pixels * 12)?;
    let mut mono = gpu.storage("Root-space weighted mono", pixels * 4)?;
    let mut vertical = gpu.storage("Signed detail first pass", pixels * 4)?;
    let mut detail = gpu.storage("Signed detail", pixels * 4)?;
    let uniform = gpu.uniform(&p.params);
    let tables = gpu.upload("Root and green response tables", &p.tables)?;
    let groups = [w.div_ceil(8), h.div_ceil(8)];
    let mut trace = gpu.trace(profile.mode.gpu(), 3);
    gpu.dispatch(
        Kernel::CodeRoot,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&tables)),
            (3, Write(rgb.write())),
            (4, Write(mono.write())),
        ],
        groups,
        &mut trace,
    );
    gpu.dispatch(
        Kernel::CodeDetailVertical,
        &mut [
            (1, Read(&uniform)),
            (4, Read(&mono)),
            (5, Write(vertical.write())),
        ],
        groups,
        &mut trace,
    );
    gpu.dispatch(
        Kernel::CodeDetailHorizontal,
        &mut [
            (1, Read(&uniform)),
            (4, Read(&mono)),
            (5, Read(&vertical)),
            (6, Write(detail.write())),
        ],
        groups,
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(Output { rgb, detail })
}
#[cfg(test)]
#[path = "root_detail_test.rs"]
mod tests;
