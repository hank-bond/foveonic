//! Prepares bounded luma defect catalogues and full-size Quattro blue-plane repair controls.

use crate::{
    nodes::sensor::{blue_despeckle, luma_cluster, ordered_blue_repair},
    source::{MatrixEncoding, Metadata, Result},
};

fn extent(metadata: &Metadata) -> Result<[u32; 2]> {
    if !super::defects::supported(metadata) {
        return Err("luma repair requires the ordinary DP Auto-calibration route".into());
    }
    Ok([
        u32::try_from(metadata.width)?,
        u32::try_from(metadata.height)?,
    ])
}

pub(super) fn prepare_records(
    metadata: &Metadata,
    stage: ordered_blue_repair::Stage,
) -> Result<ordered_blue_repair::Prepared> {
    let extent = extent(metadata)?;
    let gain = if stage == ordered_blue_repair::Stage::DarkOffsets {
        let name = if super::tiger::supported(metadata) {
            "SensorAdjustmentGainFact_BR_3COL_HI_RGBave"
        } else if super::calibration::is_ordinary_sd(metadata) {
            "SensorAdjustmentGainFact_HI"
        } else {
            "SensorAdjustmentGainFact"
        };
        if let Some(gain) = metadata.matrix(name) {
            *gain.values.get(2).ok_or("short sensor adjustment gains")? as f32
        } else {
            1.
        }
    } else {
        1.
    };
    ordered_blue_repair::Prepared::new(
        extent,
        super::defects::expand_records(metadata, "BadPixelsLumaF23")?,
        super::defects::severity(metadata)?.pixels,
        gain,
        stage,
    )
}

pub(super) fn prepare_despeckle(
    metadata: &Metadata,
    stage: blue_despeckle::Stage,
) -> Result<blue_despeckle::Prepared> {
    let extent = extent(metadata)?;
    let bounds = if stage == blue_despeckle::Stage::General {
        let area = metadata.fixed::<4>("ActiveImageArea")?;
        if area
            .iter()
            .any(|v| !v.is_finite() || *v < 0. || *v > u32::MAX as f64 || v.fract() != 0.)
        {
            return Err("invalid luma active rectangle".into());
        }
        // The caller rounds the inclusive rectangle into half-resolution cells;
        // the high-plane worker doubles those cell coordinates again.
        [
            (area[0] as u32 / 2) * 2,
            (area[1] as u32 / 2) * 2,
            (area[2] as u32).div_ceil(2) * 2,
            (area[3] as u32).div_ceil(2) * 2,
        ]
    } else {
        [0, 0, extent[0] - 1, extent[1] - 1]
    };
    blue_despeckle::Prepared::new(
        extent,
        super::repair::BLUE_NOISE,
        1.,
        if stage == blue_despeckle::Stage::DarkPairs {
            10.
        } else {
            0.
        },
        bounds,
        stage,
        // Physical bottom margins have no defined native allocation tail.
        blue_despeckle::MissingContext::Preserve,
    )
}

pub(super) fn prepare_clusters(metadata: &Metadata) -> Result<luma_cluster::Prepared> {
    let extent = extent(metadata)?;
    let severity = super::defects::severity(metadata)?;
    let mut clusters = vec![];
    if let Some(matrix) = metadata.matrix("BadClustersLumaF23") {
        if matrix.encoding != MatrixEncoding::Uint16 {
            return Err("invalid luma cluster catalogue type".into());
        }
        let words = &matrix.values;
        let mut cursor = 0;
        while cursor < words.len() {
            if words.len() - cursor < 4 {
                return Err("truncated luma cluster header".into());
            }
            let header = words[cursor] as u32;
            let row = words[cursor + 1] as u32;
            let column = words[cursor + 2] as u32;
            let properties = words[cursor + 3] as u32;
            let count = (properties & 255) as usize;
            cursor += 4;
            if words.len() - cursor < count {
                return Err("truncated luma cluster points".into());
            }
            if header >> 14 == severity.pixels {
                clusters.push(luma_cluster::Cluster {
                    origin: [row & 32767, column],
                    extent: [(header >> 7) & 127, header & 127],
                    margin: properties >> 8,
                    points: words[cursor..cursor + count]
                        .iter()
                        .map(|v| {
                            let v = *v as u32;
                            [v >> 8, v & 255]
                        })
                        .collect(),
                });
            }
            cursor += count;
        }
    }
    luma_cluster::Prepared::new(extent, clusters, severity.cluster_margin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires QUATTRO_X3F and LUMA_NATIVE_BOUNDARIES"]
    fn bounded_catalogue_matches_native_meaningful_prefix() {
        let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
        let metadata = &source.metadata;
        let records = super::super::defects::expand_records(metadata, "BadPixelsLumaF23").unwrap();
        let directory =
            std::path::PathBuf::from(std::env::var_os("LUMA_NATIVE_BOUNDARIES").unwrap());
        let native = std::fs::read(directory.join("luma-directional-0-records.bin")).unwrap();
        assert!(native.len() >= records.len() * 6);
        for (record, bytes) in records.iter().zip(native.as_chunks::<6>().0) {
            for channel in 0..3 {
                assert_eq!(
                    record[channel],
                    u32::from(u16::from_le_bytes([
                        bytes[channel * 2],
                        bytes[channel * 2 + 1]
                    ]))
                );
            }
        }
        // The native pointer decoder reads adjacent allocation bytes after the
        // bounded catalogue. Those trailing records lie outside the image.
        for bytes in native[records.len() * 6..].as_chunks::<6>().0 {
            let row = usize::from(u16::from_le_bytes([bytes[0], bytes[1]]));
            let column = usize::from(u16::from_le_bytes([bytes[2], bytes[3]]));
            assert!(row >= metadata.height || column >= metadata.width);
        }
        assert_eq!(records.len(), 187892);
        for stage in [
            ordered_blue_repair::Stage::Directional,
            ordered_blue_repair::Stage::DarkOffsets,
            ordered_blue_repair::Stage::General,
        ] {
            prepare_records(metadata, stage).unwrap();
        }
        for stage in [
            blue_despeckle::Stage::DarkPairs,
            blue_despeckle::Stage::General,
        ] {
            prepare_despeckle(metadata, stage).unwrap();
        }
    }

    #[test]
    #[ignore = "requires QUATTRO_X3F, LUMA_NATIVE_BOUNDARIES and GPU"]
    fn full_luma_clusters_match_actual_native() {
        let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
        let prepared = prepare_clusters(&source.metadata).unwrap();
        let directory =
            std::path::PathBuf::from(std::env::var_os("LUMA_NATIVE_BOUNDARIES").unwrap());
        let receipt: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join("luma-boundaries.json")).unwrap())
                .unwrap();
        let boundary = &receipt["clusters"][0];
        let read = |side: &str| {
            let name = std::path::Path::new(boundary[side]["file"].as_str().unwrap())
                .file_name()
                .unwrap();
            std::fs::read(directory.join(name)).unwrap()
        };
        let gpu = crate::gpu::Gpu::new().unwrap();
        let bytes = read("input");
        let input = gpu
            .upload(
                "Original luma cluster boundary",
                bytemuck::cast_slice::<u8, f32>(&bytes),
            )
            .unwrap();
        let mut profile = crate::profile::Profile::new(crate::profile::Mode::Gpu);
        let output = luma_cluster::apply(&gpu, &input, &prepared, &mut profile).unwrap();
        let actual = gpu.readback(&output).unwrap();
        let expected = read("output");
        let differences: Vec<_> = actual
            .as_chunks::<4>()
            .0
            .iter()
            .zip(expected.as_chunks::<4>().0)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .take(20)
            .map(|(i, (a, b))| (i, f32::from_le_bytes(*a), f32::from_le_bytes(*b)))
            .collect();
        eprintln!(
            "Luma clusters differences {differences:?}; {:?}",
            profile.gpu
        );
        assert!(differences.is_empty());
    }
}
