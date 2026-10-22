//! Converts RGB/YUV and classifies radius-one chroma neighborhoods against a supplied noise model.
//!
//! Arithmetic and border contracts follow the reference implementation's
//! XDoRGB2YUV, XDoYUV2RGB and XDoUV_Filter callbacks.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    shape: [u32; 4],
    controls: [f32; 4],
    dark: [f32; 4],
}

pub(crate) struct Prepared {
    params: Params,
    model: Vec<[f32; 2]>,
    intensity: Vec<f32>,
}

pub(crate) struct Classification {
    pub(crate) yuv: ReadGuard,
    /// Signed sixteen-bit guide values widened to i32 storage.
    pub(crate) guide: ReadGuard,
    /// Classification zero, one or two, widened to u32 storage.
    pub(crate) mask: ReadGuard,
}

fn shape(extent: [u32; 2]) -> Result<[u32; 4]> {
    if extent.contains(&0) || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 3
    {
        return Err("invalid UV image extent".into());
    }
    Ok([extent[0], extent[1], 0, 0])
}

fn pair(value: f64) -> [f32; 2] {
    let high = value as f32;
    [high, (value - f64::from(high)) as f32]
}

impl Prepared {
    /// The model stores constant R/G/B, linear R/G/B and quadratic R/G/B.
    /// The supplied intensity table uses the reversed, floored camera-code index.
    pub(crate) fn new(
        extent: [u32; 2],
        model: [f64; 9],
        intensity: Vec<f32>,
        threshold: f32,
    ) -> Result<Self> {
        let shape = shape(extent)?;
        if model
            .iter()
            .any(|v| !v.is_finite() || !(*v as f32).is_finite())
            || model[0] < 0.
            || intensity.len() != 14385
            || intensity.iter().any(|v| !v.is_finite() || *v <= 0.)
            || !threshold.is_finite()
        {
            return Err("invalid UV noise model, intensity table or threshold".into());
        }
        let dark = pair(6. * model[0].sqrt());
        Ok(Self {
            params: Params {
                shape,
                controls: [threshold, 0., 0., 0.],
                dark: [dark[0], dark[1], 0., 0.],
            },
            model: model.map(pair).to_vec(),
            intensity,
        })
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params)
            .slice(&self.model)
            .slice(&self.intensity)
    }
}

/// Converts unclamped floating RGB or YUV codes using fixed numerical coefficients.
pub(crate) fn convert(
    gpu: &Gpu,
    input: &ReadGuard,
    extent: [u32; 2],
    inverse: bool,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let mut shape = shape(extent)?;
    shape[2] = u32::from(inverse);
    let bytes = u64::from(extent[0]) * u64::from(extent[1]) * 12;
    if input.size() != bytes {
        return Err("UV conversion input size does not match its extent".into());
    }
    let matrix: [f64; 9] = if inverse {
        [1., 0., 1.13983, 1., -0.39465, -0.5806, 1., 2.03211, 0.]
    } else {
        [
            0.299, 0.587, 0.114, -0.14713, -0.28886, 0.436, 0.615, -0.51499, -0.10001,
        ]
    };
    let matrix = gpu.upload("RGB/YUV double coefficients", &matrix.map(pair))?;
    let uniform = gpu.uniform(&Params {
        shape,
        controls: [0.; 4],
        dark: [0.; 4],
    });
    let mut output = gpu.storage("RGB/YUV conversion", bytes)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::UvConvert,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&matrix)),
            (3, Write(output.write())),
        ],
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

/// Preserves Y and all borders; accepted interior pixels replace U/V by box means.
/// Classification and guide storage are initialized to zero on the borders.
pub(crate) fn apply(
    gpu: &Gpu,
    rgb: &ReadGuard,
    yuv: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<Classification> {
    let extent = prepared.extent();
    let pixels = u64::from(extent[0]) * u64::from(extent[1]);
    if rgb.size() != pixels * 12 || yuv.size() != pixels * 12 {
        return Err("UV classifier input size does not match its extent".into());
    }
    let uniform = gpu.uniform(&prepared.params);
    let model = gpu.upload("UV quadratic model", &prepared.model)?;
    let intensity = gpu.upload("UV intensity conditioning", &prepared.intensity)?;
    let mut result = Classification {
        yuv: gpu.storage("Classified YUV", pixels * 12)?,
        guide: gpu.storage("UV signed guide", pixels * 4)?,
        mask: gpu.storage("UV classes", pixels * 4)?,
    };
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::UvClassify,
        &mut [
            (0, Read(rgb)),
            (1, Read(yuv)),
            (2, Read(&uniform)),
            (3, Read(&model)),
            (4, Read(&intensity)),
            (5, Write(result.yuv.write())),
            (6, Write(result.guide.write())),
            (7, Write(result.mask.write())),
        ],
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(result)
}

#[cfg(test)]
#[path = "uv_repair_test.rs"]
mod tests;
