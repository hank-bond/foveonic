//! Compares the complete finite-input helper and its two retained numerical boundaries.

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
