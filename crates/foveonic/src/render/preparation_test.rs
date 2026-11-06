//! Checks bounded preparation ownership, exact invalidation and sharing across repeated edits.

use super::*;
use crate::source::{Matrix, MatrixEncoding};

fn metadata() -> Arc<Metadata> {
    let mut value = Metadata {
        width: 64,
        height: 128,
        rotation: 0,
        processing_version: None,
        properties: Default::default(),
        matrices: Default::default(),
        lists: Default::default(),
    };
    for (name, values) in [
        ("ActiveImageArea", vec![8., 34., 48., 64.]),
        ("DarkShieldColRange", vec![0., 2., 56., 63.]),
        ("SatMapR", vec![8192.]),
        ("SatMapG", vec![8192.]),
        ("SatMapB", vec![8192.]),
    ] {
        value.matrices.insert(
            name.into(),
            Matrix {
                encoding: MatrixEncoding::Uint32,
                representation: "u32",
                dimensions: vec![values.len()],
                values,
            },
        );
    }
    Arc::new(value)
}
fn mode() -> color_mode::Settings {
    color_mode::Settings {
        tone_index: 0,
        contrast: 0.,
        compensation: None,
    }
}
#[test]
fn tone_preparation_reuses_exact_inputs_and_bounds_changed_responses() {
    let metadata = metadata();
    let mut cache = Cache::default();
    let original = cache
        .tone(&metadata, &mode(), Default::default(), false)
        .unwrap();
    let repeat = cache
        .tone(&metadata, &mode(), Default::default(), false)
        .unwrap();
    assert!(original.shares_storage(&repeat));
    assert_eq!(original.key(), repeat.key());
    assert_eq!(
        &*original,
        tone::prepare(&metadata, &mode(), Default::default()).unwrap()
    );
    assert_eq!(Arc::strong_count(&metadata), 1);
    for controls in [
        tone::Controls {
            contrast: 0.2,
            ..Default::default()
        },
        tone::Controls {
            shadows: 0.2,
            ..Default::default()
        },
        tone::Controls {
            highlights: 0.2,
            ..Default::default()
        },
    ] {
        let changed = cache.tone(&metadata, &mode(), controls, false).unwrap();
        assert!(!original.shares_storage(&changed));
        assert_eq!(
            &*changed,
            tone::prepare(&metadata, &mode(), controls).unwrap()
        );
        assert!(cache.bytes() <= TONE_CAPACITY * (65536 * 4 + 1024));
    }
    let mut selected = mode();
    selected.contrast = 0.1;
    let changed_mode = cache
        .tone(&metadata, &selected, Default::default(), false)
        .unwrap();
    assert_eq!(
        &*changed_mode,
        tone::prepare(&metadata, &selected, Default::default()).unwrap()
    );
    selected.compensation = Some([1.; 9]);
    assert!(changed_mode.shares_storage(
        &cache
            .tone(&metadata, &selected, Default::default(), false)
            .unwrap()
    ));
    let monochrome = cache
        .tone(&metadata, &selected, Default::default(), true)
        .unwrap();
    assert_eq!(
        &*monochrome,
        tone::prepare_monochrome(&metadata, Default::default()).unwrap()
    );
}
#[test]
fn immutable_metadata_identity_separates_sources_without_retaining_them() {
    let first = metadata();
    let second = Arc::new((*first).clone());
    let mut cache = Cache::default();
    let a = cache
        .tone(&first, &mode(), Default::default(), false)
        .unwrap();
    let b = cache
        .tone(&second, &mode(), Default::default(), false)
        .unwrap();
    assert!(!a.shares_storage(&b));
    assert_eq!(a, b);
    assert_eq!(cache.tones.len(), 1);
    assert_eq!(Arc::strong_count(&first), 1);
    assert_eq!(Arc::strong_count(&second), 1);
}
#[test]
fn highlight_thresholds_share_clipping_evidence_and_remain_exact() {
    let metadata = metadata();
    let mut cache = Cache::default();
    let original = cache.highlights(&metadata, None).unwrap();
    let repeated = cache.highlights(&metadata, None).unwrap();
    assert!(Arc::ptr_eq(&original, &repeated));
    let changed = cache.highlights(&metadata, Some(-1.)).unwrap();
    assert!(!Arc::ptr_eq(&original, &changed));
    assert!(original.flags.shares_storage(&changed.flags));
    assert_ne!(original.key(), changed.key());
    assert_eq!(
        changed.key(),
        highlight::prepare(&metadata, Some(-1.)).unwrap().key()
    );
    assert_eq!(cache.bytes(), metadata.width * metadata.height * 4);
    assert!(cache.highlights(&metadata, Some(f32::NAN)).is_err());
    assert!(Arc::ptr_eq(
        &changed,
        &cache.highlights(&metadata, Some(-1.)).unwrap()
    ));
    let other = Arc::new((*metadata).clone());
    let changed_source = cache.highlights(&other, Some(-1.)).unwrap();
    assert!(!changed_source.flags.shares_storage(&changed.flags));
}

#[test]
fn preparation_inventory_deduplicates_payloads_retained_by_graph_keys() {
    let metadata = metadata();
    let mut cache = Cache::default();
    let curve = cache
        .tone(&metadata, &mode(), Default::default(), false)
        .unwrap();
    let highlights = cache.highlights(&metadata, None).unwrap();
    let curve_key = curve.key();
    let highlight_key = highlights.key();
    let mut retained = RetainedBytes::default();
    cache.account(&mut retained);
    let before = retained.bytes();
    curve.account(&mut retained);
    highlights.flags.account(&mut retained);
    assert_eq!(retained.bytes(), before);
    curve_key.account(&mut retained);
    highlight_key.account(&mut retained);
    // Additional graph-owned key vectors and scalar parameters remain small.
    assert!(retained.bytes() - before < 1024);
}
