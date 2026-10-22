//! Checks complete guarded native sector-selection callbacks on finite RGB.

use super::*;

#[test]
#[ignore = "requires a GPU and SECTOR_COLOR_REFERENCE guarded native callback fixtures"]
fn complete_native_sector_selection() {
    let directory = std::path::PathBuf::from(std::env::var_os("SECTOR_COLOR_REFERENCE").unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in manifest["cases"].as_array().unwrap() {
        let values = |name: &str| {
            case[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap() as f32)
                .collect::<Vec<_>>()
        };
        let sectors = case["sectors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| Sector {
                matrix: s["matrix"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_f64().unwrap() as f32)
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap(),
                quadrant: s["quadrant"].as_u64().unwrap() as u32,
                bin: s["bin"].as_u64().unwrap() as u32,
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let p = Prepared::new(
            [
                case["extent"][0].as_u64().unwrap() as u32,
                case["extent"][1].as_u64().unwrap() as u32,
            ],
            values("chroma").try_into().unwrap(),
            values("boundaries").try_into().unwrap(),
            sectors,
        )
        .unwrap();
        assert_eq!(
            p.extent(),
            [
                case["extent"][0].as_u64().unwrap() as u32,
                case["extent"][1].as_u64().unwrap() as u32
            ]
        );
        let _key = p.key();
        let bytes = std::fs::read(directory.join(case["input"].as_str().unwrap())).unwrap();
        let input = gpu
            .upload(
                "Native sector callback input",
                bytemuck::cast_slice::<u8, f32>(&bytes),
            )
            .unwrap();
        let output = apply(&gpu, &input, &p, &mut Default::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        let bytes = std::fs::read(directory.join(case["output"].as_str().unwrap())).unwrap();
        let expected: &[f32] = bytemuck::cast_slice(&bytes);
        let mut maximum = 0_f32;
        let mut changed = 0;
        for (&a, &b) in actual.iter().zip(expected) {
            assert!(a.is_finite() && b.is_finite());
            maximum = maximum.max((a - b).abs());
            changed += usize::from(a.to_bits() != b.to_bits());
        }
        eprintln!(
            "{}: {changed}/{} changed, max {maximum}",
            case["name"],
            actual.len()
        );
        assert!(maximum <= 0.002);
    }
}
