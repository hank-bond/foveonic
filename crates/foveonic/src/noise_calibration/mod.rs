//! Reviews cap capture groups, persists one user calibration and gates learned defects by camera identity.

use crate::{
    nodes::sensor::noise_model,
    source::{self, ProcessingFamily, Result},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

mod fit;
mod training;

pub const MINIMUM_CAPTURES: usize = 7;
pub const RECOMMENDED_CAPTURES: usize = 14;
const FORMAT_VERSION: u32 = 1;
const MAXIMUM_BODY_DEFECTS: usize = 262144;

fn valid_serial(value: &str) -> bool {
    !value.is_empty()
        && !value.chars().all(|c| c == '0')
        && !["unknown", "n/a", "unavailable", "-1"]
            .iter()
            .any(|v| value.eq_ignore_ascii_case(v))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Condition {
    pub camera_model: String,
    pub serial: Option<String>,
    pub iso: u32,
    pub shutter_micros: u64,
}
impl Condition {
    pub fn from_properties(properties: &BTreeMap<String, String>) -> Result<Self> {
        let camera_model = properties
            .get("CAMMODEL")
            .ok_or("missing camera model")?
            .trim()
            .to_owned();
        if ProcessingFamily::from_camera_model(&camera_model) != Some(ProcessingFamily::Merrill) {
            return Err("Cap calibration supports Merrill X3Fs".into());
        }
        let iso = properties
            .get("ISO")
            .ok_or("missing camera ISO")?
            .parse::<u32>()?;
        let seconds = properties
            .get("SHUTTER")
            .ok_or("missing shutter speed")?
            .parse::<f64>()?;
        if iso == 0 || !seconds.is_finite() || seconds <= 0. || seconds > u64::MAX as f64 / 1e6 {
            return Err("invalid cap ISO or shutter speed".into());
        }
        let shutter_micros = (seconds * 1e6).round() as u64;
        if shutter_micros == 0 {
            return Err("shutter speed is below calibration precision".into());
        }
        let condition = Self {
            camera_model,
            serial: properties
                .get("CAMSERIAL")
                .map(|v| v.trim().to_owned())
                .filter(|v| valid_serial(v)),
            iso,
            shutter_micros,
        };
        condition.validate()?;
        Ok(condition)
    }
    fn validate(&self) -> Result<()> {
        if self.iso == 0
            || self.shutter_micros == 0
            || ProcessingFamily::from_camera_model(&self.camera_model)
                != Some(ProcessingFamily::Merrill)
            || self
                .serial
                .as_ref()
                .is_some_and(|s| !valid_serial(s) || s.trim() != s)
        {
            return Err("invalid calibration capture condition".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capture {
    pub path: PathBuf,
    pub condition: Condition,
}
impl Capture {
    pub fn read(path: impl AsRef<Path>) -> Result<Self> {
        let properties = source::read_capture_properties(&path)?;
        Self::from_properties(path, &properties)
    }
    pub fn from_properties(
        path: impl AsRef<Path>,
        properties: &BTreeMap<String, String>,
    ) -> Result<Self> {
        let path = path.as_ref().canonicalize()?;
        let condition = Condition::from_properties(properties)?;
        Ok(Self { path, condition })
    }
}

pub fn groups(captures: &[Capture]) -> BTreeMap<Condition, Vec<PathBuf>> {
    let mut groups: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for capture in captures {
        let paths = groups.entry(capture.condition.clone()).or_default();
        if !paths.contains(&capture.path) {
            paths.push(capture.path.clone());
        }
    }
    groups
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Defect {
    pub pixel: u32,
    pub channels: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SensorDefects {
    pub camera_model: String,
    pub serial: String,
    pub extent: [u32; 2],
    pub points: Vec<Defect>,
}

fn merge_defects(maps: Vec<SensorDefects>) -> Vec<SensorDefects> {
    let mut bodies = BTreeMap::<_, BTreeMap<u32, u32>>::new();
    for map in maps {
        let points = bodies
            .entry((map.camera_model, map.serial, map.extent))
            .or_default();
        for point in map.points {
            *points.entry(point.pixel).or_default() |= point.channels;
        }
    }
    bodies
        .into_iter()
        .map(|((camera_model, serial, extent), points)| SensorDefects {
            camera_model,
            serial,
            extent,
            points: points
                .into_iter()
                .map(|(pixel, channels)| Defect { pixel, channels })
                .collect(),
        })
        .collect()
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Calibration {
    version: u32,
    pub(crate) captures: Vec<Capture>,
    pub(crate) defects: Vec<SensorDefects>,
    pub(crate) model: noise_model::Model,
    #[serde(skip)]
    signature: std::sync::OnceLock<String>,
}
impl Calibration {
    pub fn captures(&self) -> &[Capture] {
        &self.captures
    }
    pub fn defects(&self) -> &[SensorDefects] {
        &self.defects
    }
    pub fn signature(&self) -> &str {
        use std::hash::Hasher;
        self.signature.get_or_init(|| {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            hash.write(&serde_json::to_vec(self).expect("serializable validated calibration"));
            format!("{:016x}", hash.finish())
        })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let profile: Self = serde_json::from_reader(std::fs::File::open(path)?)?;
        profile.validate()?;
        Ok(profile)
    }
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        self.validate()?;
        let path = path.as_ref();
        let parent = path
            .parent()
            .ok_or("calibration needs a parent directory")?;
        std::fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer(temporary.as_file_mut(), self)?;
        temporary.as_file_mut().sync_all()?;
        temporary.persist(path)?;
        Ok(())
    }
    fn validate(&self) -> Result<()> {
        if self.version != FORMAT_VERSION {
            return Err("unsupported noise calibration version".into());
        }
        self.model.validate()?;
        let groups = groups(&self.captures);
        if groups.values().any(|v| v.len() < MINIMUM_CAPTURES) || self.captures.is_empty() {
            return Err("calibration needs at least seven captures per condition".into());
        }
        for condition in groups.keys() {
            condition.validate()?;
        }
        let mut seen = std::collections::BTreeSet::new();
        for map in &self.defects {
            if !valid_serial(&map.serial)
                || !groups.keys().any(|c| {
                    c.camera_model == map.camera_model && c.serial.as_ref() == Some(&map.serial)
                })
                || !seen.insert((&map.camera_model, &map.serial, map.extent))
                || map.points.len() > MAXIMUM_BODY_DEFECTS
                || map.extent.contains(&0)
                || map.points.iter().any(|p| {
                    u64::from(p.pixel) >= u64::from(map.extent[0]) * u64::from(map.extent[1])
                        || p.channels == 0
                        || p.channels > 7
                })
                || map.points.windows(2).any(|p| p[0].pixel >= p[1].pixel)
            {
                return Err("invalid camera-specific defect map".into());
            }
        }
        Ok(())
    }
    pub(crate) fn matching_defects(&self, metadata: &source::Metadata) -> Option<&SensorDefects> {
        let camera_model = metadata.properties.get("CAMMODEL")?;
        let serial = metadata.properties.get("CAMSERIAL")?.trim();
        if !valid_serial(serial) {
            return None;
        }
        self.defects.iter().find(|map| {
            map.camera_model == *camera_model
                && map.serial == serial
                && map.extent == [metadata.width as u32, metadata.height as u32]
        })
    }
}

#[derive(Clone, Debug)]
pub struct TrainingProgress {
    pub condition: Condition,
    pub completed: usize,
    pub total: usize,
}

pub(crate) use training::train;

#[cfg(test)]
#[path = "profile_test.rs"]
mod tests;
