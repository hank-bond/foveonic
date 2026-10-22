//! Checks the fixed-radius iteration against complete ordinary native helper calls.

use super::*;

#[test]
fn parameter_contract_rejects_invalid_radii_and_range_tables() {
    let create = |radius, weights| {
        Prepared::new(
            [3, 5],
            [1.; 3],
            [1., 1., 1., 0., 0., 0., 0., 0., 0.],
            [1., 0., 0., 0., 1., 0., 0., 0., 1.],
            vec![1.; 14385],
            weights,
            1.,
            radius,
        )
    };
    assert!(create(5, vec![1.; 2048]).is_ok());
    assert!(create(0, vec![1.; 2048]).is_err());
    assert!(create(32, vec![1.; 2048]).is_err());
    assert!(create(5, vec![1.; 2047]).is_err());
    assert!(create(5, vec![f32::NAN; 2048]).is_err());
    let a = create(5, vec![1.; 2048]).unwrap();
    let b = create(6, vec![1.; 2048]).unwrap();
    assert!(a.key() != b.key());
}

#[test]
#[ignore = "requires a GPU and RGB_PREFILTER_REFERENCE complete native helper fixtures"]
fn complete_fixed_radius_iterations_match_reference() {
    let directory = std::path::PathBuf::from(std::env::var_os("RGB_PREFILTER_REFERENCE").unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for fixture in manifest["cases"].as_array().unwrap() {
        let read =
            |name: &str| std::fs::read(directory.join(fixture[name].as_str().unwrap())).unwrap();
        let floats = |name: &str| {
            let data = read(name);
            bytemuck::cast_slice::<u8, f32>(&data).to_vec()
        };
        let doubles = |name: &str| {
            fixture[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect::<Vec<_>>()
        };
        let extent = [
            fixture["extent"][0].as_u64().unwrap() as u32,
            fixture["extent"][1].as_u64().unwrap() as u32,
        ];
        let params = Prepared::new(
            extent,
            doubles("gains").try_into().unwrap(),
            doubles("model").try_into().unwrap(),
            doubles("variance_mix").try_into().unwrap(),
            floats("intensity"),
            floats("range"),
            fixture["strength"].as_f64().unwrap() as f32,
            fixture["radius"].as_u64().unwrap() as u32,
        )
        .unwrap();
        let input = gpu
            .upload("Original native iteration input", &floats("input"))
            .unwrap();
        let result = apply(&gpu, &input, &params, &mut Default::default()).unwrap();
        for (name, output) in [
            ("output", result.rgb),
            ("source_after", result.source_after),
        ] {
            let actual = gpu.readback(&output).unwrap();
            let expected = floats(name);
            let actual = bytemuck::cast_slice::<u8, f32>(&actual);
            assert_eq!(actual.len(), expected.len());
            let mut changed = 0;
            let mut sum = 0_f64;
            let mut maximum = 0_f32;
            for (&actual, &expected) in actual.iter().zip(&expected) {
                assert!(actual.is_finite());
                changed += usize::from(actual.to_bits() != expected.to_bits());
                let difference = (actual - expected).abs();
                sum += f64::from(difference);
                maximum = maximum.max(difference);
            }
            eprintln!(
                "{} {name}: {changed}/{} differing words, mean {}, max {maximum}",
                fixture["name"],
                actual.len(),
                sum / actual.len() as f64
            );
            if name == "source_after" {
                assert_eq!(changed, 0, "WB round trip differs");
            }
            assert!(
                maximum <= 0.002,
                "{} {name}: maximum {maximum}",
                fixture["name"]
            );
        }
    }
}

#[test]
#[ignore = "requires a GPU and RGB_RANGE_REFERENCE standalone native bilateral fixtures"]
fn standalone_range_matches_native() {
    use super::{apply_range, RangePrepared};
    let root = std::path::PathBuf::from(std::env::var_os("RGB_RANGE_REFERENCE").unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let floats = |name: &str| {
        let bytes = std::fs::read(root.join(name)).unwrap();
        bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()
    };
    let gpu = Gpu::new().unwrap();
    for case in manifest["cases"].as_array().unwrap() {
        let extent = std::array::from_fn(|i| case["extent"][i].as_u64().unwrap() as u32);
        let model = std::array::from_fn(|i| case["model"][i].as_f64().unwrap());
        let adaptive = case["adaptive"].as_str().map(floats);
        let prepared = RangePrepared::new(
            extent,
            model,
            floats("intensity.f32"),
            floats("range.f32"),
            case["strength"].as_f64().unwrap() as f32,
            case["radius"].as_u64().unwrap() as u32,
            adaptive,
        )
        .unwrap();
        let input = gpu
            .upload(
                "Native standalone bilateral input",
                &floats(case["input"].as_str().unwrap()),
            )
            .unwrap();
        let output = apply_range(&gpu, &input, &prepared, &mut Default::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        let expected = floats(case["output"].as_str().unwrap());
        let maximum = actual
            .iter()
            .zip(&expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0_f32, f32::max);
        eprintln!(
            "Standalone RGB range {extent:?}, adaptive {}: max {maximum}",
            case["adaptive"].is_string()
        );
        assert!(maximum < 0.0001);
    }
}
