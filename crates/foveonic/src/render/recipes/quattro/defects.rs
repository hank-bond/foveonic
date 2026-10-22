//! Expands camera defect records and prepares ordinary DP chroma repair controls.

use crate::{
    nodes::sensor::chroma_repair::Prepared,
    source::{MatrixEncoding, Metadata, Result},
};
const REFERENCE_SENSOR_ISO: f64 = 100.;
const TEMPERATURE_SCORE_SCALE: f32 = 0.0866434;
const SHUTTER_SCORE_DIVISOR: f32 = 1000.;
const MINIMUM_DEFECT_EXPOSURE: f32 = 200.;
// Reference temperature, base threshold, severity. These are fallback CAMF rows.
const DEFAULT_BADNESS: [[f32; 3]; 4] = [
    [30., 10000., 0.],
    [30., 3000., 1.],
    [30., 500., 2.],
    [30., 200., 3.],
];
#[derive(Clone, Copy, Debug)]
pub(super) struct Severity {
    pub image: u32,
    pub pixels: u32,
    pub clusters: u32,
    pub score: f32,
    pub cluster_margin: u32,
}

pub(super) fn supported(metadata: &Metadata) -> bool {
    (super::calibration::is_ordinary_dp(metadata) || super::calibration::is_ordinary_sd(metadata))
        && super::calibration::normalization(metadata).is_ok()
        && optional(metadata, "CaptureISO", REFERENCE_SENSOR_ISO)
            .is_ok_and(|v| v.is_finite() && v > 0.)
}
pub(super) fn prepare(metadata: &Metadata) -> Result<Prepared> {
    if !supported(metadata) {
        return Err("chroma repair requires the ordinary Quattro factory-calibration route".into());
    }
    let severity = severity(metadata)?;
    let records = expand_records(metadata, "BadPixelsChromaF23")?;
    let matrix = preprocessing_matrix(metadata)?;
    Prepared::new(
        [
            u32::try_from(metadata.width / 2)?,
            u32::try_from(metadata.height / 2)?,
        ],
        records,
        severity.pixels,
        super::repair::RGB_NOISE,
        matrix,
        false,
        super::calibration::is_ordinary_sd(metadata),
    )
}
/// Ordinary factory despeckling uses immutable neighborhoods and unit strength.
pub(super) fn prepare_despeckle(
    metadata: &Metadata,
) -> Result<crate::nodes::sensor::rgb_despeckle::Prepared> {
    if !supported(metadata) {
        return Err(
            "RGB despeckling requires the ordinary Quattro factory-calibration route".into(),
        );
    }
    const SECOND_EXTREMA_EXPOSURE_SCORE: f32 = 64000.;
    if severity(metadata)?.score > SECOND_EXTREMA_EXPOSURE_SCORE {
        return Err("long-exposure RGB despeckling is not implemented".into());
    }
    crate::nodes::sensor::rgb_despeckle::Prepared::new(
        [
            u32::try_from(metadata.width / 2)?,
            u32::try_from(metadata.height / 2)?,
        ],
        super::repair::RGB_NOISE,
        1.,
        preprocessing_matrix(metadata)?,
    )
}
/// Cluster records carry their own severity; selected records retain file order.
pub(super) fn prepare_clusters(
    metadata: &Metadata,
) -> Result<crate::nodes::sensor::chroma_clusters::Prepared> {
    if !supported(metadata) {
        return Err(
            "chroma clusters require the ordinary Quattro factory-calibration route".into(),
        );
    }
    let severity = severity(metadata)?;
    let mut clusters = vec![];
    if let Some(matrix) = metadata.matrix("BadClustersChromaF23") {
        if matrix.encoding != MatrixEncoding::Uint16 {
            return Err("invalid chroma cluster catalogue type".into());
        }
        let words = &matrix.values;
        let mut cursor = 0;
        while cursor < words.len() {
            if words.len() - cursor < 4 {
                return Err("truncated chroma cluster header".into());
            }
            let header = words[cursor] as u32;
            let row = words[cursor + 1] as u32;
            let column = words[cursor + 2] as u32;
            let properties = words[cursor + 3] as u32;
            let count = (properties & 255) as usize;
            cursor += 4;
            if words.len() - cursor < count {
                return Err("truncated chroma cluster points".into());
            }
            clusters.push(crate::nodes::sensor::chroma_clusters::Cluster {
                severity: header >> 14,
                extent: [header & 127, (header >> 7) & 127],
                origin: [column, row & 32767],
                margin: properties >> 8,
                points: words[cursor..cursor + count]
                    .iter()
                    .map(|v| {
                        let v = *v as u32;
                        [v >> 10, (v >> 4) & 63, v & 15]
                    })
                    .collect(),
            });
            cursor += count;
        }
    }
    crate::nodes::sensor::chroma_clusters::Prepared::new(
        [
            u32::try_from(metadata.width / 2)?,
            u32::try_from(metadata.height / 2)?,
        ],
        clusters,
        severity.clusters,
        severity.cluster_margin,
    )
}
/// Preprocessing uses the cached Auto camera conversion, independently of the
/// selected render WB. Each matrix column receives its temperature-adjusted gain.
fn preprocessing_matrix(metadata: &Metadata) -> Result<[f64; 9]> {
    let capture_iso = optional(metadata, "CaptureISO", REFERENCE_SENSOR_ISO)?;
    if !capture_iso.is_finite() || capture_iso <= 0. {
        return Err("invalid preprocessing capture ISO".into());
    }
    let iso_gain = (capture_iso / REFERENCE_SENSOR_ISO) as f32;
    if !iso_gain.is_finite() || iso_gain <= 0. {
        return Err("invalid preprocessing ISO gain".into());
    }
    let gains = auto_white_balance(metadata)?;
    let matrix = metadata.fixed::<9>("AutoCCMatrix")?;
    if matrix.iter().any(|v| !v.is_finite()) {
        return Err("invalid preprocessing Auto color matrix".into());
    }
    // GatherParams first widens the Float32 ISO factor into each cached Auto
    // matrix coefficient. Preprocessing then multiplies its WB column.
    Ok(std::array::from_fn(|i| {
        gains[i % 3] * (matrix[i] * f64::from(iso_gain))
    }))
}

/// Factory calibration and optical correction use temperature-adjusted Auto gains.
pub(super) fn auto_white_balance(metadata: &Metadata) -> Result<[f64; 3]> {
    let raw = metadata.fixed::<3>("AutoWBGain")?;
    if raw.iter().any(|v| !v.is_finite() || v.abs() < 1e-9) {
        return Err("invalid preprocessing Auto gains".into());
    }
    let mut temperature = metadata
        .matrix("TempGainFact")
        .map(|m| m.values.as_slice())
        .unwrap_or(&[1.; 3])
        .to_vec();
    if temperature.len() != 3
        || temperature
            .iter()
            .any(|v| !v.is_finite() || !(0.5..=1.5).contains(v))
    {
        temperature = vec![1.; 3];
    }
    let adjusted: [f64; 3] = std::array::from_fn(|i| raw[i] * temperature[i]);
    Ok(adjusted.map(|v| v / adjusted[1]))
}

pub(super) fn severity(metadata: &Metadata) -> Result<Severity> {
    let table = if let Some(matrix) = metadata.matrix("BadnessTable") {
        if matrix.values.len() != 12 {
            return Err("invalid defect severity table".into());
        }
        std::array::from_fn(|i| std::array::from_fn(|j| matrix.values[i * 3 + j] as f32))
    } else {
        DEFAULT_BADNESS
    };
    if table.iter().flatten().any(|v| !v.is_finite()) {
        return Err("nonfinite defect severity table".into());
    }
    let temperature = metadata.scalar("SensorTemperature")? as f32;
    let shutter = metadata.scalar("CaptureShutter")? as f32;
    let score = (temperature * TEMPERATURE_SCORE_SCALE).exp()
        * (shutter / SHUTTER_SCORE_DIVISOR).max(MINIMUM_DEFECT_EXPOSURE);
    let thresholds = table.map(|row| row[1] * (row[0] * TEMPERATURE_SCORE_SCALE).exp());
    if !score.is_finite() || thresholds.iter().any(|v| !v.is_finite()) {
        return Err("nonfinite defect severity score".into());
    }
    let low = thresholds.into_iter().fold(1e6_f32, f32::min);
    let high = thresholds.into_iter().fold(-1e6_f32, f32::max);
    let mut image = table[3][2] as u32;
    if score >= high {
        image = table[0][2] as u32;
    } else if score > low {
        for i in (0..4).rev() {
            if thresholds[i] >= score {
                image = table[i][2] as u32;
                break;
            }
        }
    }
    let offset = integer(metadata, "BPBadnessOffset", 1)?;
    let cluster_offset = integer(metadata, "BCBadnessOffset", 1)?;
    Ok(Severity {
        image,
        pixels: if image == 0 {
            0
        } else {
            image.wrapping_sub(offset)
        },
        clusters: if image == 0 {
            0
        } else {
            image.wrapping_sub(cluster_offset)
        },
        score,
        cluster_margin: integer(metadata, "BadClusterMargin", 25)?,
    })
}
fn integer(metadata: &Metadata, name: &str, default: u32) -> Result<u32> {
    let value = optional(metadata, name, f64::from(default))?;
    if !value.is_finite() || !(0. ..=u32::MAX as f64).contains(&value) || value.fract() != 0. {
        return Err(format!("invalid {name}").into());
    }
    Ok(value as u32)
}
fn optional(metadata: &Metadata, name: &str, default: f64) -> Result<f64> {
    if metadata.matrix(name).is_some() {
        metadata.scalar(name)
    } else {
        Ok(default)
    }
}
pub(super) fn expand_records(metadata: &Metadata, name: &str) -> Result<Vec<[u32; 3]>> {
    let Some(matrix) = metadata.matrix(name) else {
        return Ok(Vec::new());
    };
    if matrix.encoding != MatrixEncoding::Uint16 || matrix.values.len() < 4 {
        return Err("invalid compressed chroma defect catalogue".into());
    }
    let words: Vec<_> = matrix.values.iter().map(|&v| v as u32).collect();
    let mut row = words[0];
    let mut out = vec![[row, words[1], words[2]]];
    let mut cursor = 3;
    loop {
        let mut column = *words.get(cursor).ok_or("missing chroma column sentinel")?;
        cursor += 1;
        if column == 0 {
            if cursor == words.len() {
                return Ok(out);
            }
            row = *words.get(cursor).ok_or("missing chroma row sentinel")?;
            cursor += 1;
            if row == 0 {
                return Ok(out);
            }
            column = *words.get(cursor).ok_or("missing chroma row column")?;
            cursor += 1;
        }
        let flags = *words.get(cursor).ok_or("missing chroma defect flags")?;
        cursor += 1;
        out.push([row, column, flags]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn factory_color_matrix_scales_capture_iso_before_its_white_balance_columns() {
        use crate::source::Matrix;
        let matrix = |values: Vec<f64>| Matrix {
            encoding: MatrixEncoding::Float32,
            representation: "f64",
            dimensions: vec![values.len()],
            values,
        };
        let mut metadata = Metadata {
            width: 5888,
            height: 3672,
            rotation: 0,
            processing_version: Some([3, 1, 2, 0]),
            properties: Default::default(),
            lists: Default::default(),
            matrices: [
                ("AutoWBGain".into(), matrix(vec![2., 1., 3.])),
                (
                    "AutoCCMatrix".into(),
                    matrix(vec![0.7, 0.2, 0.1, 0.1, 0.8, 0.1, 0.1, 0.2, 0.7]),
                ),
                ("CaptureISO".into(), matrix(vec![125.])),
            ]
            .into(),
        };
        let actual = preprocessing_matrix(&metadata).unwrap();
        let original = metadata.fixed::<9>("AutoCCMatrix").unwrap();
        for i in 0..9 {
            assert_eq!(actual[i], [2., 1., 3.][i % 3] * (original[i] * 1.25));
        }
        metadata.matrices.get_mut("CaptureISO").unwrap().values[0] = f64::NAN;
        assert!(preprocessing_matrix(&metadata).is_err());
    }
    #[test]
    #[ignore = "requires QUATTRO_X3F, RGB_NATIVE_BOUNDARY and GPU"]
    fn full_chroma_clusters_match_actual_native() {
        let source =
            crate::source::Source::open(std::env::var_os("QUATTRO_X3F").expect("set QUATTRO_X3F"))
                .unwrap();
        let prepared = prepare_clusters(&source.metadata).unwrap();
        let directory = std::path::PathBuf::from(
            std::env::var_os("RGB_NATIVE_BOUNDARY").expect("set RGB_NATIVE_BOUNDARY"),
        );
        let read = |name: &str| -> Vec<f32> {
            let b = std::fs::read(directory.join(name)).unwrap();
            b.as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect()
        };
        let gpu = crate::gpu::Gpu::new().unwrap();
        let input = gpu
            .upload("Native RGB cluster input", &read("rgb-clusters-input.f32"))
            .unwrap();
        let mask = std::fs::read(directory.join("rgb-clusters-mask.u8"))
            .unwrap()
            .into_iter()
            .map(u32::from)
            .collect::<Vec<_>>();
        let mask = gpu.upload("Native cluster mask", &mask).unwrap();
        let start = std::time::Instant::now();
        let output = crate::nodes::sensor::chroma_clusters::apply(
            &gpu,
            &input,
            &mask,
            &prepared,
            &mut Default::default(),
        )
        .unwrap();
        let b = gpu.readback(&output).unwrap();
        eprintln!("Full RGB clusters {:?}", start.elapsed());
        let actual = bytemuck::cast_slice::<u8, f32>(&b);
        let expected = read("rgb-clusters-output.f32");
        let mut changed = 0;
        let mut maximum = 0_f32;
        for (i, (&a, &b)) in actual.iter().zip(&expected).enumerate() {
            maximum = maximum.max((a - b).abs());
            changed += usize::from(a.to_bits() != b.to_bits());
            assert_eq!(a.to_bits(), b.to_bits(), "sample{i} {a} vs {b}");
        }
        eprintln!(
            "RGB clusters changed{changed}/{} max{maximum}",
            actual.len()
        );
    }
    #[test]
    #[ignore = "requires QUATTRO_X3F and CHROMA_NATIVE_BOUNDARY"]
    fn source_controls_and_catalogue_match_actual_native_caller() {
        let source =
            crate::source::Source::open(std::env::var_os("QUATTRO_X3F").expect("set QUATTRO_X3F"))
                .unwrap();
        let m = &source.metadata;
        let directory = std::path::PathBuf::from(
            std::env::var_os("CHROMA_NATIVE_BOUNDARY").expect("set CHROMA_NATIVE_BOUNDARY"),
        );
        let controls: serde_json::Value = serde_json::from_slice(
            &std::fs::read(directory.join("chroma-boundaries.json")).unwrap(),
        )
        .unwrap();
        let records = expand_records(m, "BadPixelsChromaF23").unwrap();
        let actual = std::fs::read(directory.join("chroma-single-records.bin")).unwrap();
        assert!(actual.len() >= records.len() * 6);
        for (record, bytes) in records.iter().zip(actual.as_chunks::<6>().0) {
            for c in 0..3 {
                assert_eq!(
                    record[c],
                    u32::from(u16::from_le_bytes([bytes[c * 2], bytes[c * 2 + 1]]))
                );
            }
        }
        // The native pointer-only decoder runs beyond the bounded CAMF stream.
        // Its six trailing records describe unrelated allocation bytes, all outside
        // this physical image; numerical repair skips them before reading pixels.
        for bytes in actual[records.len() * 6..].as_chunks::<6>().0 {
            let row = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
            let column = u16::from_le_bytes([bytes[2], bytes[3]]) as usize;
            assert!(row >= m.height / 2 || column >= m.width / 2);
        }
        let severity = severity(m).unwrap();
        assert_eq!(severity.image, 1);
        assert_eq!(severity.pixels, 0);
        assert_eq!(severity.clusters, 0);
        assert_eq!(severity.score.to_bits(), 0x45a82de4);
        assert_eq!(severity.cluster_margin, 25);
        let matrix = preprocessing_matrix(m).unwrap();
        for (i, &v) in matrix.iter().enumerate() {
            assert_eq!(
                v.to_bits(),
                controls["single"]["matrix"][i].as_f64().unwrap().to_bits()
            );
        }
        let prepared = prepare(m).unwrap();
        assert_eq!(prepared.extent(), [2944, 1836]);
    }
}
