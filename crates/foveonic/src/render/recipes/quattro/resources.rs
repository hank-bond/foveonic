//! Loads bundled or explicitly overridden blue calibration maps and prepares aperture/focus blends.
//!
//! Equations and arithmetic order follow the reference implementation's
//! LoadBSpatialGainTable and distinct two-map/four-map BlendSPGMaps overloads.

use crate::source::{Metadata, Result};
use std::{collections::BTreeMap, io::Read, path::Path};

const FIRST_CAMERA: u16 = 80;
const LAST_CAMERA: u16 = 83;
const MAPS_PER_CAMERA: u16 = 6;
// Reader allocation limit. The inspected resource payloads are 184 by 116.
const MAXIMUM_MAP_SAMPLES: usize = 256 * 256;

const DEFAULT_APERTURE: f64 = 8.;
// ScaleENC blends between resource banks at encoder positions 0 and 31.
// Values outside that interval retain the reference's extrapolation.
const FOCUS_ENCODER_ENDPOINT: f32 = 31.;

// Recovered Float32 knots in reciprocal f-number, corresponding to f/2.8, f/4
// and f/8. Keep the literal interval width and reciprocal below rather than
// recomputing them. Their rounding and operation order are part of the blend.
const INVERSE_F2_8: f32 = 0.35714287;
const INVERSE_F4: f32 = 0.25;
const INVERSE_F8: f32 = 0.125;
const INVERSE_F2_8_TO_F4_INTERVAL: f32 = 0.107142866;
const INVERSE_F4_TO_F8_INTERVAL_RECIPROCAL: f32 = 8.;

// Files 1..3 are the f/2.8, f/4 and f/8 maps at encoder zero.
// Files 4..6 have the same aperture order at encoder 31. Indices are zero-based.
const F2_8_FOCUS_PAIR: [usize; 2] = [0, 3];
const F2_8_TO_F4_MAPS: [usize; 4] = [0, 1, 3, 4];
const F4_TO_F8_MAPS: [usize; 4] = [1, 2, 4, 5];

#[derive(Clone)]
pub(crate) struct GainMap {
    pub(crate) extent: [u32; 2],
    pub(crate) values: Vec<f32>,
}

/// An owned snapshot of bundled or caller-supplied resources. No application path or
/// process working directory participates in loading or recipe construction.
pub(crate) struct ResourceDirectory {
    maps: BTreeMap<u16, Vec<GainMap>>,
}

impl ResourceDirectory {
    /// Exact imported map bytes travel with the processor on every platform.
    pub(crate) fn bundled() -> Result<Self> {
        macro_rules! camera {
            ($id:literal) => {
                (
                    $id,
                    [
                        include_bytes!(concat!(
                            env!("CARGO_MANIFEST_DIR"),
                            "/assets/quattro/C",
                            stringify!($id),
                            "_1"
                        )) as &[u8],
                        include_bytes!(concat!(
                            env!("CARGO_MANIFEST_DIR"),
                            "/assets/quattro/C",
                            stringify!($id),
                            "_2"
                        )) as &[u8],
                        include_bytes!(concat!(
                            env!("CARGO_MANIFEST_DIR"),
                            "/assets/quattro/C",
                            stringify!($id),
                            "_3"
                        )) as &[u8],
                        include_bytes!(concat!(
                            env!("CARGO_MANIFEST_DIR"),
                            "/assets/quattro/C",
                            stringify!($id),
                            "_4"
                        )) as &[u8],
                        include_bytes!(concat!(
                            env!("CARGO_MANIFEST_DIR"),
                            "/assets/quattro/C",
                            stringify!($id),
                            "_5"
                        )) as &[u8],
                        include_bytes!(concat!(
                            env!("CARGO_MANIFEST_DIR"),
                            "/assets/quattro/C",
                            stringify!($id),
                            "_6"
                        )) as &[u8],
                    ],
                )
            };
        }
        let mut maps = BTreeMap::new();
        for (camera, files) in [camera!(80), camera!(81), camera!(82), camera!(83)] {
            maps.insert(
                camera,
                files.into_iter().map(decode).collect::<Result<Vec<_>>>()?,
            );
        }
        Ok(Self { maps })
    }

    pub(crate) fn new(directory: &Path) -> Result<Self> {
        if !directory.is_dir() {
            return Err("blue calibration resource directory does not exist".into());
        }
        let mut maps = BTreeMap::new();
        for camera in FIRST_CAMERA..=LAST_CAMERA {
            let mut camera_maps = Vec::new();
            for index in 1..=MAPS_PER_CAMERA {
                let path = directory.join(format!("C{camera}_{index}"));
                match std::fs::metadata(&path) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error.into()),
                    Ok(_) => {}
                }
                let map = load(&path)
                    .map_err(|error| format!("blue calibration {}: {error}", path.display()))?;
                if camera_maps
                    .first()
                    .is_some_and(|first: &GainMap| first.extent != map.extent)
                {
                    return Err(
                        format!("blue calibration geometry differs for camera {camera}").into(),
                    );
                }
                camera_maps.push(map);
            }
            if camera_maps.len() == usize::from(MAPS_PER_CAMERA) {
                maps.insert(camera, camera_maps);
            }
        }
        Ok(Self { maps })
    }

    /// Camera identifiers come from metadata, independently of container format.
    /// The supplied catalogue has no maps for camera 40 (SD Quattro); that is a
    /// legitimate absence, distinct from an incomplete supplied directory.
    pub(crate) fn prepare(&self, metadata: &Metadata) -> Result<Option<GainMap>> {
        let camera = integer(metadata.scalar("CAMERAID")?, "camera identifier")?;
        if !(FIRST_CAMERA..=LAST_CAMERA).contains(&camera) {
            return Ok(None);
        }
        let scalar = |name, fallback| -> Result<f64> {
            if metadata.matrix(name).is_some() {
                metadata.scalar(name)
            } else {
                Ok(fallback)
            }
        };
        let aperture = scalar("CaptureAperture", DEFAULT_APERTURE)? as f32;
        let focus = integer(scalar("ScaleENC", 0.)?, "focus encoder")?;
        self.selected(camera, aperture, focus)
    }

    fn selected(&self, camera: u16, aperture: f32, focus: u16) -> Result<Option<GainMap>> {
        if !(FIRST_CAMERA..=LAST_CAMERA).contains(&camera) {
            return Ok(None);
        }
        let maps = self
            .maps
            .get(&camera)
            .ok_or_else(|| format!("missing complete blue calibration maps for camera {camera}"))?;
        if !aperture.is_finite() || aperture <= 0. {
            return Err("invalid blue calibration aperture".into());
        }
        let inverse = 1. / aperture;
        let values = if inverse >= INVERSE_F2_8 {
            let [first, second] = F2_8_FOCUS_PAIR.map(|index| maps[index].values.as_slice());
            blend2(first, second, f32::from(focus))
        } else {
            let (indices, weight) = if inverse >= INVERSE_F4 {
                (
                    F2_8_TO_F4_MAPS,
                    (inverse - INVERSE_F4) / INVERSE_F2_8_TO_F4_INTERVAL,
                )
            } else {
                (
                    F4_TO_F8_MAPS,
                    (inverse - INVERSE_F8) * INVERSE_F4_TO_F8_INTERVAL_RECIPROCAL,
                )
            };
            let [a, b, c, d] = indices.map(|index| maps[index].values.as_slice());
            blend4(a, b, c, d, weight, f32::from(focus))
        };
        if values.iter().any(|value| !value.is_finite()) {
            return Err("nonfinite blended blue calibration".into());
        }
        Ok(Some(GainMap {
            extent: maps[0].extent,
            values,
        }))
    }
}

fn integer(value: f64, name: &str) -> Result<u16> {
    if !value.is_finite() || value < 0. || value > f64::from(u16::MAX) || value.fract() != 0. {
        return Err(format!("invalid blue calibration {name}").into());
    }
    Ok(value as u16)
}

fn load(path: &Path) -> Result<GainMap> {
    let maximum = MAXIMUM_MAP_SAMPLES * 4 + 4;
    let mut data = Vec::new();
    std::fs::File::open(path)?
        .take((maximum + 1) as u64)
        .read_to_end(&mut data)?;
    decode(&data)
}

fn decode(data: &[u8]) -> Result<GainMap> {
    let maximum = MAXIMUM_MAP_SAMPLES * 4 + 4;
    if !(8..=maximum).contains(&data.len()) {
        return Err("invalid calibration map length".into());
    }
    let trailer = &data[data.len() - 4..];
    let height = u16::from_le_bytes([trailer[0], trailer[1]]);
    let width = u16::from_le_bytes([trailer[2], trailer[3]]);
    let count = usize::from(width) * usize::from(height);
    if width == 0
        || height == 0
        || !width.is_multiple_of(2)
        || !height.is_multiple_of(2)
        || count > MAXIMUM_MAP_SAMPLES
        || data.len() != count * 4 + 4
    {
        return Err("invalid calibration map dimensions or payload".into());
    }
    let values: Vec<_> = data[..data.len() - 4]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|value| f32::from_le_bytes(*value))
        .collect();
    if values.iter().any(|value| !value.is_finite()) {
        return Err("nonfinite calibration map".into());
    }
    Ok(GainMap {
        extent: [u32::from(width), u32::from(height)],
        values,
    })
}

fn blend2(first: &[f32], second: &[f32], focus: f32) -> Vec<f32> {
    let t = focus / FOCUS_ENCODER_ENDPOINT;
    let s = 1. - t;
    first
        .iter()
        .zip(second)
        .map(|(&a, &b)| s.mul_add(a, t * b))
        .collect()
}

fn blend4(a: &[f32], b: &[f32], c: &[f32], d: &[f32], weight: f32, focus: f32) -> Vec<f32> {
    let other = 1. - weight;
    // The four-map overload uses Double for focus blending, unlike blend2.
    let far = f64::from(focus) / f64::from(FOCUS_ENCODER_ENDPOINT);
    let near = 1. - far;
    a.iter()
        .zip(b)
        .zip(c)
        .zip(d)
        .map(|(((&a, &b), &c), &d)| {
            let first = weight.mul_add(a, other * b);
            let second = weight.mul_add(c, other * d);
            near.mul_add(f64::from(first), far * f64::from(second)) as f32
        })
        .collect()
}

#[cfg(test)]
#[path = "resources_test.rs"]
mod tests;
