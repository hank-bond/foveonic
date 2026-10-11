//! Owns decoded Merrill samples and typed metadata without photographic adjustments.
//!
//! Adapted from Foveonic's source types and Merrill camera checks.

use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

const SENSOR_WIDTH: usize = 4928;
const SENSOR_HEIGHT: usize = 3264;

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

pub struct Source {
    pub metadata: std::sync::Arc<Metadata>,
    /// Full-sensor, interleaved layer samples in original camera code units.
    pub pixels: Vec<f32>,
    pub camf: Vec<u8>,
    pub(crate) timings: Vec<crate::profile::Timing>,
}

impl Source {
    /// Host wall timings from loading; concurrent plane timings overlap and must not be summed.
    pub fn load_timings(&self) -> &[crate::profile::Timing] {
        &self.timings
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let source = crate::x3f::read(path.as_ref())?;
        if !is_merrill(
            source
                .metadata
                .properties
                .get("CAMMODEL")
                .map(String::as_str),
        ) {
            return Err("unsupported camera family".into());
        }
        if source.metadata.width != SENSOR_WIDTH || source.metadata.height != SENSOR_HEIGHT {
            return Err("unsupported Merrill full-sensor geometry".into());
        }
        Ok(source)
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
