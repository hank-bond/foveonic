//! Selects camera-derived cutoffs and preserves ordered camera records for masked GPU correction.
//!
//! Record decoding and single-exposure cutoff selection follow Foveonic repair_setup.rs at e47af35,
//! adapted with owner permission. GPU arithmetic follows its repair.rs, including prior-write reads.

use crate::source::{MatrixEncoding, Metadata, Result};

#[cfg(test)]
#[path = "repair_test.rs"]
mod ordered_tests;

/// Enabled repair follows the current source's camera cutoff unless explicitly overridden.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub enum Threshold {
    #[default]
    Camera,
    Manual(u32),
}
impl Threshold {
    pub fn validate(self) -> Result<()> {
        if matches!(self, Self::Manual(value) if value > 15) {
            return Err("repair badness must be in 0..15".into());
        }
        Ok(())
    }
    /// Resolve a single-exposure cutoff; manual selection needs no threshold-table metadata.
    pub fn resolve(self, metadata: &Metadata) -> Result<u32> {
        self.validate()?;
        match self {
            Self::Manual(value) => Ok(value),
            Self::Camera => camera_cutoff(metadata),
        }
    }
}
fn camera_float(metadata: &Metadata, name: &str) -> Result<f32> {
    let Some(m) = metadata
        .matrix(name)
        .filter(|m| m.encoding == MatrixEncoding::Float32 && m.dimensions == [1])
    else {
        return Ok(0.);
    };
    if m.values.len() != 1 || !m.values[0].is_finite() {
        return Err(format!("invalid repair camera key {name}").into());
    }
    Ok(m.values[0] as f32)
}
pub(crate) fn camera_class(metadata: &Metadata) -> Result<u32> {
    let m = metadata
        .matrix("BadnessTable")
        .ok_or("missing repair badness table")?;
    if m.encoding != MatrixEncoding::Float32 || m.dimensions != [4, 3] || m.values.len() != 12 {
        return Err("invalid repair badness table".into());
    }
    let table: [[f32; 3]; 4] =
        std::array::from_fn(|r| std::array::from_fn(|c| m.values[r * 3 + c] as f32));
    let shutter = camera_float(metadata, "CaptureShutter")?;
    let temperature = camera_float(metadata, "SensorTemperature")?;
    if !shutter.is_finite()
        || shutter < 0.
        || !temperature.is_finite()
        || table.iter().flatten().any(|v| !v.is_finite())
        || table
            .iter()
            .any(|r| r[1] <= 0. || r[2] < 0. || r[2].fract() != 0. || r[2] > 15.)
    {
        return Err("invalid repair selection inputs".into());
    }
    // Preserve the reference's float32 exp and rounded products.
    let exponent = f32::from_bits(0x3db17218);
    let thresholds = table.map(|r| r[1] * (r[0] * exponent).exp());
    if thresholds.iter().any(|v| !v.is_finite()) || thresholds.windows(2).any(|r| r[0] <= r[1]) {
        return Err("unordered repair temperature thresholds".into());
    }
    let score = (shutter / 1000.) * (temperature * exponent).exp();
    if !score.is_finite() {
        return Err("nonfinite repair exposure score".into());
    }
    let index = if score <= thresholds[3] {
        3
    } else if score >= thresholds[0] {
        0
    } else {
        (0..4).rev().find(|&i| score <= thresholds[i]).unwrap()
    };
    Ok(table[index][2] as u32)
}

pub(crate) fn camera_integer(metadata: &Metadata, name: &str, default: u32) -> Result<u32> {
    match metadata
        .matrix(name)
        .filter(|m| m.encoding != MatrixEncoding::Int16 && m.dimensions == [1])
    {
        None => Ok(default),
        Some(m)
            if m.values.len() == 1
                && m.values[0].is_finite()
                && (0. ..=u32::MAX as f64).contains(&m.values[0])
                && m.values[0].fract() == 0. =>
        {
            Ok(m.values[0] as u32)
        }
        Some(_) => Err(format!("invalid repair integer key {name}").into()),
    }
}

fn camera_cutoff(metadata: &Metadata) -> Result<u32> {
    let class = camera_class(metadata)?;
    let offset = camera_integer(metadata, "BPBadnessOffset", 1)?;
    if class == 0 {
        Ok(0)
    } else {
        class
            .checked_sub(offset)
            .ok_or_else(|| "invalid camera repair cutoff".into())
    }
}

pub struct Prepared {
    pub params: [u32; 4],
    /// Linear sensor index and unmerged camera flags, in original metadata order.
    pub records: Vec<[u32; 2]>,
}

pub fn prepare(metadata: &Metadata, minimum_badness: u32) -> Result<Prepared> {
    prepare_list(metadata, "BadPixelsF20", minimum_badness)
}

pub(crate) fn prepare_list(
    metadata: &Metadata,
    name: &str,
    minimum_badness: u32,
) -> Result<Prepared> {
    if minimum_badness > 15 {
        return Err("repair badness must be in 0..15".into());
    }
    let matrix = metadata
        .matrix(name)
        .ok_or_else(|| format!("missing {name} metadata"))?;
    if matrix.dimensions.len() != 2
        || matrix.dimensions[0] != 3
        || matrix.values.len() != matrix.dimensions[1] * 3
    {
        return Err("invalid BadPixelsF20 records".into());
    }
    let pixels = metadata
        .width
        .checked_mul(metadata.height)
        .ok_or("repair dimensions overflow")?;
    if metadata.width < 3 || metadata.height < 3 || pixels > u32::MAX as usize / 3 {
        return Err("unsupported repair geometry".into());
    }
    let mut records = Vec::new();
    for record in matrix.values.as_chunks::<3>().0 {
        if record
            .iter()
            .any(|v| *v < 0. || v.fract() != 0. || *v > u16::MAX as f64)
        {
            return Err("invalid bad-pixel record".into());
        }
        let [y, x, bits] = record.map(|v| v as u32);
        if x >= metadata.width as u32 || y >= metadata.height as u32 {
            return Err("bad-pixel record outside sensor".into());
        }
        if ((bits >> 8) & 15) >= minimum_badness {
            records.push([y * metadata.width as u32 + x, bits]);
        }
    }
    let count = u32::try_from(records.len()).map_err(|_| "too many repair records")?;
    Ok(Prepared {
        params: [metadata.width as u32, metadata.height as u32, count, 0],
        records,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{Matrix, MatrixEncoding};
    #[test]
    fn camera_cutoff_tracks_source_and_manual_selection_bypasses_camera_tables() {
        let mut metadata = Metadata {
            width: 3,
            height: 3,
            rotation: 0,
            properties: Default::default(),
            lists: Default::default(),
            matrices: Default::default(),
        };
        assert_eq!(Threshold::Manual(3).resolve(&metadata).unwrap(), 3);
        assert!(Threshold::Camera.resolve(&metadata).is_err());
        let matrix = |dimensions, values| Matrix {
            encoding: MatrixEncoding::Float32,
            representation: "f32",
            dimensions,
            values,
        };
        metadata.matrices.insert(
            "BadnessTable".into(),
            matrix(
                vec![4, 3],
                vec![0., 1000., 0., 0., 100., 1., 0., 10., 2., 0., 1., 3.],
            ),
        );
        assert_eq!(Threshold::Camera.resolve(&metadata).unwrap(), 2);
        metadata
            .matrices
            .insert("CaptureShutter".into(), matrix(vec![1], vec![9000.]));
        assert_eq!(Threshold::Camera.resolve(&metadata).unwrap(), 1);
        metadata
            .matrices
            .insert("SensorTemperature".into(), matrix(vec![1], vec![8.]));
        assert_eq!(Threshold::Camera.resolve(&metadata).unwrap(), 0);
        assert_eq!(Threshold::Manual(3).resolve(&metadata).unwrap(), 3);
        metadata
            .matrices
            .get_mut("SensorTemperature")
            .unwrap()
            .values[0] = f64::NAN;
        assert!(Threshold::Camera.resolve(&metadata).is_err());
        assert!(Threshold::Manual(16).resolve(&metadata).is_err());
    }

    #[test]
    #[ignore = "requires FOVEONIC_FIXTURE_DIR"]
    fn originals_have_valid_camera_cutoffs_and_blue_sign_selects_two() {
        let dir = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
        for file in [
            "blue-parking-sign",
            "glossy-leaves-and-buds",
            "rocks-and-tree-shadows",
            "sunlit-red-flower",
            "turquoise-still-life",
            "yellow-flowers-and-stroller",
        ] {
            let source = crate::source::Source::open(dir.join(format!("{file}.x3f"))).unwrap();
            let cutoff = Threshold::Camera.resolve(&source.metadata).unwrap();
            assert!(cutoff <= 15);
            if file == "blue-parking-sign" {
                assert_eq!(cutoff, 2);
            }
            assert!(!prepare(&source.metadata, cutoff)
                .unwrap()
                .records
                .is_empty());
        }
    }

    #[test]
    #[ignore = "requires FOVEONIC_FIXTURE_DIR"]
    fn camera_records_do_not_select_the_two_unlisted_red_speckles() {
        let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
            .join("blue-parking-sign.x3f");
        let source = crate::source::Source::open(path).unwrap();
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/data/repair-reference.json")).unwrap();
        for cutoff in 0..=3 {
            let selected = prepare(&source.metadata, cutoff).unwrap();
            for point in fixture["points"].as_array().unwrap() {
                let x = point["sensor"][0].as_u64().unwrap() as usize;
                let y = point["sensor"][1].as_u64().unwrap() as usize;
                let pixel = y * source.metadata.width + x;
                assert!(!selected.records.iter().any(|r| r[0] == pixel as u32));
                for c in 0..3 {
                    assert_eq!(
                        source.pixels[pixel * 3 + c],
                        point["original_layers"][c].as_f64().unwrap() as f32
                    );
                }
            }
        }
    }

    #[test]
    fn selection_keeps_layer_flags_and_camera_neighbors() {
        let metadata = Metadata {
            width: 3,
            height: 3,
            rotation: 0,
            properties: Default::default(),
            lists: Default::default(),
            matrices: [(
                "BadPixelsF20".into(),
                Matrix {
                    encoding: MatrixEncoding::Uint32,
                    representation: "u32",
                    dimensions: vec![3, 2],
                    values: vec![1., 1., 0x43ff as f64, 0., 0., 0x72ff as f64],
                },
            )]
            .into(),
        };
        let prepared = prepare(&metadata, 3).unwrap();
        assert_eq!(prepared.records, [[4, 0x43ff]]);
        assert_eq!(prepared.params, [3, 3, 1, 0]);
        assert!(prepare(&metadata, 16).is_err());
    }
}
