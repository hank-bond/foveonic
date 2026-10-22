//! Checks protection callbacks with owned replicated tails and complete two-pass native crop interiors.

use super::*;

fn iteration(extent: [u32; 2]) -> rgb_prefilter::Prepared {
    rgb_prefilter::Prepared::new(
        extent,
        [1.; 3],
        [1., 1., 1., 0., 0., 0., 0., 0., 0.],
        [1., 0., 0., 0., 1., 0., 0., 0., 1.],
        vec![1.; 14385],
        vec![1.; 2048],
        1.,
        1,
    )
    .unwrap()
}

#[test]
fn scale_contract_requires_matching_even_half_resolution() {
    let a = Prepared::new(
        iteration([16, 16]),
        iteration([8, 8]),
        vec![0.; 14344],
        [1.; 3],
    )
    .unwrap();
    assert_eq!(a.extent(), [16, 16]);
    let b = Prepared::new(
        iteration([16, 16]),
        iteration([8, 8]),
        vec![0.5; 14344],
        [1.; 3],
    )
    .unwrap();
    assert!(a.key() != b.key());
    assert!(Prepared::new(
        iteration([18, 16]),
        iteration([9, 8]),
        vec![0.; 14344],
        [1.; 3]
    )
    .is_err());
    assert!(Prepared::new(
        iteration([16, 16]),
        iteration([7, 8]),
        vec![0.; 14344],
        [1.; 3]
    )
    .is_err());
}

#[test]
#[ignore = "requires a GPU and TWO_SCALE_NOISE_REFERENCE native crop/protection fixtures"]
fn complete_native_protection_and_two_iteration_interiors() {
    let directory =
        std::path::PathBuf::from(std::env::var_os("TWO_SCALE_NOISE_REFERENCE").unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    let floats = |fixture: &serde_json::Value, name: &str| {
        let bytes = std::fs::read(directory.join(fixture[name].as_str().unwrap())).unwrap();
        bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()
    };
    let doubles = |fixture: &serde_json::Value, name: &str| {
        fixture[name]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect::<Vec<_>>()
    };
    let extent = |fixture: &serde_json::Value| {
        [
            fixture["extent"][0].as_u64().unwrap() as u32,
            fixture["extent"][1].as_u64().unwrap() as u32,
        ]
    };
    for fixture in manifest["protection"].as_array().unwrap() {
        let extent = extent(fixture);
        let input = |name| {
            gpu.upload("Protection fixture", &floats(fixture, name))
                .unwrap()
        };
        let gains = doubles(fixture, "gains");
        let params = Params {
            shape: [extent[0], extent[1], 0, 0],
            gains: [gains[0] as f32, gains[1] as f32, gains[2] as f32, 0.],
        };
        let result = protect(
            &gpu,
            &input("current"),
            &input("candidate"),
            &input("original"),
            &params,
            &floats(fixture, "brightness"),
            &mut Default::default(),
        )
        .unwrap();
        let actual = gpu.readback(&result).unwrap();
        compare(
            fixture["name"].as_str().unwrap(),
            bytemuck::cast_slice(&actual),
            &floats(fixture, "output"),
            extent,
            0,
        );
    }
    let model: [f64; 9] = doubles(&manifest, "model").try_into().unwrap();
    let variance_mix: [f64; 9] = doubles(&manifest, "variance_mix").try_into().unwrap();
    for fixture in manifest["cases"].as_array().unwrap() {
        let extent = extent(fixture);
        let gains: [f64; 3] = doubles(fixture, "gains").try_into().unwrap();
        let strengths = doubles(fixture, "strengths");
        let first = rgb_prefilter::Prepared::new(
            extent,
            gains,
            model,
            variance_mix,
            floats(fixture, "intensity0"),
            floats(fixture, "range0"),
            strengths[0] as f32,
            5,
        )
        .unwrap();
        let second = rgb_prefilter::Prepared::new(
            extent.map(|v| v / 2),
            gains,
            model.map(|v| v * 0.25),
            variance_mix,
            floats(fixture, "intensity1"),
            floats(fixture, "range1"),
            strengths[1] as f32,
            15,
        )
        .unwrap();
        let highlight_gains: [f64; 3] = doubles(fixture, "highlight_gains").try_into().unwrap();
        let prepared = Prepared::new(
            first,
            second,
            floats(fixture, "brightness"),
            highlight_gains.map(|v| v as f32),
        )
        .unwrap();
        let input = gpu
            .upload("Complete native two-pass crop", &floats(fixture, "input"))
            .unwrap();
        let result = apply(&gpu, &input, &prepared, &mut Default::default()).unwrap();
        let actual = gpu.readback(&result.rgb).unwrap();
        // Native crop fixtures own zero-filled tail rows. The safe GPU policy
        // replicates valid rows, affecting only the final two protection rows.
        compare(
            fixture["name"].as_str().unwrap(),
            bytemuck::cast_slice(&actual),
            &floats(fixture, "output"),
            extent,
            2,
        );
    }
}

fn compare(case: &str, actual: &[f32], expected: &[f32], extent: [u32; 2], excluded_rows: u32) {
    assert_eq!(actual.len(), expected.len());
    let cutoff = (extent[0] * (extent[1] - excluded_rows) * 3) as usize;
    let mut changed = 0;
    let mut sum = 0_f64;
    let mut maximum = 0_f32;
    let mut boundary_maximum = 0_f32;
    for (index, (&actual, &expected)) in actual.iter().zip(expected).enumerate() {
        assert!(actual.is_finite());
        let difference = (actual - expected).abs();
        if index < cutoff {
            changed += usize::from(actual.to_bits() != expected.to_bits());
            sum += f64::from(difference);
            maximum = maximum.max(difference);
        } else {
            boundary_maximum = boundary_maximum.max(difference);
        }
    }
    eprintln!(
        "{case}: {changed}/{cutoff} differing words, mean {}, max {maximum}, excluded-tail max {boundary_maximum}",
        sum / cutoff as f64
    );
    if excluded_rows == 0 {
        assert_eq!(changed, 0, "native protection words differ");
    }
    assert!(maximum <= 0.002, "{case}: maximum {maximum}");
}

#[test]
#[ignore = "requires GPU and ADAPTIVE_NOISE_REFERENCE independently checked native guidance/blending fixtures"]
fn native_guidance_recursive_means_and_residual_blends() {
    let directory = std::path::PathBuf::from(std::env::var_os("ADAPTIVE_NOISE_REFERENCE").unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    let floats = |case: &serde_json::Value, name: &str| {
        let bytes = std::fs::read(directory.join(case[name].as_str().unwrap())).unwrap();
        bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()
    };
    let extent = |case: &serde_json::Value| {
        [
            case["extent"][0].as_u64().unwrap() as u32,
            case["extent"][1].as_u64().unwrap() as u32,
        ]
    };
    let gpu = Gpu::new().unwrap();
    let compare = |case: &serde_json::Value, actual: &ReadGuard| {
        let bytes = gpu.readback(actual).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        let expected = floats(case, "output");
        let maximum = actual
            .iter()
            .zip(&expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0_f32, f32::max);
        let changed = actual
            .iter()
            .zip(&expected)
            .filter(|(a, b)| a.to_bits() != b.to_bits())
            .count();
        eprintln!(
            "{}: {changed}/{} changed, maximum {maximum}",
            case["name"],
            actual.len()
        );
        assert_eq!(actual.len(), expected.len());
        assert_eq!(actual, expected, "{} maximum {maximum}", case["name"]);
    };
    for case in manifest["guidance"].as_array().unwrap() {
        let input = gpu
            .upload("Native guidance fixture", &floats(case, "input"))
            .unwrap();
        let model: Vec<_> = case["model"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                let v = v.as_f64().unwrap();
                [v as f32, (v - f64::from(v as f32)) as f32]
            })
            .collect();
        let output = guided_mean(
            &gpu,
            &input,
            &input,
            extent(case),
            &model,
            &mut Default::default(),
        )
        .unwrap();
        compare(case, &output);
    }
    for case in manifest["blend"].as_array().unwrap() {
        let current = gpu
            .upload("Native blend current", &floats(case, "current"))
            .unwrap();
        let candidate = gpu
            .upload("Native blend candidate", &floats(case, "candidate"))
            .unwrap();
        let output = residual_blend(
            &gpu,
            &current,
            &candidate,
            extent(case),
            &floats(case, "lookup"),
            case["minimum"].as_f64().unwrap() as f32,
            &mut Default::default(),
        )
        .unwrap();
        compare(case, &output);
    }
    let doubles = |value: &serde_json::Value| {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect::<Vec<_>>()
    };
    let model: [f64; 9] = doubles(&manifest["model"]).try_into().unwrap();
    let variance: [f64; 9] = doubles(&manifest["variance_mix"]).try_into().unwrap();
    for case in manifest["complete"].as_array().unwrap() {
        let extent = extent(case);
        let gains: [f64; 3] = doubles(&case["gains"]).try_into().unwrap();
        let iteration = |index: usize| {
            rgb_prefilter::Prepared::new(
                if index == 0 {
                    extent
                } else {
                    extent.map(|v| v / 2)
                },
                gains,
                model.map(|v| if index == 0 { v } else { v * 0.25 }),
                variance,
                floats(case, &format!("intensity{index}")),
                floats(case, &format!("range{index}")),
                case["strengths"][index].as_f64().unwrap() as f32,
                if index == 0 { 5 } else { 15 },
            )
            .unwrap()
        };
        let lookup = floats(case, "lookup");
        let mut prepared = Prepared::new(
            iteration(0),
            iteration(1),
            lookup[..14344].to_vec(),
            [1.; 3],
        )
        .unwrap();
        if case["iso"].as_u64().unwrap() >= 400 {
            let restored = std::array::from_fn(|i| match i / 3 {
                0 => (model[i] * (gains[i % 3] * gains[i % 3])) / (gains[i % 3] * gains[i % 3]),
                1 => (model[i] * gains[i % 3]) / gains[i % 3],
                _ => model[i],
            });
            prepared = prepared
                .with_adaptive(
                    [restored, restored.map(|v| v * 0.25)],
                    lookup[14344..].to_vec(),
                    case["minimum"].as_f64().unwrap() as f32,
                )
                .unwrap();
        }
        let input = gpu
            .upload("Native complete adaptive fixture", &floats(case, "input"))
            .unwrap();
        let output = apply(&gpu, &input, &prepared, &mut Default::default()).unwrap();
        // The established low-ISO weighting contract uses an owned replicated
        // physical tail. The native fixture owns zero tails. Compare interiors.
        let bytes = gpu.readback(&output.rgb).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        let expected = floats(case, "output");
        let end = if case["iso"].as_u64().unwrap() < 400 {
            (extent[1] - 2) * extent[0] * 3
        } else {
            extent[0] * extent[1] * 3
        } as usize;
        let maximum = actual[..end]
            .iter()
            .zip(&expected[..end])
            .map(|(a, b)| (a - b).abs())
            .fold(0_f32, f32::max);
        eprintln!("{} complete maximum {maximum}", case["name"]);
        assert_eq!(
            &actual[..end],
            &expected[..end],
            "{} complete maximum {maximum}",
            case["name"]
        );
    }
}

#[test]
#[ignore = "requires a GPU"]
fn residual_half_integer_lut_decisions_match_native_power_rounding() {
    // Three original-image pixels land near residual-LUT half-integers. Keep
    // exact source bits so input decimal rounding cannot move the decisions.
    let current = [
        0x43043c0d, 0x43278a24, 0x4302c8c8, 0x43a31805, 0x43c0c400, 0x4382906a, 0x435c29a4,
        0x43878102, 0x435c3d2e,
    ]
    .map(f32::from_bits);
    let candidate = [
        0x4366523c, 0x438c0883, 0x435d034e, 0x4385aac6, 0x4397b4c7, 0x43517246, 0x438b82d4,
        0x43aeaa95, 0x4388dab2,
    ]
    .map(f32::from_bits);
    // Independent native BlendImages, constant brightness and linear damping.
    let expected = [
        0x435ab837, 0x438560e1, 0x43525739, 0x4387d310, 0x439ab765, 0x43553c42, 0x4388dd48,
        0x43ab24f7, 0x43867332,
    ]
    .map(f32::from_bits);
    let mut lookup = vec![1.; 14344];
    lookup.extend((0..1024).map(|i| (-f64::from(i) / 1023.) as f32));
    let gpu = Gpu::new().unwrap();
    let current = gpu.upload("Residual decision source", &current).unwrap();
    let candidate = gpu
        .upload("Residual decision candidate", &candidate)
        .unwrap();
    let output = residual_blend(
        &gpu,
        &current,
        &candidate,
        [3, 1],
        &lookup,
        0.,
        &mut Default::default(),
    )
    .unwrap();
    let bytes = gpu.readback(&output).unwrap();
    assert_eq!(bytemuck::cast_slice::<u8, f32>(&bytes), expected);
}
