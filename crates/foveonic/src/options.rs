//! Shares photographic CLI option parsing between conversion and the completed-processing profiler.

use foveonic::{
    profile::Mode,
    render::{
        ColorFieldApproach, HighlightMode, MonochromeControls, Rendering, RepairThreshold,
        Settings, TiffColorSpace,
    },
    source::Result,
};
use std::{ffi::OsString, path::PathBuf};

pub(crate) fn parse_settings(
    args: impl Iterator<Item = OsString>,
) -> Result<(Settings, Vec<PathBuf>, Mode)> {
    parse_settings_with_noise_defaults(args, Default::default(), Settings::new("", ""))
}

/// Seed only unspecified noise fields from the selected camera's recipe.
/// Parsing each batch input separately preserves defaults in mixed-family jobs.
pub(crate) fn parse_settings_for_camera(
    args: impl Iterator<Item = OsString>,
    metadata: &foveonic::source::Metadata,
) -> Result<(Settings, Vec<PathBuf>, Mode)> {
    let args: Vec<_> = args.collect();
    let capabilities = foveonic::render::ControlCapabilities::from_metadata(metadata)?;
    let defaults = capabilities.default_noise_controls_for_metadata(metadata)?;
    let mut initial = Settings::new("", "");
    initial.rendering = capabilities.default_rendering(metadata)?;
    // Select the initial rendering before reading its saved tonal defaults.
    // The ordinary parser still validates and applies every explicit value.
    for (index, flag) in args.iter().enumerate() {
        match flag.to_str() {
            Some("--rendering") => match args.get(index + 1).and_then(|v| v.to_str()) {
                Some("color") => initial.rendering = Rendering::Color,
                Some("monochrome") => initial.rendering = Rendering::Monochrome,
                _ => {}
            },
            Some("--mono-mix" | "--mono-noise" | "--mono-banding" | "--mono-smoothness") => {
                initial.rendering = Rendering::Monochrome
            }
            _ => {}
        }
    }
    capabilities.apply_capture_defaults(&mut initial, metadata)?;
    let (mut settings, paths, profiling) =
        if capabilities.family == foveonic::source::ProcessingFamily::Quattro {
            parse_settings_with_noise_defaults(args.into_iter(), defaults, initial)?
        } else {
            parse_settings(args.into_iter())?
        };
    if metadata.processing_family()? == foveonic::source::ProcessingFamily::Quattro {
        if capabilities.white_balance_active(&settings) && settings.white_balance.is_empty() {
            settings.white_balance =
                foveonic::render::ControlCapabilities::from_metadata(metadata)?
                    .default_white_balance(metadata)?;
        }
        if capabilities.color_mode_active(&settings) && settings.color_mode.is_empty() {
            settings.color_mode = foveonic::render::ControlCapabilities::from_metadata(metadata)?
                .default_color_mode(metadata)?;
        }
    }
    Ok((settings, paths, profiling))
}

fn parse_settings_with_noise_defaults(
    args: impl Iterator<Item = OsString>,
    noise_defaults: foveonic::noise::Controls,
    mut settings: Settings,
) -> Result<(Settings, Vec<PathBuf>, Mode)> {
    let mut paths = Vec::new();
    let mut profiling = Mode::Off;
    let mut args = args;
    while let Some(arg) = args.next() {
        if arg == "--reconstruction" {
            settings.reconstruction.get_or_insert(noise_defaults);
            continue;
        }
        if arg == "--repair" {
            settings.repair = Some(RepairThreshold::Camera);
            continue;
        }
        if arg == "--no-repair" {
            settings.repair = None;
            continue;
        }
        if arg == "--despeckle" || arg == "--no-despeckle" {
            settings.despeckle = arg == "--despeckle";
            continue;
        }
        if arg == "--highlight-restoration" || arg == "--no-highlight-restoration" {
            settings.highlight_restoration = arg == "--highlight-restoration";
            continue;
        }
        if arg == "--row-correction" || arg == "--no-row-correction" {
            settings.row_correction = arg == "--row-correction";
            continue;
        }
        if !arg.to_str().is_some_and(|s| s.starts_with("--")) {
            paths.push(arg.into());
            continue;
        }
        let flag = arg.to_str().ok_or("option must be UTF-8")?;
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?
            .into_string()
            .map_err(|_| "setting must be UTF-8")?;
        match flag {
            "--profile" => profiling = value.parse()?,
            "--wb" => settings.white_balance = value,
            "--mode" => settings.color_mode = value,
            "--rendering" => {
                settings.rendering = match value.as_str() {
                    "color" => Rendering::Color,
                    "monochrome" => Rendering::Monochrome,
                    _ => return Err("rendering must be color or monochrome".into()),
                }
            }
            "--mono-mix" => {
                let values: Vec<f32> = value
                    .split(',')
                    .map(str::parse)
                    .collect::<std::result::Result<_, _>>()?;
                settings.monochrome.mix = MonochromeControls::normalized_mix(
                    values.try_into().map_err(|_| "mono mix requires R,G,B")?,
                )?;
                settings.rendering = Rendering::Monochrome;
            }
            "--mono-noise" => {
                settings.monochrome.noise = value.parse()?;
                settings.rendering = Rendering::Monochrome;
            }
            "--mono-banding" => {
                settings.monochrome.banding = value.parse()?;
                settings.rendering = Rendering::Monochrome;
            }
            "--exposure" => settings.exposure = value.parse()?,
            "--contrast" => settings.tone.contrast = value.parse()?,
            "--shadows" => settings.tone.shadows = value.parse()?,
            "--highlights" => settings.tone.highlights = value.parse()?,
            "--highlight-amount" => settings.highlight_amount = Some(value.parse()?),
            "--highlight-threshold" => {
                settings.highlight_threshold_ev = if value == "auto" {
                    None
                } else {
                    Some(value.parse()?)
                };
            }
            "--highlight-mode" => {
                settings.highlight_mode = match value.as_str() {
                    "classic" => HighlightMode::Classic,
                    "color-field-local" | "color-field-global" => {
                        settings.color_field.approach = if value == "color-field-local" {
                            ColorFieldApproach::LocalGradient
                        } else {
                            ColorFieldApproach::Faithful
                        };
                        HighlightMode::ColorField
                    }
                    _ => return Err(
                        "highlight mode must be classic, color-field-local, or color-field-global"
                            .into(),
                    ),
                }
            }
            "--color-field-selectivity" => settings.color_field.selectivity = value.parse()?,
            "--saturation" => settings.saturation = value.parse()?,
            "--color-dq-gain" => settings.color_dq_gain = Some(value.parse()?),
            "--repair-badness" => settings.repair = Some(RepairThreshold::Manual(value.parse()?)),
            "--noise-override" => {
                let (key, value) = value
                    .split_once('=')
                    .ok_or("noise override requires KEY=VALUE")?;
                let parameter = foveonic::noise::Parameter::ALL
                    .into_iter()
                    .find(|p| p.key() == key)
                    .ok_or("unknown noise override parameter")?;
                settings
                    .reconstruction
                    .get_or_insert(noise_defaults)
                    .overrides
                    .set(parameter, Some(value.parse()?));
            }
            "--chroma-noise" => {
                settings.reconstruction.get_or_insert(noise_defaults).chroma = value.parse()?
            }
            "--luma-noise" => {
                settings
                    .reconstruction
                    .get_or_insert(noise_defaults)
                    .luminance = value.parse()?
            }
            "--banding-noise" => {
                settings
                    .reconstruction
                    .get_or_insert(noise_defaults)
                    .banding = value.parse()?
            }
            "--mono-smoothness" => {
                settings.monochrome_smoothness = Some(value.parse()?);
                settings.rendering = Rendering::Monochrome;
            }
            "--detail" => {
                settings.reconstruction.get_or_insert(noise_defaults).detail = value.parse()?
            }
            "--sharpen" => {
                settings.sharpen_amount = match value.as_str() {
                    "auto" => None,
                    "off" => Some(0.),
                    _ => Some(value.parse()?),
                }
            }
            "--tiff-color-space" => {
                settings.tiff_color_space = match value.as_str() {
                    "camera" => None,
                    "srgb" => Some(TiffColorSpace::Srgb),
                    "adobe-rgb" => Some(TiffColorSpace::AdobeRgb),
                    "prophoto-rgb" => Some(TiffColorSpace::ProPhotoRgb),
                    _ => {
                        return Err(
                            "TIFF color space must be camera, srgb, adobe-rgb, or prophoto-rgb"
                                .into(),
                        )
                    }
                }
            }
            "--size" => {
                let (width, height) = value.split_once('x').ok_or("size requires WIDTHxHEIGHT")?;
                settings.output_size = Some([width.parse()?, height.parse()?]);
            }
            _ => return Err(format!("unknown option {flag}").into()),
        }
    }
    if settings.is_monochrome() {
        // Active WB and color-mode requirements depend on the loaded family.
        settings.validate_values()?;
    } else {
        settings.validate()?;
    }
    Ok((settings, paths, profiling))
}

#[cfg(test)]
mod color_field_tests {
    use super::*;
    #[test]
    fn three_highlight_presets_select_shared_color_field_configurations() {
        for (name, mode, approach) in [
            (
                "classic",
                HighlightMode::Classic,
                ColorFieldApproach::Faithful,
            ),
            (
                "color-field-local",
                HighlightMode::ColorField,
                ColorFieldApproach::LocalGradient,
            ),
            (
                "color-field-global",
                HighlightMode::ColorField,
                ColorFieldApproach::Faithful,
            ),
        ] {
            let (settings, _, _) = parse_settings(
                [
                    "--wb",
                    "Sunlight",
                    "--mode",
                    "Standard",
                    "--highlight-mode",
                    name,
                ]
                .into_iter()
                .map(OsString::from),
            )
            .unwrap();
            assert_eq!(settings.highlight_mode, mode);
            assert_eq!(settings.color_field.approach, approach);
            assert!(settings.highlight_restoration);
            assert_eq!(settings.highlight_threshold_ev, None);
        }
    }

    #[test]
    fn color_field_cli_bounds_selectivity_and_preserves_strength_across_methods() {
        for value in ["auto", "-0.001", "1.001", "NaN", "inf"] {
            assert!(
                parse_settings(
                    [
                        "--wb",
                        "Sunlight",
                        "--mode",
                        "Standard",
                        "--color-field-selectivity",
                        value
                    ]
                    .into_iter()
                    .map(OsString::from)
                )
                .is_err(),
                "{value}"
            );
        }
        for method in ["classic", "color-field-local", "color-field-global"] {
            for (value, expected) in [("0", 0.), ("0.5", 0.5), ("1", 1.)] {
                let (s, _, _) = parse_settings(
                    [
                        "--wb",
                        "Sunlight",
                        "--mode",
                        "Standard",
                        "--color-field-selectivity",
                        value,
                        "--highlight-mode",
                        method,
                    ]
                    .into_iter()
                    .map(OsString::from),
                )
                .unwrap();
                assert_eq!(s.color_field.selectivity, expected);
            }
        }
        for flag in [
            "--color-field-restraint",
            "--color-field-cutoff",
            "--color-field-donation",
            "--color-field-agreement",
            "--color-field-boundary",
            "--color-field-area",
            "--color-field-approach",
        ] {
            assert!(parse_settings(
                ["--wb", "Sunlight", "--mode", "Standard", flag, "1"]
                    .into_iter()
                    .map(OsString::from)
            )
            .is_err());
        }
    }

    #[test]
    fn color_field_cli_selects_shared_controls_without_changing_classic_threshold() {
        let args = [
            "--wb",
            "Sunlight",
            "--mode",
            "Standard",
            "--highlight-mode",
            "color-field-global",
            "--highlight-threshold",
            "1.25",
            "--color-field-selectivity",
            "0.5",
        ];
        let (s, paths, _) = parse_settings(args.into_iter().map(OsString::from)).unwrap();
        assert!(paths.is_empty());
        assert_eq!(s.highlight_mode, HighlightMode::ColorField);
        assert_eq!(s.highlight_threshold_ev, Some(1.25));
        assert_eq!(s.color_field.selectivity, 0.5);
        assert_eq!(s.color_field.approach, ColorFieldApproach::Faithful);
        assert!(parse_settings(
            [
                "--wb",
                "Sunlight",
                "--mode",
                "Standard",
                "--color-field-donation",
                "2"
            ]
            .into_iter()
            .map(OsString::from)
        )
        .is_err());
    }
}
