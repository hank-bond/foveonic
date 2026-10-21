//! Checks the complete red-detail pass against independently captured signed samples.

use super::*;

#[test]
#[ignore = "requires RED_DETAIL_REFERENCE containing input.i16, output.i16, entry.json"]
fn complete_reference_red_pass_matches() {
    let directory = std::path::PathBuf::from(std::env::var("RED_DETAIL_REFERENCE").unwrap());
    let entry: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("entry.json")).unwrap()).unwrap();
    let params = Params {
        shape: [
            entry["width"].as_u64().unwrap() as u32,
            entry["height"].as_u64().unwrap() as u32,
            entry["amount"].as_u64().unwrap() as u32,
            0,
        ],
        weights: [
            entry["left"].as_i64().unwrap() as i32,
            entry["right"].as_i64().unwrap() as i32,
            0,
            0,
        ],
    };
    let source = std::fs::read(directory.join("input.i16")).unwrap();
    let source: Vec<f32> = source
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| f32::from(i16::from_le_bytes(*v)))
        .collect();
    let gpu = Gpu::new().unwrap();
    let source = gpu.upload("Red reference", &source).unwrap();
    let output = apply(&gpu, &source, &params, &mut Default::default()).unwrap();
    let bytes = gpu.readback(&output).unwrap();
    let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
    let expected = std::fs::read(directory.join("output.i16")).unwrap();
    assert_eq!(actual.len() * 2, expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected.as_chunks::<2>().0).enumerate() {
        assert_eq!(
            *actual,
            f32::from(i16::from_le_bytes(*expected)),
            "sample {index}"
        );
    }
}
