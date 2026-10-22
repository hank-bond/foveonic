//! Prepares the selected ordinary DP sparse blue LUT and ordered ordinary/HP shading gains.

use crate::{
    nodes::sensor::hlp_calibration::{Gain, Prepared},
    source::{MatrixEncoding, Metadata, Result},
};

use super::color_shading::{color, mix, unpack, Grid};

// Packed CAMF major*1,000,000 + minor*1,000 + revision.
const MODERN_COLOR_RATIO_VERSION: f64 = 3002.;
// The native HP loops visit this fixed sparse footprint. Wider grids need a
// separately recovered route rather than extending the final gain knot.
const HP_FOOTPRINT: [usize; 2] = [184, 472];
const HP_GRID_COLUMNS: usize = 13;
const HP_HORIZONTAL_KNOT_PERIOD: usize = 16;
const HP_VERTICAL_KNOT_PERIOD: f32 = 32.;

/// Select the ordinary model-22 HI route without preparing tables or resources.
pub(super) fn supported(metadata: &Metadata) -> bool {
    super::calibration::is_ordinary_dp(metadata)
        && super::calibration::normalization(metadata).is_ok_and(|v| v <= 8.)
        && metadata
            .fixed::<3>("CamsysVersion")
            .is_ok_and(|v| v[0] * 1_000_000. + v[1] * 1000. + v[2] >= MODERN_COLOR_RATIO_VERSION)
        && optional(metadata, "ChiefRayAngle", 0.).is_ok_and(|v| v == 0.)
        && recorded_balance(metadata).is_ok()
}
pub(super) fn recorded_balance(metadata: &Metadata) -> Result<&'static str> {
    match optional(metadata, "WhiteBalance", 1.)? {
        1. => Ok("Auto"),
        2. => Ok("Sunlight"),
        3. => Ok("Shade"),
        4. => Ok("Overcast"),
        5. => Ok("Incandescent"),
        6. => Ok("Fluorescent"),
        7. => Ok("Flash"),
        8. => Ok("Custom"),
        9. => Ok("Auto"),
        10. => Ok("AutoLSP"),
        11. => Ok("ColorTemp"),
        _ => Ok("Auto"),
    }
}

pub(super) fn prepare(metadata: &Metadata) -> Result<Prepared> {
    if !supported(metadata) {
        return Err("sparse HLP calibration requires the supported DP Quattro HI route".into());
    }
    let extent = [(metadata.width / 2) as u32, (metadata.height / 2) as u32];
    let special = metadata.fixed::<4>("HighlightPixelsInfo")?;
    if special
        .iter()
        .any(|v| !v.is_finite() || *v < 0. || v.fract() != 0. || *v > u32::MAX as f64)
    {
        return Err("invalid highlight-pixel geometry".into());
    }
    let special = special.map(|v| v as u32);
    let aperture = metadata
        .properties
        .get("APERTURE")
        .ok_or("missing capture aperture")?
        .parse()?;
    // Preprocessing reads the recorded CAMF choice, independently of the edit WB.
    let white_balance = recorded_balance(metadata)?;
    prepare_for(metadata, extent, special, aperture, white_balance)
}

pub(super) fn prepare_for(
    m: &Metadata,
    extent: [u32; 2],
    special: [u32; 4],
    aperture: f64,
    white_balance: &str,
) -> Result<Prepared> {
    if !aperture.is_finite() || aperture <= 0. || optional(m, "ChiefRayAngle", 0.)? != 0. {
        return Err("unsupported sparse shading aperture or chief-ray route".into());
    }
    let version = m.fixed::<3>("CamsysVersion")?;
    if version[0] * 1_000_000. + version[1] * 1000. + version[2] < MODERN_COLOR_RATIO_VERSION {
        return Err("legacy color shading ratios are unsupported".into());
    }
    let [w, h] = extent;
    let [x0, y0, dx, dy] = special;
    if w < 17 || h < 5 || dx == 0 || dy == 0 || x0 >= w || y0 >= h {
        return Err("invalid sparse shading geometry".into());
    }
    let nx = (w - x0).div_ceil(dx) as usize;
    let ny = (h - y0).div_ceil(dy) as usize;
    if nx > HP_FOOTPRINT[0] || ny > HP_FOOTPRINT[1] {
        return Err("sparse shading exceeds demonstrated HP grid coverage".into());
    }
    let spatial = spatial(m, aperture as f32)?;
    let color = color(m, white_balance, "B")?;
    let phases = ["R", "G", "BAVE", "B0", "B1", "B2", "B3"];
    let hp: Vec<_> = phases
        .iter()
        .map(|p| hp_grid(m, aperture as f32, p))
        .collect::<Result<_>>()?;
    if hp.iter().any(|g| g.width != HP_GRID_COLUMNS) {
        return Err("legacy HP grid width is unsupported".into());
    }
    let identity = Gain {
        ordinary: [1., 0., 1., 0.],
        mono: [1., 1., 0., 0.],
        red_green: [1., 0., 1., 0.],
        blue: [1., 0., 0., 0.],
        phases: [1.; 4],
    };
    let mut gains = vec![identity; nx * ny];
    for (grid, slot) in [(&spatial, 0), (&color, 1)] {
        for iy in 1..ny {
            let y = y0 + iy as u32 * dy;
            if y >= h - dy {
                break;
            }
            let row = ordinary_row(grid, y, h)?;
            let mut rgb = f64::from(row[0]);
            let mut mono = row[0];
            let mut knot = 0;
            let spacing = f64::from(w) / (grid.width - 1) as f64;
            let mono_spacing = w as f32 / (grid.width - 1) as f32;
            let mut slope = f64::from(row[1] - row[0]) / spacing;
            let mut mono_slope = (row[1] - row[0]) / mono_spacing;
            let mut boundary = spacing;
            let mut mono_boundary = mono_spacing;
            let mut mono_knot = 0;
            for ix in 1..nx {
                let x = x0 + ix as u32 * dx;
                if x >= w - dx {
                    break;
                }
                if f64::from(x) >= boundary {
                    knot += 1;
                    if knot + 1 >= row.len() {
                        return Err("ordinary shading knot outside table".into());
                    }
                    rgb = f64::from(row[knot]);
                    slope = f64::from(row[knot + 1] - row[knot]) / spacing;
                    boundary += spacing;
                }
                if x as f32 >= mono_boundary {
                    mono_knot += 1;
                    if mono_knot + 1 >= row.len() {
                        return Err("ordinary mono shading knot outside table".into());
                    }
                    mono = row[mono_knot];
                    mono_slope = (row[mono_knot + 1] - row[mono_knot]) / mono_spacing;
                    mono_boundary += mono_spacing;
                }
                let gain = &mut gains[iy * nx + ix];
                let p = pair(rgb);
                gain.ordinary[slot * 2..slot * 2 + 2].copy_from_slice(&p);
                gain.mono[slot] = mono;
                rgb += slope;
                mono += mono_slope;
            }
        }
    }
    for iy in 0..ny {
        let rows: Vec<_> = hp.iter().map(|g| hp_row(g, iy)).collect::<Result<_>>()?;
        let mut current: [f64; 3] = std::array::from_fn(|c| f64::from(rows[c][0]));
        let mut slope: [f32; 3] = std::array::from_fn(|c| (rows[c][1] - rows[c][0]) * 0.0625);
        for ix in 0..nx {
            if ix > 0 && ix % HP_HORIZONTAL_KNOT_PERIOD == 0 {
                let knot = ix / HP_HORIZONTAL_KNOT_PERIOD;
                for c in 0..3 {
                    current[c] = f64::from(rows[c][knot]);
                    slope[c] = (rows[c][knot + 1] - rows[c][knot]) * 0.0625;
                }
            }
            let g = &mut gains[iy * nx + ix];
            g.red_green[..2].copy_from_slice(&pair(current[0]));
            g.red_green[2..].copy_from_slice(&pair(current[1]));
            g.blue[..2].copy_from_slice(&pair(current[2]));
            for c in 0..4 {
                g.phases[c] = rows[c + 3][ix / HP_HORIZONTAL_KNOT_PERIOD];
            }
            for c in 0..3 {
                current[c] += f64::from(slope[c]);
            }
        }
    }
    let adjustment = m.fixed::<3>("SensorAdjustmentGainFact")?[2];
    if !adjustment.is_finite() || adjustment <= 0. {
        return Err("invalid blue sensor adjustment".into());
    }
    let normalization = super::calibration::normalization(m)?;
    Prepared::new(
        extent,
        special,
        correction_lut(m)?,
        gains,
        (f64::from(normalization) / adjustment) as f32,
    )?
    .with_blue_ceiling(14300. / normalization)
}
fn optional(m: &Metadata, name: &str, default: f64) -> Result<f64> {
    if m.matrix(name).is_some() {
        m.scalar(name)
    } else {
        Ok(default)
    }
}
fn pair(value: f64) -> [f32; 2] {
    let high = value as f32;
    [high, (value - f64::from(high)) as f32]
}

fn spatial(m: &Metadata, aperture: f32) -> Result<Grid> {
    let stops = m.vector("SpatialGain_Fstop")?;
    validate_stops(stops)?;
    let at = |family: &str| -> Result<Grid> {
        if f64::from(aperture) <= stops[0] {
            return unpack(m, &format!("SpatialGainsProps_0_{family}"), "B");
        }
        let hi = stops
            .iter()
            .position(|&v| v >= f64::from(aperture))
            .unwrap_or(stops.len() - 1);
        let lo = hi - 1;
        let alpha = (1. / f64::from(aperture) - 1. / stops[hi]) / (1. / stops[lo] - 1. / stops[hi]);
        let other = (1. - alpha) as f32;
        mix(
            unpack(m, &format!("SpatialGainsProps_{lo}_{family}"), "B")?,
            unpack(m, &format!("SpatialGainsProps_{hi}_{family}"), "B")?,
            |a, b| {
                f64::from(b).mul_add(f64::from(other), (f64::from(a) * alpha) as f32 as f64) as f32
            },
        )
    };
    let focus = optional(m, "ScaleENC", 0.)?;
    let alpha = (31. - focus) / 31.;
    let other = 1. - alpha;
    mix(at("INF")?, at("MOD")?, |a, b| {
        (f64::from(a) * alpha) as f32 + (f64::from(b) * other) as f32
    })
}
fn hp_grid(m: &Metadata, aperture: f32, phase: &str) -> Result<Grid> {
    let stops = m.vector("SpatialGainHP_Fstop")?;
    validate_stops(stops)?;
    let at = |i| unpack(m, &format!("SpatialGainHPProps_{i}"), phase);
    if f64::from(aperture) <= stops[0] {
        return at(0);
    }
    if f64::from(aperture) >= stops[stops.len() - 1] {
        return at(stops.len() - 1);
    }
    let hi = stops
        .iter()
        .position(|&v| v >= f64::from(aperture))
        .ok_or("HP aperture outside grid")?;
    let lo = hi - 1;
    let alpha = (stops[hi] as f32 - aperture) / (stops[hi] as f32 - stops[lo] as f32);
    let other = 1. - alpha;
    mix(at(lo)?, at(hi)?, |a, b| {
        f64::from(a).mul_add(f64::from(alpha), f64::from(b * other)) as f32
    })
}
fn validate_stops(stops: &[f64]) -> Result<()> {
    if stops.len() < 2
        || stops.iter().any(|v| !v.is_finite() || *v <= 0.)
        || stops.windows(2).any(|v| v[0] >= v[1])
    {
        Err("invalid shading aperture stops".into())
    } else {
        Ok(())
    }
}
fn row(g: &Grid, q: f32) -> Result<Vec<f32>> {
    let lo = q.floor() as usize;
    let hi = q.ceil() as usize;
    if hi >= g.height {
        return Err("shading row outside grid".into());
    }
    let f = q - lo as f32;
    Ok((0..g.width)
        .map(|i| {
            f.mul_add(
                g.values[hi * g.width + i] - g.values[lo * g.width + i],
                g.values[lo * g.width + i],
            )
        })
        .collect())
}
fn ordinary_row(g: &Grid, y: u32, height: u32) -> Result<Vec<f32>> {
    row(g, (y as f32 / (height - 1) as f32) * (g.height - 1) as f32)
}
fn hp_row(g: &Grid, iy: usize) -> Result<Vec<f32>> {
    row(g, iy as f32 * (1. / HP_VERTICAL_KNOT_PERIOD))
}
fn correction_lut(m: &Metadata) -> Result<Vec<u32>> {
    let matrix = m
        .matrix("LUT_HI_RR_x1")
        .ok_or("missing HI blue correction LUT")?;
    if matrix.encoding != MatrixEncoding::Uint16
        || matrix.dimensions.len() != 2
        || matrix.dimensions[0] != 3
    {
        return Err("invalid HI correction LUT".into());
    }
    let length = matrix.dimensions[1];
    let offset = m.scalar("RawOffset")?;
    if length == 0
        || length > 32768
        || matrix.values.len() != length * 3
        || offset < 0.
        || offset >= 16384.
        || offset.fract() != 0.
    {
        return Err("invalid correction LUT dimensions or offset".into());
    }
    let offset = offset as usize;
    let raw = &matrix.values[length * 2..];
    let normalization = super::calibration::normalization(m)?;
    if normalization > 8. {
        return Err("unsupported sparse-LUT normalization".into());
    }
    let mut expanded: Vec<u32> = (0..32768).map(|i| raw[i.min(length - 1)] as u32).collect();
    // makeLUT consumes integer normalization and repeats one expansion for each
    // two units. Preserve its pairwise 14-bit saturation and early tail fill.
    for _ in 0..normalization as u32 / 2 {
        let previous = expanded;
        expanded = vec![0; 32768];
        for i in (0..32768).step_by(2) {
            expanded[i] = (previous[i] * 2).min(16384);
            let sum = previous[i] + previous[i + 1];
            expanded[i + 1] = sum.min(16384);
            if sum > 16384 {
                if i >= 2 {
                    expanded[i - 2..].fill(16384);
                }
                break;
            }
        }
    }
    let extended = |i: usize| expanded[i];
    let shifted = |i: usize| {
        if i < offset {
            i as u32
        } else {
            (extended(i - offset) + offset as u32) & 65535
        }
    };
    let mut inverse = vec![0usize; 32768];
    for index in 1..16384 {
        let lower = shifted(index - 1);
        let upper = shifted(index);
        if upper <= lower {
            continue;
        }
        if upper > 32768 {
            return Err("correction LUT inverse outside supported range".into());
        }
        let reciprocal = 1. / (upper - lower) as f32;
        for j in 0..upper - lower {
            inverse[(lower + j) as usize] =
                ((index - 1) as f64 + f64::from(reciprocal * j as f32) + 0.5) as usize & 65535;
        }
    }
    let mut output: Vec<_> = inverse[..16384]
        .iter()
        .map(|&i| raw[i.min(length - 1)] as u32)
        .collect();
    output.resize(32768, *output.last().unwrap());
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires GPU, QUATTRO_X3F DP3 original and NONUNIT_HLP_REFERENCE native LUT/wrapper fixtures"]
    fn nonunit_lut_and_complete_wrapper_match_native_boundaries() {
        let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
        let directory =
            std::path::PathBuf::from(std::env::var_os("NONUNIT_HLP_REFERENCE").unwrap());
        let mut metadata = (*source.metadata).clone();
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap())
                .unwrap();
        for case in manifest["cases"].as_array().unwrap() {
            metadata.matrices.get_mut("SensorISO").unwrap().values[0] =
                case["normalization"].as_f64().unwrap() * 100.;
            let actual = correction_lut(&metadata).unwrap();
            let expected = std::fs::read(directory.join(case["file"].as_str().unwrap())).unwrap();
            assert!(actual.into_iter().eq(expected
                .as_chunks::<2>()
                .0
                .iter()
                .map(|v| u32::from(u16::from_le_bytes(*v)))));
        }
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join("wrapper.json")).unwrap())
                .unwrap();
        let gpu = crate::gpu::Gpu::new().unwrap();
        for case in manifest["cases"].as_array().unwrap() {
            metadata.matrices.get_mut("SensorISO").unwrap().values[0] =
                case["normalization"].as_f64().unwrap() * 100.;
            let floats = |kind: &str| {
                let bytes = std::fs::read(directory.join(case[kind].as_str().unwrap())).unwrap();
                bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()
            };
            let input_rgb = gpu.upload("Nonunit HLP RGB", &floats("input_rgb")).unwrap();
            let input_blue = gpu
                .upload("Nonunit HLP blue", &floats("input_blue"))
                .unwrap();
            let prepared =
                prepare_for(&metadata, [112, 76], [0, 0, 16, 4], 4., "Sunlight").unwrap();
            let output = crate::nodes::sensor::hlp_calibration::apply(
                &gpu,
                &input_rgb,
                &input_blue,
                &prepared,
                &mut Default::default(),
            )
            .unwrap();
            for (kind, value) in [
                ("rgb", &output.rgb),
                ("blue", &output.blue),
                ("scratch", &output.scratch),
            ] {
                let bytes = gpu.readback(value).unwrap();
                let actual: &[f32] = bytemuck::cast_slice(&bytes);
                let expected = floats(kind);
                let maximum = actual
                    .iter()
                    .zip(&expected)
                    .map(|(a, b)| (a - b).abs())
                    .fold(0_f32, f32::max);
                eprintln!(
                    "HLP normalization{} {kind} max{maximum}",
                    case["normalization"]
                );
                assert_eq!(actual.len(), expected.len());
                for (i, (&a, &b)) in actual.iter().zip(&expected).enumerate() {
                    assert!(
                        (a - b).abs() <= 0.008_f32.max(b.abs() * f32::EPSILON * 4.),
                        "{} {kind}/{i}: {a} vs {b}",
                        case["normalization"]
                    );
                }
            }
        }
    }
    #[test]
    #[ignore = "requires GPU, HLP_REFERENCE fixtures and QUATTRO_X3F DP2 original"]
    fn complete_wrapper_matches_independent_fixtures() {
        let source =
            crate::source::Source::open(std::env::var_os("QUATTRO_X3F").expect("set QUATTRO_X3F"))
                .unwrap();
        let directory =
            std::path::PathBuf::from(std::env::var_os("HLP_REFERENCE").expect("set HLP_REFERENCE"));
        let gpu = crate::gpu::Gpu::new().unwrap();
        let read = |name: &str| -> Vec<f32> {
            let bytes = std::fs::read(directory.join(name)).unwrap();
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|v| f32::from_le_bytes(*v))
                .collect()
        };
        let rgb = gpu.upload("HLP fixture RGB", &read("rgb.f32")).unwrap();
        let blue = gpu.upload("HLP fixture blue", &read("blue.f32")).unwrap();
        for (name, aperture) in [("28", 2.8), ("40", 4.), ("54", 5.4)] {
            let p = prepare_for(
                &source.metadata,
                [112, 76],
                [0, 0, 16, 4],
                aperture,
                "Sunlight",
            )
            .unwrap();
            let out = crate::nodes::sensor::hlp_calibration::apply(
                &gpu,
                &rgb,
                &blue,
                &p,
                &mut Default::default(),
            )
            .unwrap();
            for (part, buffer) in [
                ("rgb", &out.rgb),
                ("blue", &out.blue),
                ("scratch", &out.scratch),
            ] {
                let bytes = gpu.readback(buffer).unwrap();
                let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
                let expected = read(&format!("{name}-{part}.f32"));
                assert_eq!(actual.len(), expected.len());
                let mut changed = 0;
                let mut max = 0_f32;
                let mut sum = 0_f64;
                for (&a, &b) in actual.iter().zip(&expected) {
                    assert!(a.is_finite());
                    let e = (a - b).abs();
                    max = max.max(e);
                    sum += f64::from(e);
                    changed += usize::from(a.to_bits() != b.to_bits());
                }
                eprintln!(
                    "HLP {name}/{part} changed={changed}/{} mean={} max={max}",
                    actual.len(),
                    sum / actual.len() as f64
                );
                assert!(max <= 0.008, "{name}/{part} maximum {max}");
            }
        }
    }
    #[test]
    #[ignore = "requires GPU, HLP_NATIVE_BOUNDARY and QUATTRO_X3F DP2 original"]
    fn complete_original_wrapper_matches_native_boundary() {
        let source =
            crate::source::Source::open(std::env::var_os("QUATTRO_X3F").expect("set QUATTRO_X3F"))
                .unwrap();
        let directory = std::path::PathBuf::from(
            std::env::var_os("HLP_NATIVE_BOUNDARY").expect("set HLP_NATIVE_BOUNDARY"),
        );
        let info: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join("hlp-boundary.json")).unwrap())
                .unwrap();
        assert_eq!(info["complete"], true);
        let read = |name: &str| -> Vec<f32> {
            let bytes = std::fs::read(directory.join(name)).unwrap();
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|v| f32::from_le_bytes(*v))
                .collect()
        };
        let gpu = crate::gpu::Gpu::new().unwrap();
        let rgb = gpu
            .upload("Actual HLP input RGB", &read("hlp-input-rgb.f32"))
            .unwrap();
        let blue = gpu
            .upload("Actual HLP input blue", &read("hlp-input-blue.f32"))
            .unwrap();
        // This retained request explicitly uses f/4, independently of file f/2.8.
        let mut metadata = (*source.metadata).clone();
        metadata.properties.insert("APERTURE".into(), "4".into());
        let prepared = prepare(&metadata).unwrap();
        let output = crate::nodes::sensor::hlp_calibration::apply(
            &gpu,
            &rgb,
            &blue,
            &prepared,
            &mut Default::default(),
        )
        .unwrap();
        for (part, buffer) in [
            ("rgb", &output.rgb),
            ("blue", &output.blue),
            ("scratch", &output.scratch),
        ] {
            let bytes = gpu.readback(buffer).unwrap();
            let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
            let mut expected = read(&format!("hlp-output-{part}.f32"));
            if part == "scratch" {
                // The native header includes an unused final row and column.
                // Compare the initialized samples using the native physical stride.
                let stride = info["output"]["scratch"]["row_bytes"].as_u64().unwrap() as usize / 4;
                let [width, height] = prepared.scratch_extent();
                expected = (0..height as usize)
                    .flat_map(|y| {
                        expected[y * stride..y * stride + width as usize * 3]
                            .iter()
                            .copied()
                    })
                    .collect();
            }
            assert_eq!(actual.len(), expected.len());
            let mut changed = 0;
            let mut maximum = 0_f32;
            let mut total = 0_f64;
            for (i, (&a, &b)) in actual.iter().zip(&expected).enumerate() {
                assert!(a.is_finite() && b.is_finite());
                let error = (a - b).abs();
                maximum = maximum.max(error);
                total += f64::from(error);
                changed += usize::from(a.to_bits() != b.to_bits());
                assert!(
                    error <= 0.008_f32.max(b.abs() * f32::EPSILON * 8.),
                    "{part}/{i}: {a} vs {b}"
                );
            }
            eprintln!(
                "HLP native/{part} changed={changed}/{} mean={} max={maximum}",
                actual.len(),
                total / actual.len() as f64
            );
        }
    }
}
