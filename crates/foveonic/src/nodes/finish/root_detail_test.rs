//! Checks direct root conversion and signed detail against native crop and high-pass boundaries.
use super::*;
#[test]
fn rejects_invalid_detail_geometry() {
    assert!(Prepared::new([8, 8], [0, 0, 9, 8], 0.001, 200, 1.).is_err());
    assert!(Prepared::new([8, 8], [0, 0, 8, 8], 0.001, 200, 0.).is_err());
}
#[test]
#[ignore = "requires a GPU"]
fn gpu_root_and_detail_match_native_direct_crop() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/data/root-detail-reference.json"
    ))
    .unwrap();
    let input: Vec<f32> = f["input"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap() as f32)
        .collect();
    let crop = std::array::from_fn(|i| f["crop"][i].as_u64().unwrap() as u32);
    let extent = [
        f["width"].as_u64().unwrap() as u32,
        f["height"].as_u64().unwrap() as u32,
    ];
    let p = Prepared::new(extent, crop, 0.001, 200, f["scale"].as_f64().unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    let source = gpu.upload("Native direct crop fixture", &input).unwrap();
    let out = apply(&gpu, &source, &p, &mut Profile::default()).unwrap();
    for (name, buffer, channels) in [("rgb", &out.rgb, 3), ("detail", &out.detail, 1)] {
        let bytes = gpu.readback(buffer).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        let mut i = 0;
        for y in crop[1]..crop[3] {
            for x in crop[0]..crop[2] {
                for c in 0..channels {
                    assert_eq!(
                        actual[((y * extent[0] + x) * channels + c) as usize],
                        f[name][i].as_i64().unwrap() as f32,
                        "{name} sample {i}"
                    );
                    i += 1;
                }
            }
        }
    }
    assert_eq!(
        gpu.readback(&source).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&input)
    );
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_detail_axis_order_matches_native_quarter_turns() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/data/root-detail-rotated-reference.json"
    ))
    .unwrap();
    let gpu = Gpu::new().unwrap();
    for f in fixture["cases"].as_array().unwrap() {
        let input: Vec<f32> = f["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap() as f32)
            .collect();
        let crop = std::array::from_fn(|i| f["crop"][i].as_u64().unwrap() as u32);
        let extent = [
            f["width"].as_u64().unwrap() as u32,
            f["height"].as_u64().unwrap() as u32,
        ];
        let angle = f["angle"].as_u64().unwrap();
        let p = Prepared::new(extent, crop, 0.001, 200, f["scale"].as_f64().unwrap()).unwrap();
        let p = if matches!(angle, 90 | 270) {
            p.with_horizontal_first()
        } else {
            p
        };
        let source = gpu.upload("Native rotated detail fixture", &input).unwrap();
        let output = apply(&gpu, &source, &p, &mut Profile::default()).unwrap();
        let [width, height] = if matches!(angle, 90 | 270) {
            [crop[3] - crop[1], crop[2] - crop[0]]
        } else {
            [crop[2] - crop[0], crop[3] - crop[1]]
        };
        for (name, buffer, channels) in [("rgb", &output.rgb, 3), ("detail", &output.detail, 1)] {
            let bytes = gpu.readback(buffer).unwrap();
            let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
            for y in 0..height {
                for x in 0..width {
                    let [sx, sy] = match angle {
                        0 => [crop[0] + x, crop[1] + y],
                        90 => [crop[0] + y, crop[3] - 1 - x],
                        180 => [crop[2] - 1 - x, crop[3] - 1 - y],
                        270 => [crop[2] - 1 - y, crop[1] + x],
                        _ => unreachable!(),
                    };
                    for c in 0..channels {
                        let expected = f[name][((y * width + x) * channels + c) as usize]
                            .as_i64()
                            .unwrap() as f32;
                        assert_eq!(
                            actual[((sy * extent[0] + sx) * channels + c) as usize],
                            expected,
                            "{name}, {angle}deg, {x},{y}, channel{c}"
                        );
                    }
                }
            }
        }
    }
}
