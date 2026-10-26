//! Prepares the fixed Merrill noise base, column conditioning, and camera-derived processing couplings.
//!
//! Adapted from the reference implementation's noise.rs and params.rs at e47af35 with owner permission.

#[cfg(test)]
#[path = "noise_test.rs"]
mod tests;

use crate::source::{MatrixEncoding, Metadata, Result};

#[path = "noise_controls.rs"]
mod controls;
pub use controls::{Controls, TextureControls};

fn mean_decay(preset: f32, scale: f32) -> f32 {
    // The camera's reduced local filtering needs this ceiling to limit seams.
    const REDUCED_LOCAL_MAX_DECAY: f32 = 0.92;
    if scale > 0.5 && scale < 1. {
        preset.min(REDUCED_LOCAL_MAX_DECAY)
    } else {
        preset
    }
}

/// Uniform layout shared by the reference-style shader passes.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Parameters {
    pub source: [u32; 4], // Full sensor width/height and processing tile columns/rows.
    pub tile: [u32; 4],   // Tile width/height, context, bilateral window.
    pub area: [u32; 4],   // Half-open working rectangle.
    pub filter: [f32; 4], // Range scale, source blend, edge strength, mean decay.
    pub edge: [f32; 4],   // Edge scale, linear shift, ratio epsilon, effective ISO.
    pub weights: [f32; 4],
    pub model: [[f32; 4]; 3], // Constant, linear, and quadratic variance by sensor layer.
    pub mono: [f32; 4],
    pub diffusion: [f32; 4], // Step, noise-model step, fixed contrast, directional axis.
    pub column_area: [u32; 4], // Camera column bounds, not the shading or working-area bounds.
    pub column: [f32; 4],    // Column range coefficient, horizontal window, and reserved values.
    pub reduction: [f32; 4], // Native eighth-quantized reconstruction scale.
    pub geometry: [u32; 4], // LocalMean margin, edge LUT margin, fixed bilateral size, global branch.
    pub window: [u32; 4],   // First native tile column/row and compact atlas columns/rows.
    pub region: [u32; 4],   // Half-open physical rectangle projected from the compact atlas.
}
impl Parameters {
    pub fn atlas_extent(self) -> [u32; 2] {
        [self.tile[0], self.tile[1] * self.tile_count()]
    }
    /// Native low-scale filtering uses whole-image context rather than independent tiles.
    pub fn uses_global_context(self) -> bool {
        const GLOBAL_CONTEXT_MAX_SCALE: f32 = 0.5;
        self.reduction[0] <= GLOBAL_CONTEXT_MAX_SCALE
    }
    pub fn tile_count(self) -> u32 {
        self.window[2] * self.window[3]
    }
    pub fn reduced_tile_extent(self) -> Result<[u32; 2]> {
        let scale = self.reduction[0];
        if !(0.375..=1.).contains(&scale) {
            return Err("unsupported reconstruction scale".into());
        }
        let extent = [self.tile[0], self.tile[1]].map(|v| (v as f32 * scale) as u32);
        if self.tile[..2]
            .iter()
            .zip(extent)
            .any(|(&native, reduced)| reduced == 0 || native as f32 * scale != reduced as f32)
        {
            return Err("nonintegral reduced tile geometry".into());
        }
        Ok(extent)
    }
    pub fn output_extent(self) -> [u32; 2] {
        [
            self.region[2] - self.region[0],
            self.region[3] - self.region[1],
        ]
    }
    pub fn full_frame(mut self) -> Self {
        self.window = [0, 0, self.source[2], self.source[3]];
        self.region = [0, 0, self.source[0], self.source[1]];
        self
    }
    /// Keep native tile phases and include both contributors to eight-pixel seams.
    pub fn for_region(mut self, region: [u32; 4]) -> Result<Self> {
        // Match the asymmetric eight-pixel blend in noise_stitch.wgsl.
        const SEAM_SUPPORT: u32 = 5;
        if self.source[2..].contains(&0)
            || (0..2).any(|axis| {
                self.source[axis] == 0 || !self.source[axis].is_multiple_of(self.source[axis + 2])
            })
        {
            return Err("invalid processing tile grid".into());
        }
        if region[0] >= region[2]
            || region[1] >= region[3]
            || region[2] > self.source[0]
            || region[3] > self.source[1]
        {
            return Err("invalid processing tile region".into());
        }
        for axis in 0..2 {
            let core = self.source[axis] / self.source[axis + 2];
            let first = region[axis].saturating_sub(SEAM_SUPPORT) / core;
            let last = region[axis + 2]
                .saturating_add(SEAM_SUPPORT)
                .div_ceil(core)
                .min(self.source[axis + 2]);
            self.window[axis] = first;
            self.window[axis + 2] = last - first;
        }
        self.region = region;
        Ok(self)
    }
    pub fn atlas_pixels(self) -> u64 {
        let [w, h] = self.atlas_extent();
        u64::from(w) * u64::from(h)
    }
}

/// Key only the fields each stage uses so texture edits can reuse filtering.
#[derive(Clone, Copy)]
pub(crate) enum RecipeStage {
    Base,
    Column,
    Ratio,
    Geometry,
    Texture,
}
impl Parameters {
    pub(crate) fn key(self, stage: RecipeStage) -> crate::recipe::Key {
        let key = crate::recipe::Key::pod(&self.source)
            .slice(&self.tile[..3])
            .with(&self.window);
        match stage {
            RecipeStage::Base => key
                .with(&self.tile[3])
                .with(&self.area)
                .with(&self.filter)
                .with(&[self.edge[0], self.edge[1], self.edge[3]])
                .with(&self.model)
                .with(&self.reduction)
                .with(&self.geometry),
            RecipeStage::Column => key
                .with(&self.area)
                .with(&self.model)
                .with(&self.column_area)
                .with(&self.column),
            RecipeStage::Ratio => key
                .slice(&self.weights[..3])
                .with(&self.mono)
                .slice(&self.diffusion[..3])
                .with(&self.edge[2]),
            RecipeStage::Texture => key.slice(&self.weights[..3]).with(&self.edge[2]),
            RecipeStage::Geometry => key,
        }
    }
}

pub(crate) fn column_parameters(metadata: &Metadata, mut p: Parameters) -> Result<Parameters> {
    let dark = metadata.fixed::<4>("DarkShieldTop")?;
    let active = crate::nodes::sensor::calibration::rectangle(metadata, "ActiveImageArea")?;
    if [dark[0], dark[2]]
        .iter()
        .any(|v| !v.is_finite() || v.fract() != 0. || !(0. ..u32::MAX as f64).contains(v))
        || active[1] >= active[3]
        || active[3] > p.source[1]
        || active[1] >= p.tile[1]
        || p.source[1] - active[3] >= p.tile[1]
    {
        return Err("invalid noise column rectangle".into());
    }
    p.column_area = [dark[0] as u32, active[1], dark[2] as u32 + 1, active[3]];
    Ok(p)
}

pub(crate) fn pattern_parameters(
    metadata: &Metadata,
    p: Parameters,
) -> Result<super::pattern::Prepared> {
    const EXTRA: [[f32; 13]; 7] = [
        [
            100., 0.025, 0.025, 0., 0.12, 0.0025, 3., 0., 0., 0., 0., 0., 0.,
        ],
        [
            400., 0.05, 0.05, 0., 0.12, 0.0025, 3., 0., 0., 0., 0., 0., 0.,
        ],
        [
            800., 0.075, 0.075, 0.25, 0.12, 0.0025, 3., 0., 0., 0., 0., 0., 0.,
        ],
        [
            1600., 0.1, 0.1, 0.5, 0.12, 0.0025, 3., 0., 0., 0., 0., 0., 0.,
        ],
        [
            3200., 0., 0., 0.75, 0.12, 0.0025, 3., 0., 0., 0., 0., 0., 0.,
        ],
        [6400., 0., 0., 1., 0.12, 0.0025, 3., 0., 0., 0., 0., 0., 0.],
        [12800., 0., 0., 1., 0.12, 0.0025, 3., 0., 0., 0., 0., 0., 0.],
    ];
    let rows =
        table::<13>(metadata, "AdditionalISONoiseSettings")?.unwrap_or_else(|| EXTRA.to_vec());
    if !(2..20).contains(&rows.len())
        || rows.windows(2).any(|r| r[0][0] >= r[1][0])
        || rows[0][0] <= 0.
    {
        return Err("invalid additional ISO noise knots".into());
    }
    let last = rows.last().unwrap()[0];
    let estimated = p.edge[3].clamp(rows[0][0], last);
    let capture = scalar(metadata, "CaptureISO", 0., MatrixEncoding::Float32)?;
    let capture = if capture > 0. {
        capture
    } else {
        metadata
            .properties
            .get("ISO")
            .ok_or("missing pattern capture ISO")?
            .parse::<f32>()?
    };
    if !capture.is_finite() || capture <= 0. {
        return Err("invalid pattern capture ISO".into());
    }
    let capture = capture.clamp(rows[0][0], last);
    let high = rows
        .iter()
        .position(|row| estimated <= row[0])
        .unwrap()
        .max(1);
    let weight = (capture - rows[high - 1][0]) / (rows[high][0] - rows[high - 1][0]);
    let extra: [f32; 13] =
        std::array::from_fn(|c| (1. - weight).mul_add(rows[high - 1][c], weight * rows[high][c]));
    let half = metadata
        .matrix("PermSizeHalf")
        .filter(|m| m.dimensions == [1])
        .map(|m| m.values.first().copied().unwrap_or(f64::NAN))
        .unwrap_or(4.);
    if !half.is_finite() || half.fract() != 0. || !(0. ..=4.).contains(&half) {
        return Err("unsupported pattern half size".into());
    }
    super::pattern::Prepared::new(
        [p.tile[0], p.tile[1], p.tile_count(), half as u32],
        [extra[1], extra[2], extra[3].clamp(0., 1.), 0.],
    )
}

const HI_COLOR: [[f32; 13]; 7] = [
    [100., 1., 1., 10., 0.025, 0.7, 0.5, 4., 1., 1., 0., 1.5, 1.],
    [400., 1., 0.75, 10., 0.1, 0.7, 0.9, 4., 1., 1., 0., 1.5, 1.],
    [800., 1., 0.5, 10., 0.1, 0.7, 1., 4., 1., 1., 0., 1.5, 1.],
    [
        1600., 1., 0.375, 10., 0.25, 0.7, 1., 4., 1., 1., 0.25, 1.5, 1.,
    ],
    [3200., 1., 0.5, 10., 0.5, 0.7, 1., 4., 1., 1., 0.35, 1.5, 1.],
    [
        6400., 1., 0.375, 10., 0.5, 0.7, 1., 4., 1., 1., 0.6, 1.5, 1.,
    ],
    [12800., 1., 0.25, 10., 0.1, 0.7, 1., 4., 1., 1., 1., 1.5, 1.],
];
const SLIDERS: [[f32; 13]; 5] = [
    [0.25, 1., 1., 1., 10., 1.2, 0.8, 1., 0.5, 0.25, 1., 1., 1.],
    [0.5, 1., 1., 1., 5., 1.1, 0.9, 1., 0.75, 0.5, 1., 1., 1.],
    [1.; 13],
    [2., 1., 1., 1., 0.2, 0.9, 1.1, 1., 1.25, 1., 0.25, 1., 1.],
    [4., 1., 1., 1., 0.1, 0.8, 1.2, 1., 1.5, 1., 0.125, 1., 1.],
];

fn scalar(metadata: &Metadata, name: &str, default: f32, encoding: MatrixEncoding) -> Result<f32> {
    match metadata
        .matrix(name)
        .filter(|m| m.encoding == encoding && m.dimensions == [1])
    {
        Some(m) if m.values.len() == 1 && m.values[0].is_finite() => Ok(m.values[0] as f32),
        Some(_) => Err(format!("invalid noise scalar {name}").into()),
        None => Ok(default),
    }
}
fn table<const N: usize>(metadata: &Metadata, name: &str) -> Result<Option<Vec<[f32; N]>>> {
    let Some(m) = metadata
        .matrix(name)
        .filter(|m| m.encoding == MatrixEncoding::Float32 && m.dimensions.len() == 2)
    else {
        return Ok(None);
    };
    if m.dimensions[1] != N
        || m.dimensions[0] * N != m.values.len()
        || m.values.iter().any(|v| !v.is_finite())
    {
        return Err(format!("invalid noise table {name}").into());
    }
    Ok(Some(
        m.values
            .as_chunks::<N>()
            .0
            .iter()
            .map(|r| r.map(|v| v as f32))
            .collect(),
    ))
}

pub(crate) fn processing_gain(metadata: &Metadata) -> Result<(f32, f32)> {
    let f = |key, default| scalar(metadata, key, default, MatrixEncoding::Float32);
    let capture = f("CaptureISO", 0.)?;
    let capture = if capture > 0. {
        capture
    } else {
        metadata
            .properties
            .get("ISO")
            .ok_or("missing capture ISO")?
            .parse::<f32>()?
    };
    let extended = scalar(metadata, "ISOrange", 0., MatrixEncoding::Uint16)? != 0.;
    let processing = if extended {
        match capture {
            800. => 666.,
            1600. => 1200.,
            3200. => 2133.,
            6400. => 4266.,
            _ => capture,
        }
    } else {
        capture
    };
    let sensor = f("SensorISO", 100.)?;
    let gain = processing / if sensor == 0. { 100. } else { sensor };
    if !capture.is_finite() || capture <= 0. || !gain.is_finite() || gain <= 0. {
        return Err("invalid processing ISO gain".into());
    }
    Ok((capture, gain))
}
fn effective_iso(metadata: &Metadata, wb: [f64; 3]) -> Result<f32> {
    let f = |key, default| scalar(metadata, key, default, MatrixEncoding::Float32);
    let (capture, gain) = processing_gain(metadata)?;
    let aperture = metadata
        .properties
        .get("APERTURE")
        .ok_or("missing aperture")?
        .parse::<f64>()?;
    let shutter = metadata
        .properties
        .get("SHUTTER")
        .ok_or("missing shutter")?
        .parse::<f64>()?;
    let histogram = crate::nodes::sensor::estimated_iso::prepare(metadata, wb)? as f32;
    let histogram = if histogram == 0. { capture } else { histogram };
    let ambient = match metadata
        .matrix("AESensorValueAmbient")
        .filter(|m| m.encoding == MatrixEncoding::Int16 && m.dimensions.len() == 1)
    {
        Some(m) if !m.values.is_empty() => m.values.iter().sum::<f64>() / m.values.len() as f64,
        _ => 120.,
    };
    let ambient_ev = (ambient - 48.) * 0.125 + 5.;
    let reference_ev =
        ((aperture * aperture / shutter).ln() - f64::from(gain.ln())) / std::f64::consts::LN_2;
    let reference_ev = if reference_ev.is_finite() {
        reference_ev
    } else {
        0.
    };
    let delta = reference_ev - ambient_ev;
    let estimate = f("ExpEstimate", 0.)? as f64;
    let net = f("ExpNet", 0.)? as f64;
    let ordinary = if reference_ev < 0. {
        estimate - (ambient_ev - reference_ev) + net
    } else {
        estimate + net
    };
    let exponent = if ambient_ev < f("EstimatedISOFlashAmbient", 8.)? as f64
        && delta > f("EstimatedISODiffEV", 3.)? as f64
    {
        estimate + net + delta.min(3.)
    } else {
        ordinary
    };
    let iso = histogram.max(capture / (exponent.exp2() as f32));
    if !iso.is_finite() || iso <= 0. {
        return Err("invalid effective noise ISO".into());
    }
    Ok(iso)
}

struct Recipe {
    wb: [f64; 3],
    mixed: [f64; 13],
    chroma: [f32; 13],
    luminance: [f32; 13],
    iso: f32,
}
#[derive(Clone, Copy)]
pub(crate) struct Couplings {
    pub despeckle_scale: f32,
    pub color_dq_limit: f32,
    pub sharpen_base: f32,
}
pub(crate) fn couplings(metadata: &Metadata, wb: &str, controls: Controls) -> Result<Couplings> {
    let r = recipe(metadata, wb, controls)?;
    let scale = r.mixed[1] as f32;
    // The ColorDQ cap is in column 7. The similar-looking value in column 12
    // belongs to sharpening, so it must not drive automatic noise correction.
    let limit = (r.mixed[7] as f32) * r.luminance[7];
    // Reference HI, control zero, unit reconstruction-relative scales.
    let sharpen_base = ((r.mixed[12] as f32) * r.luminance[12]).max(0.);
    if !scale.is_finite()
        || scale < 0.
        || !limit.is_finite()
        || limit < 0.
        || !sharpen_base.is_finite()
    {
        return Err("invalid production noise coupling".into());
    }
    Ok(Couplings {
        despeckle_scale: scale,
        color_dq_limit: limit,
        sharpen_base,
    })
}
pub(crate) fn monochrome_iso(metadata: &Metadata, wb: &str) -> Result<f32> {
    Ok(recipe(metadata, wb, Controls::default())?.iso)
}

fn recipe(metadata: &Metadata, wb: &str, controls: Controls) -> Result<Recipe> {
    controls.validate()?;
    let key = metadata
        .lists
        .get("WhiteBalanceGains")
        .and_then(|l| l.get(wb))
        .ok_or("missing noise white balance")?;
    let raw = metadata.fixed::<3>(key)?;
    let optional = |key| {
        if metadata.matrix(key).is_some() {
            metadata.fixed::<3>(key)
        } else {
            Ok([1.; 3])
        }
    };
    let fnumber = optional("FnumberGainFact")?;
    let mut temperature = optional("TempGainFact")?;
    if temperature.iter().any(|v| !(0.5..=1.5).contains(v)) {
        temperature = [1.; 3];
    }
    let sensor = optional("SensorAdjustmentGainFact")?;
    let wb: [f64; 3] =
        std::array::from_fn(|c| ((2. * raw[c] * fnumber[c]) * temperature[c]) * sensor[c]);
    if wb.iter().any(|v| !v.is_finite() || *v <= 0.) {
        return Err("invalid noise white balance".into());
    }
    let effective = effective_iso(metadata, wb)?;
    let rows = table::<13>(metadata, "ISONoiseSettings")?.unwrap_or(HI_COLOR.to_vec());
    if rows.len() < 2 || rows.len() > 19 || rows.windows(2).any(|r| r[0][0] >= r[1][0]) {
        return Err("invalid ISO noise knots".into());
    }
    let iso = effective.clamp(rows[0][0], rows[rows.len() - 1][0]);
    let upper = rows.iter().position(|r| iso <= r[0]).unwrap().max(1);
    let fraction = (iso - rows[upper - 1][0]) / (rows[upper][0] - rows[upper - 1][0]);
    let mixed: [f64; 13] = std::array::from_fn(|c| {
        (1. - fraction as f64).mul_add(
            rows[upper - 1][c] as f64,
            (fraction * rows[upper][c]) as f64,
        )
    });
    let sliders = table::<13>(metadata, "DenoisingSlidersStep")?.unwrap_or(SLIDERS.to_vec());
    if sliders.len() != 5 || sliders.iter().flatten().any(|v| *v < 0.) {
        return Err("invalid noise slider rows".into());
    }
    let chroma = sliders[controls.chroma as usize];
    let luminance = sliders[controls.luminance as usize];
    Ok(Recipe {
        wb,
        mixed,
        chroma,
        luminance,
        iso,
    })
}
pub(crate) fn prepare(metadata: &Metadata, wb: &str, controls: Controls) -> Result<Parameters> {
    controls.validate()?;
    let controls = controls.base_controls();
    let (w, h) = (metadata.width as u32, metadata.height as u32);
    if !w.is_multiple_of(4) || !h.is_multiple_of(4) || w < 512 || h < 512 {
        return Err(
            "Merrill noise filtering requires four-by-four tiles with 64-pixel context".into(),
        );
    }
    let Recipe {
        wb,
        mixed,
        chroma,
        luminance,
        iso,
    } = recipe(metadata, wb, controls)?;
    let t = ((iso.ln() as f64) + f64::from_bits(0xc0126bb1bbb55516))
        / f64::from_bits(0xc010a2b23f3bab72);
    let upper_strength = t.mul_add(-0.25, 0.7);
    let lower_strength = t.mul_add(f64::from_bits(0x3fb99999999999a0), 0.8);
    let lower_decay = t.mul_add(f64::from_bits(0xbf9eb851eb851ec0), 0.95);
    let upper_scale = t.mul_add(0.1, 0.2);
    // Cleanup sliders act after this camera-derived middle-preset base.
    const MIDDLE_PRESET_FRACTION: f64 = 0.5;
    let strength =
        (upper_strength - lower_strength).mul_add(MIDDLE_PRESET_FRACTION, lower_strength);
    let decay = (0.95 - lower_decay).mul_add(MIDDLE_PRESET_FRACTION, lower_decay);
    let edge_scale = (upper_scale - 1.).mul_add(MIDDLE_PRESET_FRACTION, 1.);
    let model = table::<3>(metadata, "ColorNoiseModel")?.unwrap_or(vec![
        [7.428315, 10.099755, 1.7354154],
        [0.053895496, 0.06337678, 0.12630486],
        [3.646658e-5, 6.276_382e-5, 5.662402e-5],
    ]);
    if model.len() != 3 {
        return Err("invalid camera noise model".into());
    }
    let noise_step = scalar(
        metadata,
        "ResidualAnisoDiffScaleNM",
        1.,
        MatrixEncoding::Float32,
    )? * chroma[0];
    let mono: [f32; 4] = std::array::from_fn(|r| {
        if r < 3 {
            ((model[r][0] + model[r][2]) + model[r][1]) / 9. * noise_step
        } else {
            0.
        }
    });
    let mut model: [[f32; 4]; 3] =
        std::array::from_fn(|r| std::array::from_fn(|c| if c < 3 { model[r][c] } else { 0. }));
    for (c, wb) in wb.iter().enumerate() {
        let gain = *wb as f32;
        model[0][c] *= gain * gain;
        model[1][c] *= gain;
    }
    let active = crate::nodes::sensor::calibration::rectangle(metadata, "ActiveImageArea")?;
    let cols = metadata.fixed::<4>("DarkShieldColRange")?;
    let area = [
        cols[1] as u32 + 6,
        active[1]
            .checked_sub(32)
            .ok_or("noise working top underflow")?,
        (cols[2] as u32)
            .checked_sub(8)
            .ok_or("noise working right underflow")?,
        active[3] + 32,
    ];
    if area[0] >= area[2] || area[1] >= area[3] || area[2] > w || area[3] > h {
        return Err("invalid noise working rectangle".into());
    }
    let weights = wb.map(|v| (1. / (v * 3.)) as f32);
    let original_scale = (mixed[2] as f32) * luminance[2];
    let mut reconstruction_scale =
        ((f64::from(original_scale).mul_add(8., 0.5) as i32) as f32 * 0.125).max(0.375);
    if reconstruction_scale == 1. && original_scale < 0.984375 {
        reconstruction_scale = 0.875;
    }
    let fixed_size =
        ((((mixed[3].mul_add(2., 1.) as u32) & !1) * (luminance[3] as u32)) | 1).max(1);
    let params = Parameters {
        source: [w, h, 4, 4],
        tile: [
            w / 4 + 128,
            h / 4 + 128,
            64,
            if iso <= 400. { 11 } else { 21 },
        ],
        area,
        filter: [
            mixed[4] as f32,
            1.,
            (strength as f32).clamp(0., 0.99),
            mean_decay((decay as f32).clamp(0., 1.), reconstruction_scale),
        ],
        edge: [
            edge_scale as f32,
            scalar(metadata, "EdgeMapLinShift", 10., MatrixEncoding::Float32)?,
            scalar(metadata, "ResidualEpsilon", 5., MatrixEncoding::Float32)?,
            iso,
        ],
        weights: [weights[0], weights[1], weights[2], 0.],
        model,
        mono,
        diffusion: [
            scalar(metadata, "AnisoLambda", 0.25, MatrixEncoding::Float32)?,
            noise_step,
            scalar(metadata, "ResidualAnisoDiffK2", 2., MatrixEncoding::Int16)?,
            0.,
        ],
        reduction: [reconstruction_scale, 0., 0., 0.],
        geometry: [8, 8, fixed_size, 0],
        window: [0, 0, 4, 4],
        region: [0, 0, w, h],
        column_area: [0; 4], // Prepared only when the column stage is required.
        column: [
            [0.625, 0.5, 0.375, 0.25, 0.125][controls.banding as usize]
                * if iso < 800. {
                    1.
                } else {
                    f32::from_bits(0x3faaa993)
                },
            (15 + 6 * controls.banding) as f32,
            0.,
            0.,
        ],
    };
    if params.filter[0] <= 0.
        || params.edge[0] <= 0.
        || params.edge[2] <= 0.
        || params.diffusion[2] <= 0.
        || bytemuck::cast_slice::<_, f32>(&[params])
            .iter()
            .skip(12)
            .any(|v| !v.is_finite())
    {
        return Err("invalid reconstruction coefficients".into());
    }
    Ok(params)
}
