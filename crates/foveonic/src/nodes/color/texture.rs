//! Restores independently cleaned brightness and chroma in sensor tiles before color conversion.

use crate::{
    color_space::{ColorSpace, TiffColorSpace},
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    noise::{Controls, Parameters},
    profile::Profile,
    recipe::Key,
    source::Result,
};

// Two binomial steps define the fine/coarse boundary. Two more separate
// broad shading, which is always restored. Size controls can double the
// native 30-pixel support, staying inside the existing 64-pixel tile context.
pub(crate) const NATIVE_STEPS: [u32; 4] = [1, 2, 4, 8];
pub(crate) const FINE_BOUNDARY_STEP: usize = 1;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    // Fine, coarse, luminance cleanup, chromatic cleanup.
    strength: [f32; 4],
    steps: [f32; 4],
    color: Color,
}
/// Sensor coordinates for neutral restored grain and true linear RGB luminance.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Color {
    active: [u32; 4],
    rgb_y: [f32; 4],
    neutral_y: [f32; 4],
}
impl Color {
    pub(crate) fn new(matrix: [[f32; 4]; 3], space: ColorSpace, active: [u32; 4]) -> Result<Self> {
        let display = TiffColorSpace::Srgb.transform(space);
        let combined: [[f32; 3]; 3] = std::array::from_fn(|r| {
            std::array::from_fn(|c| (0..3).map(|k| display[r][k] * matrix[k][c]).sum())
        });
        let rgb_y = std::array::from_fn(|c| {
            if c == 3 {
                0.
            } else {
                (0..3)
                    .map(|r| [0.2126, 0.7152, 0.0722][r] * combined[r][c])
                    .sum()
            }
        });
        let inverse =
            crate::color_space::inverse(std::array::from_fn(|i| f64::from(combined[i / 3][i % 3])));
        let mut neutral_y: [f32; 4] = std::array::from_fn(|c| {
            if c == 3 {
                0.
            } else {
                inverse[c * 3..c * 3 + 3].iter().sum::<f64>() as f32
            }
        });
        let y: f32 = (0..3).map(|c| neutral_y[c] * rgb_y[c]).sum();
        if active[0] >= active[2]
            || active[1] >= active[3]
            || !y.is_finite()
            || y <= 0.
            || rgb_y.iter().any(|v| !v.is_finite())
            || neutral_y[..3].iter().any(|v| !v.is_finite() || *v <= 0.)
        {
            return Err("invalid texture color coordinates".into());
        }
        neutral_y.iter_mut().for_each(|v| *v /= y);
        Ok(Self {
            active,
            rgb_y,
            neutral_y,
        })
    }
}
#[cfg(test)]
impl Default for Color {
    fn default() -> Self {
        Self::new(
            [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]],
            ColorSpace::Srgb,
            [0, 0, u32::MAX, u32::MAX],
        )
        .unwrap()
    }
}
#[derive(Clone, Copy)]
pub(crate) struct Prepared {
    uniforms: Uniforms,
    blur_step_count: usize,
    identity: bool,
}
impl Prepared {
    /// The neutral spatial controls skip the pyramid. Uniforms are still
    /// allocated when the identity branch returns the input unchanged.
    pub(crate) fn allocation(
        self,
        params: Parameters,
        requested: Requested,
    ) -> Option<crate::render::allocation::GpuAllocation> {
        use crate::render::allocation::{image_bytes, uploaded_bytes, GpuAllocation};
        if self.blur_step_count != 0 || !requested.layers || requested.change {
            return None;
        }
        let extent = super::reconstruction::whole_atlas_extent(params)?;
        // Input-size validation runs before the identity return in apply.
        let bytes = image_bytes(extent, 3)?;
        GpuAllocation::new(
            if self.identity { 0 } else { bytes },
            uploaded_bytes::<Parameters>(1)?.checked_add(uploaded_bytes::<Uniforms>(1)?)?,
        )
    }

    pub(crate) fn needs_base(self) -> bool {
        !self.identity
    }
    pub(crate) fn needs_ratio(self) -> bool {
        self.uniforms.strength[2] != 0.
    }
    pub(crate) fn new(controls: Controls, scale: f32, color: Color) -> Result<Self> {
        controls.validate()?;
        let effects = controls.merrill_effects();
        let detail = effects.texture.unwrap_or_default();
        if !scale.is_finite() || !(0. ..=1.).contains(&scale) || scale == 0. {
            return Err("invalid texture sampling scale".into());
        }
        Ok(Self {
            blur_step_count: match effects.texture {
                None => 0,
                // At unit Coarse Amount the broad blur cancels in the mix.
                Some(t) if t.coarse == 1. => FINE_BOUNDARY_STEP + 1,
                Some(_) => NATIVE_STEPS.len(),
            },
            identity: !effects.changes_layers(),
            uniforms: Uniforms {
                strength: effects.strength,
                color,
                steps: std::array::from_fn(|i| {
                    let size = if i <= FINE_BOUNDARY_STEP {
                        detail.fine_size
                    } else {
                        detail.coarse_size
                    };
                    // Fractional taps preserve support in native sensor pixels,
                    // including bands narrower than one working-grid cell.
                    NATIVE_STEPS[i] as f32 * scale * size
                }),
            },
        })
    }
    pub(crate) fn key(self) -> Key {
        Key::pod(&self.uniforms)
    }
}

pub(crate) struct Inputs<'a> {
    pub raw: &'a ReadGuard,
    pub base: Option<&'a ReadGuard>,
    pub ratio: Option<&'a ReadGuard>,
}
#[derive(Default)]
pub(crate) struct Requested {
    pub layers: bool,
    pub change: bool,
}
pub(crate) struct Output {
    pub layers: Option<ReadGuard>,
    /// Multiplicative luminance maps. One is neutral, as for other ratio views.
    pub change: Option<ReadGuard>,
}

pub(crate) fn apply(
    gpu: &Gpu,
    inputs: Inputs<'_>,
    params: Parameters,
    prepared: Prepared,
    requested: Requested,
    profile: &mut Profile,
) -> Result<Output> {
    let Inputs { raw, base, ratio } = inputs;
    let bytes = params.atlas_pixels() * 4;
    if raw.size() != bytes * 3
        || base.is_some_and(|v| v.size() != bytes * 3)
        || ratio.is_some_and(|v| v.size() != bytes)
        || (prepared.needs_base() && base.is_none())
        || (prepared.needs_ratio() && ratio.is_none())
    {
        return Err("texture restoration requires matching processing tiles".into());
    }
    // Inactive shader branches never read these bindings. Reuse an existing
    // allocation for their required binding slots rather than allocating dummies.
    let base = base.unwrap_or(raw);
    let ratio = ratio.unwrap_or(raw);
    if !requested.layers && !requested.change {
        return Err("no texture output requested".into());
    }
    let groups = [
        params.tile[0].div_ceil(8),
        (params.tile[1] * params.tile_count()).div_ceil(8),
    ];
    let uniform = gpu.uniform(&params);
    let controls = gpu.uniform(&prepared.uniforms);
    let steps = &prepared.uniforms.steps[..prepared.blur_step_count];
    let identity = prepared.identity;
    let restore = requested.layers && !identity;
    let mut result = Output {
        layers: (requested.layers && identity).then(|| raw.clone()),
        change: None,
    };
    let pyramid_passes = if steps.is_empty() {
        0
    } else {
        1 + 2 * steps.len() as u32
    };
    let dispatches = u32::from(restore) + u32::from(requested.change) + pyramid_passes;
    if dispatches == 0 {
        return Ok(result);
    }
    let mut trace = gpu.trace(profile.mode.gpu(), dispatches);
    let mut fine = ratio.clone();
    let mut coarse = ratio.clone();
    if !steps.is_empty() {
        let mut log = gpu.storage("Texture luminance residual", bytes)?;
        gpu.dispatch(
            Kernel::TextureExtract,
            &mut [
                (0, Read(raw)),
                (1, Read(&uniform)),
                (2, Write(log.write())),
                (3, Read(base)),
                (4, Read(ratio)),
                (7, Read(&controls)),
            ],
            groups,
            &mut trace,
        );
        let mut current = log;
        for (index, &step) in steps.iter().enumerate() {
            for axis in 0..2 {
                let pass = gpu.uniform(&Uniforms {
                    steps: [step, axis as f32, 0., 0.],
                    ..prepared.uniforms
                });
                let mut output = gpu.storage("Texture binomial scale", bytes)?;
                gpu.dispatch(
                    Kernel::TextureBlur,
                    &mut [
                        (0, Read(&current)),
                        (1, Read(&uniform)),
                        (2, Write(output.write())),
                        (7, Read(&pass)),
                    ],
                    groups,
                    &mut trace,
                );
                current = output;
            }
            if index == FINE_BOUNDARY_STEP {
                fine = current.clone();
            }
        }
        coarse = current;
    }
    if restore {
        let mut layers = gpu.storage("Restored sensor texture", bytes * 3)?;
        gpu.dispatch(
            Kernel::TextureMix,
            &mut [
                (0, Read(raw)),
                (1, Read(&uniform)),
                (2, Write(layers.write())),
                (3, Read(base)),
                (4, Read(ratio)),
                (5, Read(&fine)),
                (6, Read(&coarse)),
                (7, Read(&controls)),
            ],
            groups,
            &mut trace,
        );
        result.layers = Some(layers);
    }
    let change_tiles = if requested.change {
        let mut change = gpu.storage("Texture luminance change", bytes)?;
        gpu.dispatch(
            Kernel::TextureInspect,
            &mut [
                (0, Read(raw)),
                (1, Read(&uniform)),
                (2, Write(change.write())),
                (3, Read(base)),
                (4, Read(ratio)),
                (5, Read(&fine)),
                (6, Read(&coarse)),
                (7, Read(&controls)),
            ],
            groups,
            &mut trace,
        );
        Some(change)
    } else {
        None
    };
    gpu.finish_trace(&trace, profile)?;
    if let Some(tiles) = change_tiles {
        result.change = Some(super::reconstruction::project(
            gpu, &tiles, params, 1, profile,
        )?);
    }
    Ok(result)
}

#[cfg(test)]
#[path = "texture_test.rs"]
mod tests;
