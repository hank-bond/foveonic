//! Prepares ordinary enabled highlight restoration and odd-phase blue mismatch controls.
use crate::{
    nodes::sensor::{blue_mismatch, highlight_restoration},
    source::{Metadata, Result},
};
fn rectangle(metadata: &Metadata) -> Result<[u32; 4]> {
    let area = metadata.fixed::<4>("ActiveImageArea")?;
    if area
        .iter()
        .any(|v| !v.is_finite() || *v < 0. || *v > u32::MAX as f64 || v.fract() != 0.)
    {
        return Err("invalid highlight active rectangle".into());
    }
    Ok([
        area[0] as u32 / 2,
        area[1] as u32 / 2,
        (area[2] as u32).div_ceil(2),
        (area[3] as u32).div_ceil(2),
    ])
}
pub(super) fn prepare_blue(metadata: &Metadata) -> Result<blue_mismatch::Prepared> {
    if !super::defects::supported(metadata) {
        return Err("blue mismatch requires the ordinary Quattro route".into());
    }
    if (super::calibration::normalization(metadata)? - 1.).abs() >= f32::EPSILON {
        return Err("blue mismatch is bypassed outside unit sensor normalization".into());
    }
    let r = rectangle(metadata)?;
    let threshold = if super::calibration::is_ordinary_sd(metadata) {
        14295.
    } else {
        13995.
    };
    blue_mismatch::Prepared::new(
        [
            u32::try_from(metadata.width)?,
            u32::try_from(metadata.height)?,
        ],
        [r[0] * 2 + 1, r[1] * 2 + 1, r[2] * 2, r[3] * 2],
        threshold,
    )
}
pub(super) fn prepare_restoration(metadata: &Metadata) -> Result<highlight_restoration::Prepared> {
    if !super::defects::supported(metadata) {
        return Err("highlight restoration requires the ordinary Quattro enabled route".into());
    }
    highlight_restoration::Prepared::new(
        [
            u32::try_from(metadata.width / 2)?,
            u32::try_from(metadata.height / 2)?,
        ],
        rectangle(metadata)?,
        super::calibration::normalization(metadata)?,
        metadata.fixed::<3>("AutoWBGain")?.map(|v| v as f32),
        metadata.fixed::<9>("AutoCCMatrix")?,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nonunit_restoration_is_prepared_while_blue_mismatch_is_explicitly_bypassed() {
        use crate::source::{Matrix, MatrixEncoding};
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
                ("CAMERAID".into(), matrix(vec![81.])),
                ("SensorISO".into(), matrix(vec![100.])),
                ("CaptureISO".into(), matrix(vec![400.])),
                ("WhiteBalance".into(), matrix(vec![1.])),
                (
                    "ActiveImageArea".into(),
                    matrix(vec![20., 20., 5400., 3600.]),
                ),
                ("AutoWBGain".into(), matrix(vec![1., 1., 1.])),
                (
                    "AutoCCMatrix".into(),
                    matrix(vec![1., 0., 0., 0., 1., 0., 0., 0., 1.]),
                ),
            ]
            .into(),
        };
        let unit = prepare_restoration(&metadata).unwrap();
        assert!(prepare_blue(&metadata).is_ok());
        for sensor_iso in [50., 200.] {
            metadata.matrices.get_mut("SensorISO").unwrap().values[0] = sensor_iso;
            assert!(prepare_restoration(&metadata).unwrap().key() != unit.key());
            assert!(prepare_blue(&metadata).is_err());
        }
    }
    #[test]
    #[ignore = "requires QUATTRO_X3F,HIGHLIGHT_NATIVE_BOUNDARY and GPU"]
    fn complete_highlight_boundaries_match_native() {
        let source =
            crate::source::Source::open(std::env::var_os("QUATTRO_X3F").expect("set QUATTRO_X3F"))
                .unwrap();
        let directory = std::path::PathBuf::from(
            std::env::var_os("HIGHLIGHT_NATIVE_BOUNDARY").expect("set HIGHLIGHT_NATIVE_BOUNDARY"),
        );
        let c: serde_json::Value = serde_json::from_slice(
            &std::fs::read(directory.join("highlight-boundaries.json")).unwrap(),
        )
        .unwrap();
        let gpu = crate::gpu::Gpu::new().unwrap();
        let read = |name: &str| -> Vec<f32> {
            let b = std::fs::read(directory.join(name)).unwrap();
            b.as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect()
        };
        let blue = prepare_blue(&source.metadata).unwrap();
        let input = gpu
            .upload(
                "Original blue mismatch input",
                &read(c["blue-mismatch"]["input"]["file"].as_str().unwrap()),
            )
            .unwrap();
        let output = blue_mismatch::apply(&gpu, &input, &blue, &mut Default::default()).unwrap();
        let actual = gpu.readback(&output).unwrap();
        assert_eq!(
            actual,
            std::fs::read(directory.join(c["blue-mismatch"]["output"]["file"].as_str().unwrap()))
                .unwrap()
        );
        let prepared = prepare_restoration(&source.metadata).unwrap();
        let input = gpu
            .upload(
                "Original highlight RGB",
                &read(c["restoration"]["input"]["file"].as_str().unwrap()),
            )
            .unwrap();
        let start = std::time::Instant::now();
        let output =
            highlight_restoration::apply(&gpu, &input, &prepared, &mut Default::default()).unwrap();
        let b = gpu.readback(&output.rgb).unwrap();
        eprintln!("Highlight GPU+readback {:?}", start.elapsed());
        let actual = bytemuck::cast_slice::<u8, f32>(&b);
        let expected = read(c["restoration"]["output"]["file"].as_str().unwrap());
        let mut maximum = 0_f32;
        let mut changed = 0;
        let mut sum = 0_f64;
        for (i, (&a, &b)) in actual.iter().zip(&expected).enumerate() {
            let e = (a - b).abs();
            assert!(a.is_finite());
            maximum = maximum.max(e);
            changed += usize::from(a.to_bits() != b.to_bits());
            sum += f64::from(e);
            assert!(
                e <= 0.01_f32.max(b.abs() * f32::EPSILON * 16.),
                "sample{i} {a} vs {b}"
            );
        }
        eprintln!(
            "Highlight changed{changed}/{} mean{} max{maximum}",
            actual.len(),
            sum / actual.len() as f64
        );
        let actual = gpu.readback(&output.mask).unwrap();
        let actual = bytemuck::cast_slice::<u8, u32>(&actual);
        let expected = std::fs::read(
            directory.join(c["restoration"]["mask_output"]["file"].as_str().unwrap()),
        )
        .unwrap();
        assert!(actual
            .iter()
            .copied()
            .eq(expected.into_iter().map(u32::from)));
    }
}
