//! Makes linear camera samples on the GPU so DNG retains room for later edits.

use super::Engine;
use crate::{
    dng::appearance::Prepared,
    gpu::{Binding, Kernel},
    image::{Layout, RenderedImage, Space},
    profile::Profile,
    source::Result,
};
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Basis {
    rows: [[f32; 4]; 3],
    neutral: [f32; 4],
}
#[cfg(test)]
#[path = "gpu_test.rs"]
mod tests;

impl Engine {
    pub(crate) fn encode_dng_base(
        &self,
        image: &RenderedImage,
        prepared: &Prepared,
        profile: &mut Profile,
    ) -> Result<(Vec<u8>, f32)> {
        self.check_owner(image)?;
        if image.description().layout != Layout::Rgb32Float
            || !matches!(image.description().space, Space::LinearRgb(_))
        {
            return Err("DNG requires a linear technical base".into());
        }
        let params = image.params();
        let [w, h, _, _] = params.output;
        let size = u64::from(w) * u64::from(h);
        let mut camera = self.gpu.storage("DNG camera coordinates", size * 12)?;
        let mut stats = self.gpu.storage("DNG needed range", 8)?;
        let mut packed = self.gpu.storage("DNG uint16 samples", size * 8)?;
        let geometry = self.gpu.uniform(&params);
        let mut basis = Basis {
            rows: std::array::from_fn(|r| {
                [
                    prepared.mapping.0[r * 3] as f32,
                    prepared.mapping.0[r * 3 + 1] as f32,
                    prepared.mapping.0[r * 3 + 2] as f32,
                    0.,
                ]
            }),
            neutral: [
                prepared.neutral[0] as f32,
                prepared.neutral[1] as f32,
                prepared.neutral[2] as f32,
                1.,
            ],
        };
        if basis
            .rows
            .iter()
            .flatten()
            .chain(&basis.neutral)
            .any(|v| !v.is_finite())
            || basis.neutral[..3].iter().any(|v| *v <= 0.)
        {
            return Err("unrepresentable DNG basis".into());
        }
        let uniform = self.gpu.uniform(&basis);
        let mut trace = self.gpu.trace(profile.mode.gpu(), 2);
        self.gpu.dispatch(
            Kernel::DngConvert,
            &mut [
                (0, Binding::Read(image.buffer())),
                (1, Binding::Read(&geometry)),
                (2, Binding::Write(camera.write())),
                (3, Binding::Write(stats.write())),
                (4, Binding::Read(&uniform)),
            ],
            [w.div_ceil(8), h.div_ceil(8)],
            &mut trace,
        );
        let statistics = self.gpu.readback(&stats)?;
        let words = bytemuck::cast_slice::<u8, u32>(&statistics);
        let scale = f32::from_bits(words[0]).max(1.) * 1.001;
        if words[1] != 0 || !scale.is_finite() {
            return Err("non-finite DNG camera samples or range".into());
        }
        basis.neutral[3] = scale;
        let uniform = self.gpu.uniform(&basis);
        self.gpu.dispatch(
            Kernel::DngQuantize,
            &mut [
                (1, Binding::Read(&geometry)),
                (2, Binding::Read(&camera)),
                (4, Binding::Read(&uniform)),
                (5, Binding::Write(packed.write())),
            ],
            [w.div_ceil(8), h.div_ceil(8)],
            &mut trace,
        );
        self.gpu.finish_trace(&trace, profile)?;
        Ok((self.gpu.readback(&packed)?, scale))
    }
}
