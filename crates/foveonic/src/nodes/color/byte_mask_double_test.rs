//! Verifies cubic byte-mask doubling against native bounded and whole-image captures.
use super::*;
#[test]
fn rejects_empty_geometry() {
    assert!(Params::new([0, 1]).is_err());
}
#[test]
#[ignore = "requires BLUE_GUIDED_REFERENCE and GPU"]
fn bounded_native_byte_resize_matches() {
    let dir = std::path::PathBuf::from(std::env::var_os("BLUE_GUIDED_REFERENCE").unwrap());
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("mask-cases.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let extent = [
            case["extent"][0].as_u64().unwrap() as u32,
            case["extent"][1].as_u64().unwrap() as u32,
        ];
        let input = std::fs::read(dir.join(format!("{name}-input.u8")))
            .unwrap()
            .into_iter()
            .map(u32::from)
            .collect::<Vec<_>>();
        let input = gpu.upload("Native low byte mask", &input).unwrap();
        let out = apply(
            &gpu,
            &input,
            &Params::new(extent).unwrap(),
            &mut Profile::default(),
        )
        .unwrap();
        let bytes = gpu.readback(&out).unwrap();
        let actual = bytemuck::cast_slice::<u8, u32>(&bytes);
        let expected = std::fs::read(dir.join(format!("{name}-output.u8")))
            .unwrap()
            .into_iter()
            .map(u32::from)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{name}");
    }
}
#[test]
#[ignore = "requires HIGHLIGHT_NATIVE_BOUNDARY, STAGE2B_NATIVE_BOUNDARY and GPU"]
fn full_actual_highlight_mask_matches_stage2b_input() {
    let low = std::path::PathBuf::from(std::env::var_os("HIGHLIGHT_NATIVE_BOUNDARY").unwrap());
    let high = std::path::PathBuf::from(std::env::var_os("STAGE2B_NATIVE_BOUNDARY").unwrap());
    let input = std::fs::read(low.join("restoration-output-mask.u8"))
        .unwrap()
        .into_iter()
        .map(u32::from)
        .collect::<Vec<_>>();
    let expected = std::fs::read(high.join("apply-mask.bin"))
        .unwrap()
        .into_iter()
        .map(u32::from)
        .collect::<Vec<_>>();
    let gpu = Gpu::new().unwrap();
    let input = gpu.upload("Native green highlight mask", &input).unwrap();
    let mut profile = Profile::new(crate::profile::Mode::Gpu);
    let out = apply(
        &gpu,
        &input,
        &Params::new([2944, 1836]).unwrap(),
        &mut profile,
    )
    .unwrap();
    let bytes = gpu.readback(&out).unwrap();
    let actual = bytemuck::cast_slice::<u8, u32>(&bytes);
    let bad: Vec<_> = actual
        .iter()
        .zip(&expected)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, (a, b))| (i, *a, *b))
        .take(10)
        .collect();
    eprintln!("Doubled actual mask differences{bad:?} {:?}", profile.gpu);
    assert!(bad.is_empty());
}
