//! Shares photographic CLI option parsing between conversion and the completed-processing profiler.

use foveonic::{
    profile::Mode,
    render::{RepairThreshold, Settings},
    source::Result,
};
use std::{ffi::OsString, path::PathBuf};

pub(crate) fn parse_settings(
    args: impl Iterator<Item = OsString>,
) -> Result<(Settings, Vec<PathBuf>, Mode)> {
    let mut settings = Settings::new("", "");
    let mut paths = Vec::new();
    let mut profiling = Mode::Off;
    let mut args = args;
    while let Some(arg) = args.next() {
        if arg == "--reconstruction" {
            settings.reconstruction.get_or_insert_with(Default::default);
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
            "--exposure" => settings.exposure = value.parse()?,
            "--contrast" => settings.tone.contrast = value.parse()?,
            "--shadows" => settings.tone.shadows = value.parse()?,
            "--highlights" => settings.tone.highlights = value.parse()?,
            "--highlight-threshold" => {
                settings.highlight_threshold_ev = if value == "auto" {
                    None
                } else {
                    Some(value.parse()?)
                };
            }
            "--saturation" => settings.saturation = value.parse()?,
            "--sensor-gain" => settings.sensor_gain = value.parse()?,
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
                    .get_or_insert_with(Default::default)
                    .overrides
                    .set(parameter, Some(value.parse()?));
            }
            "--chroma-noise" => {
                settings
                    .reconstruction
                    .get_or_insert_with(Default::default)
                    .chroma = value.parse()?
            }
            "--luma-noise" => {
                settings
                    .reconstruction
                    .get_or_insert_with(Default::default)
                    .luminance = value.parse()?
            }
            "--banding-noise" => {
                settings
                    .reconstruction
                    .get_or_insert_with(Default::default)
                    .banding = value.parse()?
            }
            "--detail" => {
                settings
                    .reconstruction
                    .get_or_insert_with(Default::default)
                    .detail = value.parse()?
            }
            "--sharpen" => settings.sharpen_amount = value.parse()?,
            "--size" => {
                let (width, height) = value.split_once('x').ok_or("size requires WIDTHxHEIGHT")?;
                settings.output_size = Some([width.parse()?, height.parse()?]);
            }
            _ => return Err(format!("unknown option {flag}").into()),
        }
    }
    settings.validate()?;
    Ok((settings, paths, profiling))
}
