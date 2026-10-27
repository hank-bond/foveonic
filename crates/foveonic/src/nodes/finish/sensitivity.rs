//! Applies one stabilized hue-guided brightness response to linear color or monochrome samples.

use crate::{
    color_space::{ColorSpace, TiffColorSpace},
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

/// Native-pixel support of the separable brightness-gain filter.
pub(crate) const NATIVE_RADIUS: u32 = 8;

#[cfg(test)]
#[path = "sensitivity_test.rs"]
mod tests;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Params {
    /// Native dimensions, filter radius and pass axis.
    pub shape: [u32; 4],
    /// Nominal linear code white and reserved components.
    pub values: [f32; 4],
    pub hues: [[f32; 4]; 2],
    pub luma: [f32; 4],
    pub guide_matrix: [[f32; 4]; 3],
}
impl Params {
    pub fn new(
        extent: [u32; 2],
        controls: [f32; 6],
        space: ColorSpace,
        white: f32,
        scale: f32,
    ) -> Self {
        // D50 colorants of the researched linear reference working profile.
        // The profile's Adobe name alone does not identify these exact colorants.
        const REFERENCE_COLORANTS: [f64; 9] = [
            42294., 11865., 9030., 21808., 40115., 3611., 1982., 4520., 47557.,
        ];
        const ICC_FIXED_SCALE: f64 = (1_u32 << 16) as f64;
        let inverse = crate::color_space::inverse(REFERENCE_COLORANTS.map(|v| v / ICC_FIXED_SCALE));
        let source = TiffColorSpace::from(space).to_xyz();
        let guide_matrix = std::array::from_fn(|r| {
            std::array::from_fn(|c| {
                if c < 3 {
                    (0..3)
                        .map(|k| inverse[r * 3 + k] * source[k * 3 + c])
                        .sum::<f64>() as f32
                } else {
                    0.
                }
            })
        });
        let mut hues = [[0.; 4]; 2];
        for (i, v) in controls.into_iter().enumerate() {
            hues[i / 4][i % 4] = v;
        }
        Self {
            shape: [
                extent[0],
                extent[1],
                (NATIVE_RADIUS as f32 * scale).ceil().max(1.) as u32,
                0,
            ],
            values: [white, 0., 0., 0.],
            hues,
            luma: crate::nodes::finish::photographic::luminance(space),
            guide_matrix,
        }
    }
    pub fn key(self) -> Key {
        Key::pod(&self)
    }
}

fn pass(
    gpu: &Gpu,
    kernel: Kernel,
    input: &ReadGuard,
    auxiliary: Option<&ReadGuard>,
    params: Params,
    rgb: bool,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let extent = [params.shape[0], params.shape[1]];
    let pixels = u64::from(extent[0]) * u64::from(extent[1]);
    let mut output = gpu.storage(kernel.label(), pixels * if rgb { 12 } else { 4 })?;
    let uniform = gpu.uniform(&params);
    let mut bindings = vec![
        (0, Read(input)),
        (1, Read(&uniform)),
        (2, Write(output.write())),
    ];
    if let Some(auxiliary) = auxiliary {
        bindings.push((3, Read(auxiliary)));
    }
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        kernel,
        &mut bindings,
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    guide: &ReadGuard,
    mut p: Params,
    extent: [u32; 2],
    profile: &mut Profile,
) -> Result<ReadGuard> {
    if p.hues == [[0.; 4]; 2] {
        return Ok(input.clone());
    }
    p.shape[..2].copy_from_slice(&extent);
    let bytes = u64::from(extent[0]) * u64::from(extent[1]) * 12;
    if input.size() != bytes || guide.size() != bytes {
        return Err("color sensitivity needs matching image and clean color guide".into());
    }
    let mut gain = pass(
        gpu,
        Kernel::ColorSensitivityMap,
        guide,
        None,
        p,
        false,
        profile,
    )?;
    for axis in 0..2 {
        p.shape[3] = axis;
        gain = pass(
            gpu,
            Kernel::ColorSensitivityFilter,
            &gain,
            Some(guide),
            p,
            false,
            profile,
        )?;
    }
    pass(
        gpu,
        Kernel::ColorSensitivityApply,
        input,
        Some(&gain),
        p,
        true,
        profile,
    )
}
