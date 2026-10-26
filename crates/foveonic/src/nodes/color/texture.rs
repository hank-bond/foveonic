//! Restores independently cleaned brightness and chroma in sensor tiles before color conversion.

use crate::{
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
const NATIVE_STEPS: [u32; 4] = [1, 2, 4, 8];
const FINE_BOUNDARY_STEP: usize = 1;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    // Fine, coarse, luminance cleanup, chromatic cleanup.
    strength: [f32; 4],
    steps: [f32; 4],
}
#[derive(Clone, Copy)]
pub(crate) struct Prepared {
    uniforms: Uniforms,
    spatial: bool,
    identity: bool,
}
impl Prepared {
    pub(crate) fn needs_base(self) -> bool {
        !self.identity
    }
    pub(crate) fn needs_ratio(self) -> bool {
        self.uniforms.strength[2] != 0.
    }
    pub(crate) fn new(controls: Controls, scale: f32) -> Result<Self> {
        controls.validate()?;
        let effects = controls.merrill_effects();
        let detail = effects.texture.unwrap_or_default();
        if !scale.is_finite() || !(0. ..=1.).contains(&scale) || scale == 0. {
            return Err("invalid texture sampling scale".into());
        }
        Ok(Self {
            spatial: effects.texture.is_some(),
            identity: !effects.changes_layers(),
            uniforms: Uniforms {
                strength: effects.strength,
                steps: std::array::from_fn(|i| {
                    let size = if i <= FINE_BOUNDARY_STEP {
                        detail.fine_size
                    } else {
                        detail.coarse_size
                    };
                    // Retain the existing reduced-grid default. Fractional taps let
                    // size controls move continuously between pixels.
                    (NATIVE_STEPS[i] as f32 * scale).round().max(1.) * size
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
    let unit = !prepared.spatial;
    let identity = prepared.identity;
    let restore = requested.layers && !identity;
    let mut result = Output {
        layers: (requested.layers && identity).then(|| raw.clone()),
        change: None,
    };
    // Neutral detail amounts cancel all bands. Neither restoration nor its
    // diagnostic needs the spatial pyramid in that common case.
    const PYRAMID_PASSES: u32 = 1 + 2 * NATIVE_STEPS.len() as u32;
    let dispatches =
        u32::from(restore) + u32::from(requested.change) + if unit { 0 } else { PYRAMID_PASSES };
    if dispatches == 0 {
        return Ok(result);
    }
    let mut trace = gpu.trace(profile.mode.gpu(), dispatches);
    let mut fine = ratio.clone();
    let mut coarse = ratio.clone();
    if !unit {
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
        for (index, step) in prepared.uniforms.steps.into_iter().enumerate() {
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
