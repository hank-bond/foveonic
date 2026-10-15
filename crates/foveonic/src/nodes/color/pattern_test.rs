//! Checks deterministic native offsets, masked prefix ordering, edge-seeded extrema, and immutable ratio inputs.
use super::*;
use serde_json::Value;
fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../tests/data/pattern-reference.json")).unwrap()
}
fn array(v: &Value) -> Vec<f32> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap() as f32)
        .collect()
}
#[test]
fn offsets_match_native_sequence_and_invalid_parameters_are_rejected() {
    let f = fixture();
    let p = Prepared::new([80, 72, 1, 4], [0.075, 0.1, 0.65, 0.]).unwrap();
    assert!(p
        .offsets
        .iter()
        .zip(f["offsets"].as_array().unwrap())
        .all(|(&a, b)| u64::from(a) == b.as_u64().unwrap()));
    assert_eq!(p.offsets.len(), 9 * 512);
    assert!(Prepared::new([80, 72, 1, 5], [0.075, 0.1, 0.65, 0.]).is_err());
    assert!(Prepared::new([80, 72, 1, 4], [0.075, 0.1, f32::NAN, 0.]).is_err());
    assert!(Prepared::new([80, 72, 1, 4], [0.075, 0.1, 1.1, 0.]).is_err());
}
#[test]
#[ignore = "requires a GPU"]
fn gpu_pattern_matches_native_extents_seeded_edges_and_global_offset_order() {
    let f = fixture();
    let source = array(&f["input"]);
    let edges = array(&f["edges"]);
    let gpu = Gpu::new().unwrap();
    // Two independently initialized tiles must each restart both native offset sequences.
    let input: Vec<f32> = source.iter().chain(&source).copied().collect();
    let seed: Vec<f32> = edges.iter().chain(&edges).copied().collect();
    let original = gpu.upload("Native pattern ratio fixture", &input).unwrap();
    let edge_raw = gpu.upload("Original edge fixture", &seed).unwrap();
    let mut geometry: crate::noise::Parameters = bytemuck::Zeroable::zeroed();
    geometry.source = [80, 72, 2, 1];
    geometry.tile = [80, 72, 0, 11];
    geometry.reduction = [1., 0., 0., 0.];
    geometry.geometry = [8, 8, 21, 0];
    let edge = crate::nodes::color::reconstruction::pattern_seed(
        &gpu,
        &edge_raw,
        geometry,
        &mut Profile::default(),
    )
    .unwrap();
    let seed_before = gpu.readback(&edge).unwrap();
    for output in f["outputs"].as_array().unwrap() {
        let blend = output["blend"].as_f64().unwrap() as f32;
        let p = Prepared::new([80, 72, 2, 4], [0.075, 0.1, blend, 0.]).unwrap();
        let result = apply(
            &gpu,
            &original,
            &edge,
            &p,
            true,
            true,
            &mut Profile::default(),
        )
        .unwrap();
        let bytes = gpu.readback(result.ratio.as_ref().unwrap()).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        let expected = array(&output["ratio"]);
        let (mut error, mut position) = (0_f32, 0);
        for (i, &a) in actual.iter().enumerate() {
            assert!(a.is_finite());
            let d = (a - expected[i % source.len()]).abs();
            if d > error {
                error = d;
                position = i;
            }
        }
        assert!(
            error < 0.0000003,
            "pattern blend {blend}: maximum {error} at {position}"
        );
        let mask = gpu.readback(result.mask.as_ref().unwrap()).unwrap();
        let mask = bytemuck::cast_slice::<u8, f32>(&mask);
        for (i, &a) in mask.iter().enumerate() {
            assert_eq!(
                a,
                f["mask"][i % source.len()].as_u64().unwrap() as f32 * 4095.
            );
        }
        let unchanged = gpu.readback(&original).unwrap();
        assert!(bytemuck::cast_slice::<u8, f32>(&unchanged)
            .iter()
            .zip(&input)
            .all(|(a, b)| a.to_bits() == b.to_bits()));
        assert!(
            gpu.readback(&edge).unwrap() == seed_before,
            "conditioned ratio changed seed"
        );
        let unchanged = gpu.readback(&edge_raw).unwrap();
        assert!(bytemuck::cast_slice::<u8, f32>(&unchanged)
            .iter()
            .zip(&seed)
            .all(|(a, b)| a.to_bits() == b.to_bits()));
    }
    let zero = gpu
        .upload("Zero luminance ratios", &vec![0_f32; input.len()])
        .unwrap();
    let p = Prepared::new([80, 72, 2, 4], [0.075, 0.1, 0.65, 0.]).unwrap();
    let result = apply(&gpu, &zero, &edge, &p, true, false, &mut Profile::default()).unwrap();
    let bytes = gpu.readback(result.ratio.as_ref().unwrap()).unwrap();
    assert!(bytemuck::cast_slice::<u8, f32>(&bytes)
        .iter()
        .all(|v| *v == 0.));
}
