//! Checks source interpolation tables and native crop/rotate cases.
use super::*;
#[test]
fn rejects_invalid_crop_and_rotation() {
    assert!(Prepared::new([13, 11], [0, 0, 14, 11], [13, 11], 0, false).is_err());
    assert!(Prepared::new([13, 11], [0, 0, 13, 11], [13, 11], 45, false).is_err());
}
#[test]
#[ignore = "requires CODE_CROP_REFERENCE"]
fn kernel_tables_match_native() {
    let dir = std::path::PathBuf::from(std::env::var_os("CODE_CROP_REFERENCE").unwrap());
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("kernels.json")).unwrap()).unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let actual = interpolation_kernel(
            case["scales"][0].as_f64().unwrap(),
            case["scales"][1].as_f64().unwrap(),
        );
        let expected = std::fs::read(dir.join(format!("{name}.f32"))).unwrap();
        assert_eq!(bytemuck::cast_slice::<f32, u8>(&actual), expected, "{name}");
    }
}
#[test]
#[ignore = "requires CODE_CROP_REFERENCE and GPU"]
fn native_crop_resize_and_rotations_match() {
    let dir = std::path::PathBuf::from(std::env::var_os("CODE_CROP_REFERENCE").unwrap());
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("cases.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let pair = |key: &str| {
            [
                case[key][0].as_u64().unwrap() as u32,
                case[key][1].as_u64().unwrap() as u32,
            ]
        };
        let crop = std::array::from_fn(|i| case["crop"][i].as_u64().unwrap() as u32);
        let params = Prepared::new(
            pair("input"),
            crop,
            pair("output"),
            case["rotation"].as_u64().unwrap() as u32,
            case["clamp15"].as_bool().unwrap(),
        )
        .unwrap();
        let input = std::fs::read(dir.join(format!("{name}-input.u16"))).unwrap();
        let input = input
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| f32::from(u16::from_le_bytes(*v)))
            .collect::<Vec<_>>();
        let input = gpu.upload("Native code crop input", &input).unwrap();
        let output = apply(&gpu, &input, &params, &mut Profile::default()).unwrap();
        let b = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&b);
        let expected = std::fs::read(dir.join(format!("{name}-output.u16"))).unwrap();
        let expected = expected
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| f32::from(u16::from_le_bytes(*v)))
            .collect::<Vec<_>>();
        let bad: Vec<_> = actual
            .iter()
            .zip(&expected)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| (i, *a, *b))
            .take(10)
            .collect();
        assert!(bad.is_empty(), "{name}: {bad:?}");
    }
}
#[test]
#[ignore = "requires CODE_CROP_NATIVE_BOUNDARY and GPU"]
fn actual_fullsize_mapping_and_patch_samples_match() {
    let directory =
        std::path::PathBuf::from(std::env::var_os("CODE_CROP_NATIVE_BOUNDARY").unwrap());
    let controls: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("controls.json")).unwrap()).unwrap();
    let case = controls["crop_comparisons"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["kind"] == "rgb")
        .unwrap();
    let prepared =
        Prepared::new([5888, 3672], [216, 28, 5639, 3643], [5424, 3616], 0, true).unwrap();
    assert_eq!(prepared.params.column[2], 65523);
    let gpu = Gpu::new().unwrap();
    for sample in case["samples"].as_array().unwrap() {
        let x = sample["xy"][0].as_u64().unwrap() as u32;
        let y = sample["xy"][1].as_u64().unwrap() as u32;
        let xx = prepared.params.column[0]
            .wrapping_add(x.wrapping_mul(prepared.params.column[2]))
            .wrapping_add(y.wrapping_mul(prepared.params.row[0]));
        let yy = prepared.params.column[1]
            .wrapping_add(x.wrapping_mul(prepared.params.column[3]))
            .wrapping_add(y.wrapping_mul(prepared.params.row[1]));
        assert_eq!(
            xx,
            u32::try_from(sample["source_q16"][0].as_u64().unwrap()).unwrap()
        );
        assert_eq!(
            yy,
            u32::try_from(sample["source_q16"][1].as_u64().unwrap()).unwrap()
        );
        // Move the captured4x4 support into an8x8 owned test image while retaining
        // its exact fractional phase and the full-image interpolation kernel.
        let mut local = prepared.clone();
        local.params.shape = [8, 8, 1, 1];
        local.params.column = [(3 << 16) | (xx & 65535), (3 << 16) | (yy & 65535), 0, 0];
        local.params.row[0] = 0;
        local.params.row[1] = 0;
        let patch = sample["source_patch"].as_array().unwrap();
        let mut input = vec![0_f32; 8 * 8 * 3];
        for dy in 0..4 {
            for dx in 0..4 {
                for c in 0..3 {
                    input[((dy + 2) * 8 + dx + 2) * 3 + c] =
                        patch[(dy * 4 + dx) * 3 + c].as_f64().unwrap() as f32;
                }
            }
        }
        let input = gpu.upload("Actual crop4x4 support", &input).unwrap();
        let output = apply(&gpu, &input, &local, &mut Profile::default()).unwrap();
        let b = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&b);
        let expected: Vec<_> = sample["actual"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect();
        assert_eq!(actual, expected, "sample{x},{y}");
    }
}
