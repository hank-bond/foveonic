//! Checks saved adjustment rationals, malformed fields and original Quattro defaults.

use super::*;
use crate::source::CaptureAdjustments;

fn fixture(values: &[(u16, i32, i32)]) -> Vec<u8> {
    let start = ENTRY_START + values.len() * ENTRY_BYTES;
    let mut bytes = vec![0; start + values.len() * 8];
    bytes[4..8].copy_from_slice(&0x40002_u32.to_le_bytes());
    bytes[52..56].copy_from_slice(&(values.len() as u32).to_le_bytes());
    for (i, &(tag, numerator, denominator)) in values.iter().enumerate() {
        let entry = ENTRY_START + i * ENTRY_BYTES;
        let offset = start + i * 8;
        bytes[entry..entry + 2].copy_from_slice(&tag.to_le_bytes());
        bytes[entry + 2..entry + 4].copy_from_slice(&10_u16.to_le_bytes());
        bytes[entry + 4..entry + 8].copy_from_slice(&1_u32.to_le_bytes());
        bytes[entry + 8..entry + 12].copy_from_slice(&(offset as u32).to_le_bytes());
        bytes[offset..offset + 4].copy_from_slice(&numerator.to_le_bytes());
        bytes[offset + 4..offset + 8].copy_from_slice(&denominator.to_le_bytes());
    }
    bytes
}

#[test]
fn preserves_file_choices_and_distinguishes_missing_values() {
    let properties = read(&fixture(&[
        (14, 2, 4),
        (15, 1, 4),
        (63, 3, 6),
        (8, -5, 10),
        (10, 10, 10),
        (17, 3, 10),
        (64, 4, 6),
    ]))
    .unwrap();
    let choices = CaptureAdjustments::from_properties(&properties).unwrap();
    assert_eq!(choices.chroma_noise, Some(0.5));
    assert_eq!(choices.luminance_noise, Some(0.25));
    assert_eq!(choices.smoothness, Some(0.5));
    assert_eq!(choices.sharpness, Some(-0.5));
    assert_eq!(choices.highlight_correction, Some(1.));
    assert_eq!(choices.mono_exposure, Some(0.3));
    assert_eq!(choices.mono_smoothness, Some(4. / 6.));
    assert!(choices.exposure.is_none());
    assert!(read(&0x30000_u32.to_le_bytes().repeat(2))
        .unwrap()
        .is_empty());
}

#[test]
fn retains_inline_preset_enums_without_applying_them() {
    let mut header = fixture(&[(1, 0, 1), (2, 0, 1)]);
    for (i, value) in [7_u32, 4].into_iter().enumerate() {
        let entry = ENTRY_START + i * ENTRY_BYTES;
        header[entry + 2..entry + 4].copy_from_slice(&1_u16.to_le_bytes());
        header[entry + 8..entry + 12].copy_from_slice(&value.to_le_bytes());
    }
    let choices = CaptureAdjustments::from_properties(&read(&header).unwrap()).unwrap();
    assert_eq!(choices.white_balance, Some(7));
    assert_eq!(choices.color_mode, Some(4));
}

#[test]
fn rejects_corrupt_recognized_adjustments() {
    for case in 0..6 {
        let mut header = fixture(&[(14, 2, 4)]);
        match case {
            0 => header.truncate(55),
            1 => header[52..56].copy_from_slice(&u32::MAX.to_le_bytes()),
            2 => header[ENTRY_START + 2..ENTRY_START + 4].copy_from_slice(&5_u16.to_le_bytes()),
            3 => header[ENTRY_START + 4..ENTRY_START + 8].copy_from_slice(&2_u32.to_le_bytes()),
            4 => header[ENTRY_START + 8..ENTRY_START + 12].copy_from_slice(&56_u32.to_le_bytes()),
            _ => {
                let len = header.len();
                header[len - 4..].fill(0);
            }
        }
        assert!(read(&header).is_err(), "case {case}");
    }
    assert!(read(&fixture(&[(14, 1, 4), (14, 2, 4)])).is_err());
}

#[test]
#[ignore = "requires QUATTRO_X3F pointing to an acquired original"]
fn original_header_choices_match_full_and_capture_readers() {
    let path = std::env::var_os("QUATTRO_X3F").unwrap();
    let mut file = std::fs::File::open(&path).unwrap();
    let properties = crate::x3f::capture::read(&mut file).unwrap();
    let capture = CaptureAdjustments::from_properties(&properties).unwrap();
    let source = crate::source::Source::open(path).unwrap();
    let decoded = source.metadata.capture_adjustments().unwrap();
    assert_eq!(
        properties["X3F_MARK_BITS"],
        source.metadata.properties["X3F_MARK_BITS"]
    );
    assert_eq!(capture.chroma_noise, Some(0.5));
    assert_eq!(capture.luminance_noise, Some(0.5));
    assert_eq!(decoded.chroma_noise, capture.chroma_noise);
    assert_eq!(decoded.luminance_noise, capture.luminance_noise);
    assert_eq!(decoded.smoothness, capture.smoothness);
    assert_eq!(decoded.sharpness, Some(0.));
    assert_eq!(decoded.highlight_correction, Some(1.));
    assert_eq!(decoded.mono_mix, capture.mono_mix);
    assert_eq!(
        decoded.mono_mix,
        Some([10813. / 32768., 10813. / 32768., 11142. / 32768.])
    );
}

#[test]
fn saved_monochrome_mix_preserves_unsigned_header_weights() {
    let mut bytes = fixture(&[(25, 0, 1), (26, 0, 1), (27, 0, 1)]);
    for (i, weight) in [10813_u16, 10813, 11142].into_iter().enumerate() {
        let entry = ENTRY_START + i * ENTRY_BYTES;
        bytes[entry + 2..entry + 4].copy_from_slice(&3_u16.to_le_bytes());
        bytes[entry + 8..entry + 12].copy_from_slice(&u32::from(weight).to_le_bytes());
    }
    let captured = CaptureAdjustments::from_properties(&read(&bytes).unwrap()).unwrap();
    assert_eq!(
        captured.mono_mix,
        Some([10813. / 32768., 10813. / 32768., 11142. / 32768.])
    );
    bytes[ENTRY_START + 2..ENTRY_START + 4].copy_from_slice(&10_u16.to_le_bytes());
    assert!(read(&bytes).is_err());
    let zero = CaptureAdjustments::from_properties(
        &[
            ("X3F_ADJUSTMENT_MONO_MIX_RED".into(), "0".into()),
            ("X3F_ADJUSTMENT_MONO_MIX_GREEN".into(), "0".into()),
            ("X3F_ADJUSTMENT_MONO_MIX_BLUE".into(), "0".into()),
        ]
        .into(),
    )
    .unwrap();
    assert_eq!(zero.mono_mix, Some([1. / 3.; 3]));
}
