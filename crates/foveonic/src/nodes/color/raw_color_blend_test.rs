//! Compares the complete finite-input helper, its two retained numerical boundaries and the fused chroma limit.

use super::*;

#[test]
#[ignore = "requires a GPU and RAW_COLOR_BLEND_REFERENCE guarded native helper fixtures"]
fn complete_native_raw_color_helper() {
    let directory =
        std::path::PathBuf::from(std::env::var_os("RAW_COLOR_BLEND_REFERENCE").unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in manifest["cases"].as_array().unwrap() {
        let scalar = |name: &str| case[name].as_f64().unwrap() as f32;
        let array = |name: &str| {
            case[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect::<Vec<_>>()
        };
        let floats = |name: &str| {
            let bytes = std::fs::read(directory.join(case[name].as_str().unwrap())).unwrap();
            bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()
        };
        let mut prepared = Prepared::new(
            Controls {
                extent: [
                    case["extent"][0].as_u64().unwrap() as u32,
                    case["extent"][1].as_u64().unwrap() as u32,
                ],
                white_balance: array("white_balance")
                    .into_iter()
                    .map(|v| v as f32)
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap(),
                normalization: scalar("normalization"),
                neutral_low: scalar("neutral_low"),
                neutral_high: scalar("neutral_high"),
                matrix: array("matrix").try_into().unwrap(),
                input_divisor: scalar("input_divisor"),
                output_gain: scalar("output_gain"),
                brightness_factor: scalar("brightness_factor"),
                brightness_offset: scalar("brightness_offset"),
                brightness_gain: scalar("brightness_gain"),
                second_exponent: scalar("second_exponent"),
                residual_scale: scalar("residual_scale"),
                hue_strength: array("hue_strength")
                    .into_iter()
                    .map(|v| v as f32)
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap(),
            },
            floats("brightness"),
            floats("signed_response"),
            floats("adaptive"),
        )
        .unwrap();
        let mask = case.get("mask").and_then(|v| v.as_str()).map(|name| {
            let bytes = std::fs::read(directory.join(name)).unwrap();
            gpu.upload(
                "Recovered channel flags",
                bytemuck::cast_slice::<u8, u32>(&bytes),
            )
            .unwrap()
        });
        if case["masked_highlights"].as_bool().unwrap_or(false) {
            prepared = prepared
                .with_masked_highlights([22000., 24000., 14000.], scalar("highlight_strength"))
                .unwrap();
        }
        let input = gpu
            .upload("Native raw-color helper input", &floats("input"))
            .unwrap();
        let output = apply(
            &gpu,
            &input,
            mask.as_ref(),
            &prepared,
            &mut Default::default(),
        )
        .unwrap();
        for (name, buffer) in [
            ("neutralized", &output.neutralized),
            ("raw_color", &output.raw_color),
            ("output", &output.rgb),
        ] {
            let bytes = gpu.readback(buffer).unwrap();
            let actual: &[f32] = bytemuck::cast_slice(&bytes);
            let expected = floats(name);
            assert_eq!(actual.len(), expected.len());
            let mut maximum = 0_f32;
            let mut sum = 0_f64;
            let mut changed = 0;
            for (index, (&a, &b)) in actual.iter().zip(&expected).enumerate() {
                let margin = if name == "output" {
                    case["margin"].as_u64().unwrap_or(0) as usize
                } else {
                    0
                };
                let width = prepared.extent()[0] as usize;
                let height = prepared.extent()[1] as usize;
                let x = (index / 3) % width;
                let y = (index / 3) / width;
                if x < margin || y < margin || x + margin >= width || y + margin >= height {
                    continue;
                }
                assert!(a.is_finite() && b.is_finite());
                maximum = maximum.max((a - b).abs());
                sum += f64::from((a - b).abs());
                changed += usize::from(a.to_bits() != b.to_bits());
            }
            eprintln!(
                "{} {name}: {changed}/{} changed, max {maximum}, mean {}",
                case["name"],
                actual.len(),
                sum / actual.len() as f64
            );
            assert!(
                maximum <= 0.015,
                "{} {name}: maximum {maximum}",
                case["name"]
            );
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn chroma_limit_matches_a_separate_clamp_and_conversion() {
    let gpu = Gpu::new().unwrap();
    let [width, height] = [19, 13];
    let samples: Vec<_> = (0..width * height)
        .map(|i| {
            [
                128. + i as f32,
                (i * 29 % 67) as f32 - 35.,
                (i * 7 % 23) as f32 - 12.,
            ]
        })
        .collect();
    let mut limited = samples.clone();
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let center = (y as u32 * width + x as u32) as usize;
            for channel in 1..3 {
                let mut neighbors = Vec::with_capacity(48);
                for dy in -3..=3 {
                    for dx in -3..=3 {
                        if (dx, dy) == (0, 0) {
                            continue;
                        }
                        let row = (y + dy).clamp(0, height as i32 - 1) as u32;
                        let column = (x + dx).clamp(0, width as i32 - 1) as u32;
                        neighbors.push(samples[(row * width + column) as usize][channel]);
                    }
                }
                neighbors.sort_by(f32::total_cmp);
                limited[center][channel] =
                    samples[center][channel].clamp(neighbors[1], neighbors[46]);
            }
        }
    }
    assert_ne!(limited, samples);
    let extent = [width, height];
    let upload = |samples: &[[f32; 3]]| gpu.upload("Chroma limit test input", samples).unwrap();
    let fused = limit_chroma(&gpu, &upload(&samples), extent, &mut Profile::default()).unwrap();
    let separate = uv_repair::convert(
        &gpu,
        &upload(&limited),
        extent,
        uv_repair::YUV_TO_RGB,
        &mut Profile::default(),
    )
    .unwrap();
    assert_eq!(
        gpu.readback(&fused).unwrap(),
        gpu.readback(&separate).unwrap()
    );
}
