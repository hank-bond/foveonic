//! Resolves monochrome layer weights, ISO presets, limits, and range-weight tables from metadata.
//!
//! Filter constants and stage arithmetic follow the reference implementation's monochrome modules.

use super::Controls;
use crate::{
    noise,
    recipe::Key,
    source::{Metadata, Result},
};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Parameters {
    pub source: [u32; 4], // width, height, directional window, algorithmic strips
    pub mix: [f32; 4],    // three WB-normalized amounts, processing gain
    pub filter: [f32; 4], // directional divisor, histogram divisor, column gate, speckle gate
    pub area: [u32; 4],
    pub detail: [f32; 4], // estimated ISO, signed-detail magnitude limit
}

pub(crate) struct Prepared {
    pub params: Parameters,
}
#[derive(Clone, Copy)]
pub(crate) enum Stage {
    Mix,
    Directional,
    Column,
    Histogram,
    Despeckle,
    Signal,
    Expand,
}
impl Prepared {
    pub(crate) fn new(metadata: &Metadata, wb: &str, controls: Controls) -> Result<Self> {
        controls.validate()?;
        let iso = noise::monochrome_iso(metadata, wb)?;
        if iso > 6400. {
            return Err(
                "monochrome column correction does not yet support estimated ISO above 6400".into(),
            );
        }
        let width = metadata.width as u32;
        let height = metadata.height as u32;
        if width < 41 || height < 41 || !height.is_multiple_of(8) {
            return Err("unsupported monochrome working geometry".into());
        }
        let strips = if height >= 328 { 8 } else { 1 };
        let key = metadata
            .lists
            .get("WhiteBalanceGains")
            .and_then(|l| l.get(wb))
            .ok_or("missing mono white balance")?;
        let raw = metadata.fixed::<3>(key)?;
        if raw.iter().any(|v| !v.is_finite() || *v <= 0.) {
            return Err("invalid mono white balance".into());
        }
        let normalization = (1. / (2. * raw.into_iter().fold(f64::INFINITY, f64::min))) as f32;
        let mix = controls.mix.map(|v| v * normalization);
        let gain = noise::processing_gain(metadata)?.1;
        let active = crate::nodes::sensor::calibration::rectangle(metadata, "ActiveImageArea")?;
        let cols = metadata.fixed::<4>("DarkShieldColRange")?;
        let area = [
            cols[1] as u32 + 6,
            active[1]
                .checked_sub(32)
                .ok_or("mono working top underflow")?,
            (cols[2] as u32)
                .checked_sub(8)
                .ok_or("mono working right underflow")?,
            active[3] + 32,
        ];
        if area[0] >= area[2] || area[1] >= area[3] || area[2] > width || area[3] > height {
            return Err("invalid mono working borders".into());
        }
        let column = controls.banding as usize;
        let directional = if iso < 800. {
            1.
        } else {
            f32::from_bits(0x3faaa993)
        } * [0.625, 0.5, 0.375, 0.25, 0.125][column];
        let knots = [100., 200., 400., 800., 1600., 3200., 6400.];
        let fractions = [0.05_f32, 0.055, 0.07, 0.12, 0.17, 0.25, 0.33];
        let fraction = interpolate(iso, &knots, &fractions);
        let histogram = mono_noise(iso, controls.noise as usize);
        let magnitude = interpolate(iso, &[100., 125., 160., 200.], &[500., 800., 1100., 1500.]);
        let speckle = (64. / (4096. / (iso / 100.).min(64.).max(gain))).powi(3);
        if mix
            .iter()
            .chain([gain, directional, histogram, fraction, speckle].iter())
            .any(|v| !v.is_finite())
        {
            return Err("unrepresentable mono parameters".into());
        }
        Ok(Self {
            params: Parameters {
                source: [width, height, [15, 21, 27, 33, 39][column], strips],
                mix: [mix[0], mix[1], mix[2], gain],
                filter: [
                    directional,
                    histogram,
                    fraction * [0.2, 0.3, 0.4, 0.5, 1.][column],
                    speckle,
                ],
                area,
                detail: [iso, magnitude, 0., 0.],
            },
        })
    }
    pub(crate) fn key(&self, stage: Stage) -> Key {
        let p = &self.params;
        let key = Key::pod(&[p.source[0], p.source[1]]);
        match stage {
            Stage::Mix => key.slice(&p.mix[..3]).with(&p.area),
            Stage::Directional => key.with(&p.source[2]).with(&p.filter[0]),
            Stage::Column => key.with(&p.source).with(&p.filter[2]),
            Stage::Histogram => key.with(&p.source[3]).with(&p.filter[1]),
            Stage::Despeckle => key.with(&p.filter[3]),
            Stage::Signal => key.with(&p.mix[3]).with(&p.detail[1]),
            Stage::Expand => key.with(&p.mix[3]),
        }
    }
    /// Range tables contain metadata-derived coefficients only, never photographic samples.
    pub(crate) fn table(&self, directional: bool) -> (Vec<u32>, Vec<f32>) {
        let count = if directional { 4096 } else { 1024 };
        let divisor = f64::from(self.params.filter[usize::from(!directional)]);
        let mut indexes = Vec::with_capacity(count * 3);
        let mut weights = Vec::new();
        for bin in 0..count {
            let level = (bin * if directional { 1 } else { 4 }) as f64;
            let variance = (0.0000173171583027778 * level).mul_add(
                level,
                0.0428362301201803_f64.mul_add(level, 9.85389175535173),
            ) / divisor;
            let radius = (3. * variance.sqrt()) as usize / if directional { 1 } else { 4 };
            let first = if directional {
                0
            } else {
                bin.saturating_sub(radius)
            };
            let last = if directional {
                radius
            } else {
                (bin + radius).min(1023)
            };
            indexes.extend([
                weights.len() as u32,
                first as u32,
                (last - first + 1) as u32,
            ]);
            for neighbor in first..=last {
                let delta = if directional {
                    neighbor
                } else {
                    bin.abs_diff(neighbor) * 4
                } as f64;
                weights.push((-(delta * delta) / variance).exp() as f32);
            }
        }
        (indexes, weights)
    }
}
impl Stage {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Mix => "submit mono mixing",
            Self::Directional => "submit mono directional",
            Self::Column => "submit mono column",
            Self::Histogram => "submit mono histogram",
            Self::Despeckle => "submit mono despeckle",
            Self::Signal => "submit mono detail",
            Self::Expand => "submit mono gain",
        }
    }
    pub(crate) fn output_name(self) -> &'static str {
        match self {
            Self::Mix => "mono.mix",
            Self::Directional => "mono.directional",
            Self::Column => "mono.column",
            Self::Histogram => "mono.filtered",
            Self::Despeckle => "mono.despeckle",
            Self::Signal => "detail.signal",
            Self::Expand => "monochrome.rgb",
        }
    }
}
fn interpolate(iso: f32, knots: &[f32], values: &[f32]) -> f32 {
    if iso <= knots[0] {
        return values[0];
    }
    if iso >= knots[knots.len() - 1] {
        return values[values.len() - 1];
    }
    let high = knots
        .iter()
        .position(|v| iso <= *v)
        .unwrap_or(knots.len() - 1);
    values[high].mul_add(
        iso - knots[high - 1],
        values[high - 1] * (knots[high] - iso),
    ) / (knots[high] - knots[high - 1])
}
fn mono_noise(iso: f32, index: usize) -> f32 {
    // Modern secondary-monochrome, HI working scale. The metadata ISO remains independent of the slider.
    let knots = [0., 100., 200., 400., 800., 1600., 3200., 6400.];
    let rows: [[f64; 5]; 8] = [
        [0., 0.5, 0.3, 0.1, 0.05],
        [0., 0.5, 0.3, 0.1, 0.05],
        [0., 0.5, 0.3, 0.1, 0.05],
        [0., 0.5, 0.3, 0.1, 0.05],
        [0., 0.5, 0.3, 0.1, 0.05],
        [0., 1., 0.45, 0.3, 0.2],
        [0., 1.5, 0.65, 0.35, 0.2],
        [0., 2., 1., 0.4, 0.2],
    ];
    let high = knots.iter().position(|k| iso <= *k).unwrap_or(7).max(1);
    let numerator = rows[high][index].mul_add(
        f64::from(iso - knots[high - 1]),
        rows[high - 1][index] * f64::from(knots[high] - iso),
    ) as f32;
    numerator / (knots[high] - knots[high - 1])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modern_mono_gain_and_detail_limits_use_their_independent_iso_knots() {
        for iso in [100., 200., 400., 800., 1600., 2400., 3200., 6400.] {
            assert_eq!(mono_noise(iso, 0), 0.);
        }
        assert_eq!(mono_noise(100., 2), 0.3);
        assert_eq!(mono_noise(1600., 2), 0.45);
        assert!((mono_noise(2400., 2) - 0.55).abs() < 1e-7);
        assert_eq!(mono_noise(6400., 2), 1.);
        assert_eq!(
            interpolate(
                6400.,
                &[100., 125., 160., 200.],
                &[500., 800., 1100., 1500.]
            ),
            1500.
        );
        assert_eq!(
            interpolate(100., &[100., 125., 160., 200.], &[500., 800., 1100., 1500.]),
            500.
        );
    }
}
