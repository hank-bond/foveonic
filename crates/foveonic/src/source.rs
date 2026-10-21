//! Owns original equal-resolution sensor samples and typed camera metadata.
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
    pub properties: BTreeMap<String, String>,
    pub matrices: BTreeMap<String, Matrix>,
    pub lists: BTreeMap<String, PropertyList>,
}

impl Metadata {
    pub fn processing_family(&self) -> Result<ProcessingFamily> {
        family(self.properties.get("CAMMODEL").map(String::as_str))
            .ok_or_else(|| "unsupported camera family".into())
    }

    /// Camera-ordered white-balance choices resolved through the input profile.
    pub fn white_balance_presets(&self) -> Vec<&str> {
        self.white_balance_catalogue()
            .map(|list| list.names().collect())
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
        self.lists.get(name)
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

pub struct Source {
    pub metadata: std::sync::Arc<Metadata>,
    /// Full-sensor, interleaved layer samples in original camera code units.
    pub pixels: Vec<f32>,
    pub camf: Vec<u8>,
    pub(crate) timings: Vec<crate::profile::Timing>,
}

impl Source {
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
                if source.metadata.width != 2688 || source.metadata.height != 1792 {
                    return Err("unsupported Classic full-sensor geometry".into());
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
        model,
        Some("SIGMA SD15" | "SIGMA DP2" | "SIGMA DP1S" | "SIGMA DP1s")
    ) {
        Some(ProcessingFamily::Classic)
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
