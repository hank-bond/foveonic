//! Checks adaptive proportional sharpening limits, rounding and highlighted overflow.
use super::*;
#[test]
fn rejects_invalid_controls() {
    assert!(Prepared::new([1, 1], vec![], 1., 100., 0.35, 1., true).is_err());
}
#[test]
#[ignore = "requires GPU and STAGE3_NATIVE"]
fn complete_original_sharpen_matches_native() {
    let directory =
        std::path::PathBuf::from(std::env::var_os("STAGE3_NATIVE").expect("set STAGE3_NATIVE"));
    let read16 = |name: &str, signed: bool| -> Vec<f32> {
        let b = std::fs::read(directory.join(name)).unwrap();
        b.as_chunks::<2>()
            .0
            .iter()
            .map(|v| {
                if signed {
                    f32::from(i16::from_le_bytes(*v))
                } else {
                    f32::from(u16::from_le_bytes(*v))
                }
            })
            .collect()
    };
    let b = std::fs::read(directory.join("sharp-lut.f32")).unwrap();
    let table = b
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| f32::from_le_bytes(*v))
        .collect();
    let data = std::fs::read(directory.join("sharp-data.bin")).unwrap();
    let field = |offset| f32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
    let receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("boundary.json")).unwrap()).unwrap();
    let extent = std::array::from_fn(|i| receipt["input"]["extent"][i].as_u64().unwrap() as u32);
    let prepared = Prepared::new(
        extent,
        table,
        field(0x3c),
        field(0x44),
        field(0x48),
        1.,
        true,
    )
    .unwrap();
    let gpu = Gpu::new().unwrap();
    let input = gpu
        .upload(
            "Original RGB sharpening input",
            &read16("sharp-input-rgb.u16", false),
        )
        .unwrap();
    let detail = gpu
        .upload(
            "Original quarter-code detail",
            &read16("sharp-input-detail.i16", true),
        )
        .unwrap();
    let start = std::time::Instant::now();
    let output = apply(&gpu, &input, &detail, &prepared, &mut Default::default()).unwrap();
    let b = gpu.readback(&output).unwrap();
    let actual = bytemuck::cast_slice::<u8, f32>(&b);
    let expected = read16("sharp-output-rgb.u16", false);
    eprintln!("Original sharpening {:?}", start.elapsed());
    assert!(
        receipt["wrapper_output"].as_u64().is_some(),
        "requires enclosing wrapper return after all workers join"
    );
    let mut differences = 0;
    let mut maximum = 0_f32;
    for (i, (&a, &b)) in actual.iter().zip(&expected).enumerate() {
        differences += usize::from(a != b);
        maximum = maximum.max((a - b).abs());
        assert_eq!(a, b, "sample{i} {a} vs {b}");
    }
    eprintln!("Original sharpening differences{differences} max{maximum}");
}
#[test]
#[ignore = "requires GPU and SHARPEN_REFERENCE"]
fn proportional_limits_and_overflow_match_independent_equations() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("SHARPEN_REFERENCE").expect("set SHARPEN_REFERENCE"),
    );
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("cases.json")).unwrap()).unwrap();
    let read = |name: &str| -> Vec<f32> {
        let b = std::fs::read(directory.join(name)).unwrap();
        b.as_chunks::<4>()
            .0
            .iter()
            .map(|v| f32::from_le_bytes(*v))
            .collect()
    };
    let gpu = Gpu::new().unwrap();
    let input = gpu
        .upload("Sharpen limit fixtures", &read("input.f32"))
        .unwrap();
    let detail = gpu
        .upload("Quarter-code endpoint fixtures", &read("detail.f32"))
        .unwrap();
    for c in cases.as_array().unwrap() {
        let n = c["n"].as_u64().unwrap();
        let prepared = Prepared::new(
            [8, 7],
            vec![1.15],
            c["normalization"].as_f64().unwrap() as f32,
            10000.,
            0.35,
            c["scale"].as_f64().unwrap() as f32,
            c["preserve"].as_bool().unwrap(),
        )
        .unwrap();
        let output = apply(&gpu, &input, &detail, &prepared, &mut Default::default()).unwrap();
        let b = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&b);
        let expected = read(&format!("{n}-output.f32"));
        assert_eq!(actual, expected, "case{n}");
    }
}
