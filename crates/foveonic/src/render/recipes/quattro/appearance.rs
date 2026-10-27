//! Prepares supported color-mode responses from exposure and tone controls.

use crate::{
    nodes::finish::{code_response, tone},
    source::{Metadata, Result},
};

// These profiles are the reference implementation's ordinary color responses.
// Its lattice is sampled at quarter-code resolution, with a control every fifty
// samples. Preserve the discrete join and Float32 interpolation boundaries.
pub(super) const LINEAR_CODE_WHITE: u32 = 16384;
const SATURATION_CODE: usize = LINEAR_CODE_WHITE as usize;
const CONTROL_SPACING: usize = 50;
const PROFILE_GAMMA: f64 = 2.222_222_222_222_222_3;
pub(super) const MODES: [&str; 10] = [
    "Standard",
    "Vivid",
    "Neutral",
    "Portrait",
    "Landscape",
    "FCBlue",
    "FCYellow",
    "Cinema",
    "ForestGreen",
    "SunsetRed",
];

#[derive(Clone, Copy)]
struct Profile {
    start: f64,
    end: f64,
    first_slope: f64,
    second_slope: f64,
    join: f64,
}

fn profile(mode: &str) -> Result<Profile> {
    let (start, end, first_slope, second_slope, join) = match mode {
        "Standard" => (-1.19, 2.85, 3., 1.55, 0.078),
        "Vivid" => (-1.2, 2.85, 3.55, 1.6, 0.07),
        "Neutral" => (-1.19, 2.85, 2.8, 1.56, 0.08),
        "Portrait" => (-1.05, 2.65, 3.1, 1.97, 0.03),
        "Cinema" => (-1.2, 2.85, 3.5, 1.9, 0.07),
        "Landscape" | "FCBlue" | "FCYellow" | "ForestGreen" | "SunsetRed" => {
            (-1.17, 2.85, 3., 1.4, 0.1)
        }
        _ => return Err(format!("unsupported Quattro color mode {mode}").into()),
    };
    Ok(Profile {
        start,
        end,
        first_slope,
        second_slope,
        join,
    })
}

pub(super) fn prepare(
    metadata: &Metadata,
    mode: &str,
    exposure: f64,
    mut controls: tone::Controls,
) -> Result<code_response::Prepared> {
    if !(-2. ..=2.).contains(&exposure)
        || [controls.contrast, controls.shadows, controls.highlights]
            .iter()
            .any(|value| !value.is_finite())
    {
        return Err(
            "Quattro response requires finite controls and exposure between -2 and 2 EV".into(),
        );
    }
    if let Some(key) = metadata
        .lists
        .get("ColorModeContrastCompensations")
        .and_then(|list| list.get(mode))
    {
        controls.contrast += metadata.scalar(key)?;
    }
    // The curve already removes sRGB encoding. Generic finishing supplies
    // destination primaries and transfer once.
    let response: Vec<f32> = tone_table(profile(mode)?, exposure, controls)
        .into_iter()
        .map(|value| f32::from(value) / 65536.)
        .collect();
    code_response::Prepared::new(response.repeat(3))
}

fn tone_table(profile: Profile, exposure: f64, controls: tone::Controls) -> Vec<u16> {
    curve_table(profile, exposure, controls, false)
}

/// Monochrome uses the same sampled lattice with a steeper neutral response.
pub(super) fn prepare_monochrome(
    exposure: f64,
    controls: tone::Controls,
) -> Result<code_response::Prepared> {
    if !(-2. ..=2.).contains(&exposure)
        || [controls.contrast, controls.shadows, controls.highlights]
            .iter()
            .any(|v| !v.is_finite())
    {
        return Err("invalid monochrome response controls".into());
    }
    let profile = Profile {
        start: -1.17,
        end: 1.55,
        first_slope: 3.25,
        second_slope: 2.46,
        join: 0.1,
    };
    let response: Vec<f32> = curve_table(profile, exposure, controls, true)
        .into_iter()
        .map(|value| f32::from(value) / 65536.)
        .collect();
    code_response::Prepared::new(response.repeat(3))
}

fn curve_table(
    profile: Profile,
    exposure: f64,
    controls: tone::Controls,
    monochrome: bool,
) -> Vec<u16> {
    // Exposure changes the input extent of the response. Its upper bound also
    // bounds the size of the reference's smoothing lattice.
    let saturation = (-exposure).exp2() * SATURATION_CODE as f64;
    // Monochrome retains the original code domain when positive exposure moves
    // saturation below it, so the shoulder continues above nominal saturation.
    let domain = if monochrome {
        saturation.max(SATURATION_CODE as f64)
    } else {
        saturation
    };
    let scale = domain as usize;
    let contrast = (controls.contrast.clamp(-2., 2.) * 0.5).exp2();
    let start = if monochrome {
        profile.start
    } else {
        profile.start * ((-controls.shadows).clamp(-2., 2.) / 8.).exp2()
    };
    let end = if monochrome {
        profile.end
    } else {
        profile.end * (controls.highlights.clamp(-2., 2.) / 8.).exp2()
    };
    let gamma = if monochrome { 2.4 } else { PROFILE_GAMMA };
    let increment = 1. / saturation;
    let logistic = |coordinate: f64, slope: f64| {
        let position = (end - start).mul_add(coordinate.powf(1. / gamma), start);
        1. / (1. + (-slope * position).exp())
    };
    let mut coordinate = 0.;
    let mut basic = Vec::with_capacity(scale + 1);
    loop {
        basic.push(logistic(coordinate, profile.first_slope * contrast) as f32);
        coordinate += increment;
        if coordinate >= profile.join {
            break;
        }
    }
    let offset = logistic(coordinate, profile.first_slope * contrast)
        - logistic(coordinate, profile.second_slope * contrast);
    while basic.len() <= scale {
        basic.push((offset + logistic(coordinate, profile.second_slope * contrast)) as f32);
        coordinate += increment;
    }
    if monochrome {
        let shadows = (-controls.shadows).clamp(-2., 2.);
        let highlights = controls.highlights.clamp(-2., 2.);
        if shadows != 0. {
            let threshold = basic[scale] * 0.25;
            let end = basic.iter().position(|v| *v >= threshold).unwrap_or(scale);
            let base = basic[0];
            let span = basic[end] - base;
            let power = (2. * shadows).exp2() as f32;
            for value in &mut basic[..end] {
                *value = ((*value - base) / span).powf(power).mul_add(span, base);
            }
        }
        if highlights != 0. {
            let peak = basic[scale];
            let threshold = peak * 0.75;
            let begin = basic.iter().rposition(|v| *v <= threshold).unwrap_or(0);
            let base = basic[begin];
            let span = peak - base;
            let control = if highlights < 0. {
                highlights * 0.25
            } else {
                highlights
            };
            let power = (-2. * control).exp2() as f32;
            for value in &mut basic[begin..] {
                *value = ((*value - base) / span).powf(power).mul_add(span, base);
            }
        }
    }
    let quarter: Vec<f32> = basic.into_iter().step_by(4).collect();
    let mut controls: Vec<f32> = quarter.iter().step_by(CONTROL_SPACING).copied().collect();
    controls.push(*quarter.last().unwrap());
    let mut smoothed = Vec::with_capacity(scale / 4 + 1);
    let mut scratch = controls.clone();
    for index in 0..=scale / 4 {
        let t = (1. / (domain * 0.25) as f32) * index as f32;
        let remaining = 1. - t;
        scratch.copy_from_slice(&controls);
        for length in (1..scratch.len()).rev() {
            for j in 0..length {
                // ARM64 rounds the right product before fusing the left.
                scratch[j] = remaining.mul_add(scratch[j], t * scratch[j + 1]);
            }
        }
        smoothed.push(scratch[0]);
    }
    let mut expanded = Vec::with_capacity(scale + 1);
    expanded.push(smoothed[0]);
    for pair in smoothed.windows(2) {
        let [left, right] = [pair[0], pair[1]];
        let step = (right - left) * 0.25;
        expanded.extend([left + step, left + step * 2., left + step * 3., right]);
    }
    // The reference repeats the final quarter-grid sample across the remainder.
    expanded.resize(scale + 1, *expanded.last().unwrap());
    let low = f64::from(expanded[0]);
    let range = f64::from(expanded[scale]) - low;
    let response_scale = if monochrome {
        saturation / ((f64::from(expanded[saturation as usize]) - low) / range)
    } else {
        saturation
    };
    let curve: Vec<f32> = expanded
        .into_iter()
        .map(|value| {
            let encoded = (f64::from(value) - low) / range;
            let linear = if encoded < 0.04045 {
                encoded / 12.92
            } else {
                ((encoded + 0.055) / 1.055).powf(2.4)
            };
            (response_scale * linear) as f32
        })
        .collect();
    let mut tone: Vec<u16> = curve[..scale]
        .iter()
        .map(|&v| (65535. / f64::from(curve[scale]) * f64::from(v) + 0.5).clamp(0., 65535.) as u16)
        .collect();
    tone.resize(65536, *tone.last().unwrap());
    tone
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn color_profiles_match_native_samples() {
        let positions = [0, 100, 1000, 4095, 8192, 16383, 32767, 65535];
        for (mode, expected) in [
            ("Vivid", [0, 205, 9292, 42642, 58615, 65535, 65535, 65535]),
            ("Neutral", [0, 278, 9654, 42256, 58272, 65535, 65535, 65535]),
            (
                "Portrait",
                [0, 351, 9984, 44549, 60164, 65535, 65535, 65535],
            ),
            (
                "Landscape",
                [0, 253, 9313, 41829, 57240, 65534, 65534, 65534],
            ),
            ("FCBlue", [0, 253, 9313, 41829, 57240, 65534, 65534, 65534]),
            (
                "FCYellow",
                [0, 253, 9313, 41829, 57240, 65534, 65534, 65534],
            ),
            (
                "ForestGreen",
                [0, 253, 9313, 41829, 57240, 65534, 65534, 65534],
            ),
            (
                "SunsetRed",
                [0, 253, 9313, 41829, 57240, 65534, 65534, 65534],
            ),
            ("Cinema", [0, 209, 9410, 46421, 60992, 65535, 65535, 65535]),
        ] {
            let table = tone_table(profile(mode).unwrap(), 0., tone::Controls::default());
            assert_eq!(positions.map(|index| table[index]), expected);
        }
        assert!(profile("Monochrome").is_err());
    }

    #[test]
    fn exposure_and_tone_controls_match_native_samples() {
        // Independent native Standard tables at the eight source-code positions.
        let positions = [0, 100, 1000, 4095, 8192, 16383, 32767, 65535];
        for (exposure, controls, expected) in [
            (
                0.,
                tone::Controls::default(),
                [0, 257, 9628, 42278, 58228, 65535, 65535, 65535],
            ),
            (
                0.3,
                tone::Controls::default(),
                [0, 322, 13060, 47734, 61394, 65535, 65535, 65535],
            ),
            (
                -0.3,
                tone::Controls::default(),
                [0, 212, 6895, 36868, 54318, 64214, 65535, 65535],
            ),
            (
                0.,
                tone::Controls {
                    shadows: 1.,
                    ..Default::default()
                },
                [0, 316, 11369, 43794, 58659, 65535, 65535, 65535],
            ),
            (
                0.,
                tone::Controls {
                    contrast: 1.,
                    ..Default::default()
                },
                [0, 151, 9081, 50578, 62753, 65535, 65535, 65535],
            ),
            (
                0.7,
                tone::Controls {
                    shadows: -0.6,
                    contrast: 0.8,
                    highlights: -0.3,
                },
                [0, 259, 17166, 58209, 64997, 65535, 65535, 65535],
            ),
        ] {
            let table = tone_table(profile("Standard").unwrap(), exposure, controls);
            assert_eq!(positions.map(|index| table[index]), expected);
        }
    }

    #[test]
    #[ignore = "requires QUATTRO_CONTROL_TONES with native tone cases and binary tables"]
    fn adjusted_modes_match_complete_native_tables() {
        let directory = std::path::PathBuf::from(std::env::var("QUATTRO_CONTROL_TONES").unwrap());
        let cases: Vec<serde_json::Value> =
            serde_json::from_slice(&std::fs::read(directory.join("cases.json")).unwrap()).unwrap();
        for case in cases {
            let actual = tone_table(
                profile(case["mode"].as_str().unwrap_or("Standard")).unwrap(),
                case["exposure"].as_f64().unwrap(),
                tone::Controls {
                    shadows: case["shadows"].as_f64().unwrap(),
                    contrast: case["contrast"].as_f64().unwrap(),
                    highlights: case["highlights"].as_f64().unwrap(),
                },
            );
            let expected = std::fs::read(directory.join(case["file"].as_str().unwrap())).unwrap();
            let differences: Vec<u16> = expected
                .as_chunks::<2>()
                .0
                .iter()
                .zip(&actual)
                .map(|(bytes, value)| u16::from_le_bytes(*bytes).abs_diff(*value))
                .collect();
            let maximum = differences.iter().max().unwrap();
            eprintln!(
                "case {case}: maximum {maximum}, changed {}",
                differences.iter().filter(|&&d| d != 0).count()
            );
            assert_eq!(*maximum, 0, "case {case}: maximum {maximum}");
        }
    }

    #[test]
    #[ignore = "requires QUATTRO_STANDARD_TONE containing an independently captured tone LUT"]
    fn neutral_standard_matches_complete_native_table() {
        let bytes = std::fs::read(std::env::var("QUATTRO_STANDARD_TONE").unwrap()).unwrap();
        assert_eq!(bytes.len(), 3 * 65536 * 2);
        let tone = tone_table(profile("Standard").unwrap(), 0., tone::Controls::default());
        for (index, word) in bytes.as_chunks::<2>().0.iter().enumerate() {
            assert_eq!(
                tone[index % 65536],
                u16::from_le_bytes([word[0], word[1]]),
                "entry {index}"
            );
        }
    }

    #[test]
    #[ignore = "requires QUATTRO_MONO_TONE captured native monochrome lookup"]
    fn monochrome_lattice_matches_complete_native_lookup() {
        let root = std::path::PathBuf::from(std::env::var_os("QUATTRO_MONO_TONE").unwrap());
        let profile = Profile {
            start: -1.17,
            end: 1.55,
            first_slope: 3.25,
            second_slope: 2.46,
            join: 0.1,
        };
        let actual = curve_table(profile, 0., tone::Controls::default(), true);
        let expected = std::fs::read(root.join("tone.u16")).unwrap();
        let expected: &[u16] = bytemuck::cast_slice(&expected);
        assert_eq!(expected.len(), actual.len() * 3);
        let mut different = 0;
        let mut maximum = 0;
        for channel in expected.chunks_exact(actual.len()) {
            for (&a, &b) in actual.iter().zip(channel) {
                different += usize::from(a != b);
                maximum = maximum.max(a.abs_diff(b));
            }
        }
        assert_eq!(different, 0, "different={different}, maximum={maximum}");
    }
    #[test]
    #[ignore = "requires QUATTRO_MONO_TONE_VARIANTS native exposure and combined tone lookups"]
    fn monochrome_edits_match_complete_native_lookup() {
        let root =
            std::path::PathBuf::from(std::env::var_os("QUATTRO_MONO_TONE_VARIANTS").unwrap());
        let profile = Profile {
            start: -1.17,
            end: 1.55,
            first_slope: 3.25,
            second_slope: 2.46,
            join: 0.1,
        };
        for (name, exposure, controls) in [
            ("minus1", -1., tone::Controls::default()),
            ("plus1", 1., tone::Controls::default()),
            (
                "combined",
                0.5,
                tone::Controls {
                    contrast: 0.5,
                    shadows: 0.5,
                    highlights: -0.5,
                },
            ),
        ] {
            let actual = curve_table(profile, exposure, controls, true);
            let bytes =
                std::fs::read(root.join(format!("monochrome-tone-{name}/tone.u16"))).unwrap();
            let expected: &[u16] = bytemuck::cast_slice(&bytes);
            assert_eq!(expected.len(), actual.len() * 3);
            let maximum = actual
                .iter()
                .cycle()
                .zip(expected)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            let different = actual
                .iter()
                .cycle()
                .zip(expected)
                .filter(|(a, b)| **a != **b)
                .count();
            eprintln!("Mono editedtone {name} different={different} maximum={maximum}");
            assert_eq!(different, 0);
        }
    }
}
