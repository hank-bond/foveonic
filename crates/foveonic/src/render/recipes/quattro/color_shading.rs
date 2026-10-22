//! Decodes and combines ordinary metadata shading grids for explicit channels and white balance.

use crate::source::{MatrixEncoding, Metadata, Result};

// Version 0.3.2 stores resolved ratios for every recorded white-balance alias.
const DIRECT_COLOR_RATIO_VERSION: f64 = 3002.;

#[derive(Clone)]
pub(super) struct Grid {
    pub width: usize,
    pub height: usize,
    pub values: Vec<f32>,
}

pub(super) fn unpack(m: &Metadata, name: &str, channel: &str) -> Result<Grid> {
    let props = m
        .lists
        .get(name)
        .ok_or_else(|| format!("missing shading properties {name}"))?;
    let get = |prefix: &str| {
        props
            .get(&format!("{prefix}{channel}"))
            .ok_or_else(|| format!("missing shading field {prefix}{channel}"))
    };
    let minimum = get("MinGains")?.parse::<f64>()? as f32;
    let delta = get("Delta")?.parse::<f64>()?;
    let table = m
        .matrix(get("GainsTable")?)
        .ok_or("missing shading byte grid")?;
    if table.encoding != MatrixEncoding::Uint8 || table.dimensions.len() != 2 {
        return Err("invalid shading grid encoding".into());
    }
    let [height, width] = [table.dimensions[0], table.dimensions[1]];
    if width < 2
        || height < 2
        || width.checked_mul(height) != Some(table.values.len())
        || !minimum.is_finite()
        || !delta.is_finite()
    {
        return Err("invalid shading grid dimensions".into());
    }
    let values = table
        .values
        .iter()
        .map(|v| minimum + (delta * v) as f32)
        .collect();
    Ok(Grid {
        width,
        height,
        values,
    })
}
pub(super) fn mix(a: Grid, b: Grid, f: impl Fn(f32, f32) -> f32) -> Result<Grid> {
    if a.width != b.width || a.height != b.height {
        return Err("mismatched shading grids".into());
    }
    let values = a
        .values
        .iter()
        .zip(&b.values)
        .map(|(&a, &b)| f(a, b))
        .collect();
    Ok(Grid { values, ..a })
}
pub(super) fn color(m: &Metadata, white_balance: &str, channel: &str) -> Result<Grid> {
    let version = m.fixed::<3>("CamsysVersion")?;
    if version[0] * 1_000_000. + version[1] * 1000. + version[2] < DIRECT_COLOR_RATIO_VERSION {
        return Err("legacy color-shading ratios are unsupported".into());
    }

    // Modern Custom and ColorTemp use the captured ratio triple just like the
    // named presets. Their older temperature-dependent calculation is excluded.
    let aliases = m
        .lists
        .get("ColorShadingGainRatio")
        .ok_or("missing color-shading aliases")?;
    let selected = m.fixed::<3>(
        aliases
            .get(white_balance)
            .ok_or("missing selected color shading")?,
    )?;
    let auto = m.fixed::<3>(aliases.get("Auto").ok_or("missing Auto color shading")?)?;
    let blend = if white_balance == "Auto" {
        0.
    } else {
        if m.matrix("ColorShadingBlendRatio").is_some() {
            m.scalar("ColorShadingBlendRatio")?
        } else {
            0.
        }
    };
    let other = (1. - blend) as f32;
    let weights: [f32; 3] = std::array::from_fn(|i| {
        f64::from(other).mul_add(auto[i], (blend * selected[i]) as f32 as f64) as f32
    });
    let a = unpack(m, "SunlightColorShadingFactor", channel)?;
    let b = unpack(m, "FluorescentColorShadingFactor", channel)?;
    let c = unpack(m, "IncandescentColorShadingFactor", channel)?;
    if a.width != b.width || a.width != c.width || a.height != b.height || a.height != c.height {
        return Err("mismatched color shading grids".into());
    }
    let values = a
        .values
        .iter()
        .zip(&b.values)
        .zip(&c.values)
        .map(|((&a, &b), &c)| {
            // The native chain rounds its first blend before adding the
            // incandescent contribution. This matters for mixed lighting.
            weights[2].mul_add(c, weights[0].mul_add(a, weights[1] * b))
        })
        .collect();
    Ok(Grid { values, ..a })
}

#[cfg(test)]
#[path = "color_shading_test.rs"]
mod tests;
