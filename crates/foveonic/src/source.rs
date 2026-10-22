//! Owns original sensor storage layouts and typed camera metadata.
//!
//! Adapted from the reference implementation's source types and camera checks.

use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const SENSOR_WIDTH: usize = 4928;
const SENSOR_HEIGHT: usize = 3264;

/// Processing selection belongs to input and recipes, never pixel operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessingFamily {
    Merrill,
    Classic,
    Quattro,
}

impl ProcessingFamily {
    /// Identify processing from the lightweight capture properties before RAW decoding.
    pub fn from_camera_model(model: &str) -> Option<Self> {
        family(Some(model))
    }
}

/// Original CAMF numeric categories, retained separately from widened values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[repr(u32)]
#[serde(into = "u32")]
pub enum MatrixEncoding {
    Int16 = 0,
    Int32 = 1,
    Uint32 = 2,
    Float32 = 3,
    Uint8 = 5,
    Uint16 = 6,
}

impl From<MatrixEncoding> for u32 {
    fn from(value: MatrixEncoding) -> Self {
        value as u32
    }
}

impl TryFrom<u32> for MatrixEncoding {
    type Error = &'static str;

    fn try_from(code: u32) -> std::result::Result<Self, Self::Error> {
        match code {
            0 => Ok(Self::Int16),
            1 => Ok(Self::Int32),
            2 => Ok(Self::Uint32),
            3 => Ok(Self::Float32),
            5 => Ok(Self::Uint8),
            6 => Ok(Self::Uint16),
            _ => Err("unsupported camera matrix encoding"),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Matrix {
    pub encoding: MatrixEncoding,
    pub representation: &'static str,
    pub dimensions: Vec<usize>,
    pub values: Vec<f64>,
}

/// Preserve camera order because named color modes use it to select tone rows.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(transparent)]
pub struct PropertyList(Vec<(String, String)>);

impl PropertyList {
    pub fn new(entries: Vec<(String, String)>) -> Result<Self> {
        for (i, (name, _)) in entries.iter().enumerate() {
            if entries[..i].iter().any(|(previous, _)| previous == name) {
                return Err("duplicate camera property name".into());
            }
        }
        Ok(Self(entries))
    }

    pub fn get(&self, name: &str) -> Option<&String> {
        self.0
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|(name, _)| name.as_str())
    }

    pub fn index(&self, name: &str) -> Option<usize> {
        self.0.iter().position(|(key, _)| key == name)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Metadata {
    pub width: usize,
    pub height: usize,
    pub rotation: u32,
    /// Version-4 processing words, distinct from camera identity and compression.
    pub processing_version: Option<[u16; 4]>,
    pub properties: BTreeMap<String, String>,
    pub matrices: BTreeMap<String, Matrix>,
    pub lists: BTreeMap<String, PropertyList>,
}

/// Optional photographic choices recorded in the version-4 file header.
/// Noise and smoothness values retain the normalized reference control scale.
#[derive(Clone, Debug, Default)]
pub struct CaptureAdjustments {
    pub white_balance: Option<u16>,
    pub color_mode: Option<u16>,
    pub exposure: Option<f32>,
    pub contrast: Option<f32>,
    pub shadows: Option<f32>,
    pub highlights: Option<f32>,
    pub saturation: Option<f32>,
    pub sharpness: Option<f32>,
    pub fill_light: Option<f32>,
    pub highlight_correction: Option<f32>,
    pub chroma_noise: Option<f32>,
    pub luminance_noise: Option<f32>,
    pub banding_noise: Option<f32>,
    pub smoothness: Option<f32>,
    pub mono_exposure: Option<f32>,
    pub mono_contrast: Option<f32>,
    pub mono_shadows: Option<f32>,
    pub mono_highlights: Option<f32>,
    pub mono_sharpness: Option<f32>,
    pub mono_fill_light: Option<f32>,
    pub mono_luminance_noise: Option<f32>,
    pub mono_banding_noise: Option<f32>,
    pub mono_smoothness: Option<f32>,
    /// Nonnegative RGB proportions, normalized from the saved UInt16 weights.
    pub mono_mix: Option<[f32; 3]>,
}

// The header stores unit monochrome channel gain as this UInt16 code.
const MONO_MIX_UNITY_CODE: f32 = 32768.;

impl CaptureAdjustments {
    /// The property-only capture path can resolve controls before RAW decoding.
    pub fn from_properties(properties: &BTreeMap<String, String>) -> Result<Self> {
        let number = |name: &str| -> Result<Option<f32>> {
            properties
                .get(&format!(
                    "{}{name}",
                    crate::x3f::adjustments::PROPERTY_PREFIX
                ))
                .map(|value| {
                    let value: f32 = value.parse()?;
                    if !value.is_finite() {
                        return Err("nonfinite recorded X3F adjustment".into());
                    }
                    Ok(value)
                })
                .transpose()
        };
        let preset = |name: &str| -> Result<Option<u16>> {
            properties
                .get(&format!(
                    "{}{name}",
                    crate::x3f::adjustments::PROPERTY_PREFIX
                ))
                .map(|value| value.parse().map_err(Into::into))
                .transpose()
        };
        let mix = [
            preset("MONO_MIX_RED")?,
            preset("MONO_MIX_GREEN")?,
            preset("MONO_MIX_BLUE")?,
        ];
        let mono_mix = if mix.iter().any(Option::is_some) {
            let values = mix.map(|v| v.map(f32::from).unwrap_or(MONO_MIX_UNITY_CODE / 3.));
            let sum: f32 = values.iter().sum();
            Some(if sum > 0. {
                values.map(|v| v / sum)
            } else {
                [1. / 3.; 3]
            })
        } else {
            None
        };
        Ok(Self {
            white_balance: preset("WHITE_BALANCE")?,
            color_mode: preset("COLOR_MODE")?,
            exposure: number("EXPOSURE")?,
            contrast: number("CONTRAST")?,
            shadows: number("SHADOWS")?,
            highlights: number("HIGHLIGHTS")?,
            saturation: number("SATURATION")?,
            sharpness: number("SHARPNESS")?,
            fill_light: number("FILL_LIGHT")?,
            highlight_correction: number("HIGHLIGHT_CORRECTION")?,
            chroma_noise: number("CHROMA_NOISE")?,
            luminance_noise: number("LUMINANCE_NOISE")?,
            banding_noise: number("BANDING_NOISE")?,
            smoothness: number("SMOOTHNESS")?,
            mono_exposure: number("MONO_EXPOSURE")?,
            mono_contrast: number("MONO_CONTRAST")?,
            mono_shadows: number("MONO_SHADOWS")?,
            mono_highlights: number("MONO_HIGHLIGHTS")?,
            mono_sharpness: number("MONO_SHARPNESS")?,
            mono_fill_light: number("MONO_FILL_LIGHT")?,
            mono_luminance_noise: number("MONO_LUMINANCE_NOISE")?,
            mono_banding_noise: number("MONO_BANDING_NOISE")?,
            mono_smoothness: number("MONO_SMOOTHNESS")?,
            mono_mix,
        })
    }
}

impl Metadata {
    /// Read saved numeric choices without substituting family defaults.
    pub fn capture_adjustments(&self) -> Result<CaptureAdjustments> {
        CaptureAdjustments::from_properties(&self.properties)
    }

    pub fn processing_family(&self) -> Result<ProcessingFamily> {
        family(self.properties.get("CAMMODEL").map(String::as_str))
            .ok_or_else(|| "unsupported camera family".into())
    }

    /// Camera-ordered illuminants. Classic monochrome is a separate rendering choice.
    pub fn white_balance_presets(&self) -> Vec<&str> {
        let classic = self.processing_family().ok() == Some(ProcessingFamily::Classic);
        self.white_balance_catalogue()
            .map(|list| {
                list.names()
                    .filter(|name| !classic || *name != "Monochrome")
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Camera-ordered modes, including the fixed Standard mode of early cached profiles.
    pub fn color_mode_presets(&self) -> Vec<&str> {
        if let Some(list) = self.lists.get("ColorModeCompensations") {
            list.names().collect()
        } else if self.processing_family().ok() == Some(ProcessingFamily::Classic) {
            vec!["Standard"]
        } else {
            Vec::new()
        }
    }

    fn white_balance_catalogue(&self) -> Option<&PropertyList> {
        let name = if self.processing_family().ok() == Some(ProcessingFamily::Classic)
            && self.lists.contains_key("DP1_WhiteBalanceGains")
        {
            "DP1_WhiteBalanceGains"
        } else {
            "WhiteBalanceGains"
        };
        self.lists.get(name).or_else(|| {
            (self.processing_family().ok() == Some(ProcessingFamily::Classic))
                .then(|| self.lists.get("WhiteBalanceCorrections"))
                .flatten()
        })
    }

    /// Resolve the recorded WB label to this file's canonical CAMF preset name.
    pub fn recorded_white_balance(&self) -> Result<String> {
        let recorded = self
            .properties
            .get("WB_DESC")
            .ok_or("missing recorded white balance (WB_DESC)")?
            .trim();
        // Merrill property labels call the CAMF Sunlight preset Daylight.
        let name = if recorded.eq_ignore_ascii_case("Daylight") {
            "Sunlight"
        } else {
            recorded
        };
        self.white_balance_catalogue()
            .and_then(|list| {
                list.names()
                    .find(|key| key.trim().eq_ignore_ascii_case(name))
            })
            .map(str::to_owned)
            .ok_or_else(|| format!("unavailable recorded white balance: {name}").into())
    }

    pub fn recorded_color_mode(&self) -> Result<String> {
        if self.processing_family().ok() == Some(ProcessingFamily::Classic)
            && !self.properties.contains_key("CM_DESC")
            && !self.lists.contains_key("ColorModeCompensations")
        {
            return Ok("Standard".into());
        }
        let recorded = self
            .properties
            .get("CM_DESC")
            .ok_or("missing recorded color mode (CM_DESC)")?
            .trim();
        self.recorded_preset("ColorModeCompensations", recorded, "color mode")
    }

    fn recorded_preset(&self, list: &str, name: &str, kind: &str) -> Result<String> {
        self.lists
            .get(list)
            .and_then(|list| {
                list.names()
                    .find(|key| key.trim().eq_ignore_ascii_case(name))
            })
            .map(str::to_owned)
            .ok_or_else(|| format!("unavailable recorded {kind}: {name}").into())
    }

    /// Camera matrix names are case-insensitive. Stored spelling stays intact.
    pub fn matrix(&self, name: &str) -> Option<&Matrix> {
        self.matrices
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, matrix)| matrix)
    }

    pub fn vector(&self, name: &str) -> Result<&[f64]> {
        self.matrix(name)
            .map(|m| m.values.as_slice())
            .ok_or_else(|| format!("missing CAMF matrix {name}").into())
    }

    pub fn scalar(&self, name: &str) -> Result<f64> {
        let values = self.vector(name)?;
        if values.len() != 1 {
            return Err(format!("{name} is not a scalar").into());
        }
        Ok(values[0])
    }

    pub fn fixed<const N: usize>(&self, name: &str) -> Result<[f64; N]> {
        self.vector(name)?
            .try_into()
            .map_err(|_| format!("{name} has wrong dimensions").into())
    }
}

/// Read photographic capture properties without loading sensor samples or CAMF calibration.
/// Rendering still needs a full `Source`. This lighter read lets capture details
/// appear while sensor samples and calibration are still loading.
pub fn read_capture_properties(path: impl AsRef<Path>) -> Result<BTreeMap<String, String>> {
    let mut file = std::fs::File::open(path)?;
    let properties = crate::x3f::capture::read(&mut file)?;
    if family(properties.get("CAMMODEL").map(String::as_str)).is_none() {
        return Err("unsupported camera family".into());
    }
    Ok(properties)
}

/// Physical ordering of immutable samples returned by `Source::samples`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceStorage {
    /// One R, G, B triplet per sensor location.
    InterleavedRgb,
    /// One R, G, B00, B01, B10, B11 group per low-resolution cell.
    PackedRgbBluePhases,
}

pub struct Source {
    pub metadata: std::sync::Arc<Metadata>,
    /// Immutable samples in original camera code units, ordered by `storage`.
    pub(crate) pixels: Vec<f32>,
    pub(crate) storage: SourceStorage,
    pub camf: Vec<u8>,
    pub(crate) timings: Vec<crate::profile::Timing>,
}

impl Source {
    /// Physical sample ordering.
    pub fn storage(&self) -> SourceStorage {
        self.storage
    }

    /// Number of stored triplets or six-sample cells along each axis.
    pub fn storage_extent(&self) -> [usize; 2] {
        let divisor = match self.storage {
            SourceStorage::InterleavedRgb => 1,
            SourceStorage::PackedRgbBluePhases => 2,
        };
        [
            self.metadata.width / divisor,
            self.metadata.height / divisor,
        ]
    }

    /// Original camera codes ordered according to `storage()`. Samples remain immutable.
    pub fn samples(&self) -> &[f32] {
        &self.pixels
    }

    /// Keep reading separate from decoding so cached-file speed is not mistaken
    /// for decoder speed. Read time includes opening, checks, allocation, and
    /// reading all bytes. Parallel-plane timings overlap the decode total, so
    /// adding them would count the same work twice. A first process read may
    /// still hit the filesystem cache and does not establish SD-card speed.
    pub fn load_timings(&self) -> &[crate::profile::Timing] {
        &self.timings
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let source = crate::x3f::read(path.as_ref())?;
        match source.metadata.processing_family()? {
            ProcessingFamily::Merrill => {
                if source.metadata.width != SENSOR_WIDTH || source.metadata.height != SENSOR_HEIGHT
                {
                    return Err("unsupported Merrill full-sensor geometry".into());
                }
            }
            ProcessingFamily::Classic => {
                let model = source
                    .metadata
                    .properties
                    .get("CAMMODEL")
                    .map(|m| m.trim().to_ascii_uppercase());
                let expected = if matches!(model.as_deref(), Some("SIGMA SD9" | "SIGMA SD10")) {
                    [2304, 1531]
                } else {
                    [2688, 1792]
                };
                if [source.metadata.width, source.metadata.height] != expected {
                    return Err("unsupported Classic full-sensor geometry".into());
                }
            }
            ProcessingFamily::Quattro => {
                if source.storage != SourceStorage::PackedRgbBluePhases {
                    return Err("unsupported Quattro sensor layout".into());
                }
            }
        }
        Ok(source)
    }
}

fn family(model: Option<&str>) -> Option<ProcessingFamily> {
    if is_merrill(model) {
        Some(ProcessingFamily::Merrill)
    } else if matches!(
        model.map(|m| m.trim().to_ascii_uppercase()).as_deref(),
        Some(
            "SIGMA SD9"
                | "SIGMA SD10"
                | "SIGMA SD14"
                | "SIGMA SD15"
                | "SIGMA DP1"
                | "SIGMA DP1S"
                | "SIGMA DP1X"
                | "SIGMA DP2"
                | "SIGMA DP2S"
                | "SIGMA DP2X"
        )
    ) {
        Some(ProcessingFamily::Classic)
    } else if matches!(
        model.map(|m| m.trim().to_ascii_uppercase()).as_deref(),
        Some(
            "SIGMA DP0 QUATTRO"
                | "SIGMA DP1 QUATTRO"
                | "SIGMA DP2 QUATTRO"
                | "SIGMA DP3 QUATTRO"
                | "SIGMA SD QUATTRO"
                | "SIGMA SD QUATTRO H"
                | "SD QUATTRO"
                | "SD QUATTRO H"
        )
    ) {
        Some(ProcessingFamily::Quattro)
    } else {
        None
    }
}

fn is_merrill(model: Option<&str>) -> bool {
    matches!(
        model,
        Some("SIGMA DP1 Merrill" | "SIGMA DP2 Merrill" | "SIGMA DP3 Merrill" | "SIGMA SD1 Merrill")
    )
}

#[cfg(test)]
#[path = "source_test.rs"]
mod tests;
