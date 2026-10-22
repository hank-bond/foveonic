//! Checks ordered chroma repair parameters and independently generated pixel fixtures.

use super::*;
#[test]
fn rejects_singular_projection_and_invalid_geometry() {
    assert!(Prepared::new([12, 12], vec![], 0, [1.; 9], [0.; 9], false, false).is_err());
    assert!(Prepared::new(
        [0, 12],
        vec![],
        0,
        [1.; 9],
        [1., 0., 0., 0., 1., 0., 0., 0., 1.],
        false,
        false
    )
    .is_err());
}

#[test]
#[ignore = "requires GPU and CHROMA_REFERENCE independent fixtures"]
fn single_and_general_match_independent_fixtures() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("CHROMA_REFERENCE").expect("set CHROMA_REFERENCE"),
    );
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("cases.json")).unwrap()).unwrap();
    let read = |name: &str| -> Vec<f32> {
        let bytes = std::fs::read(directory.join(name)).unwrap();
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| f32::from_le_bytes(*v))
            .collect()
    };
    let gpu = Gpu::new().unwrap();
    let input = gpu
        .upload("Chroma fixture input", &read("input.f32"))
        .unwrap();
    let mask = gpu
        .upload("Chroma fixture mask", &vec![0_u32; 144])
        .unwrap();
    let mut maximum = 0_f32;
    let mut changed = 0;
    let mut total = 0;
    for case in cases.as_array().unwrap() {
        let array = |key: &str| {
            case[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect::<Vec<_>>()
        };
        let noise: Vec<_> = array("noise").into_iter().map(|v| v as f32).collect();
        let matrix = array("matrix");
        let flags = case["flags"].as_u64().unwrap() as u32;
        let prepared = Prepared::new(
            [12, 12],
            vec![[6, 6, flags], [6, 7, flags]],
            case["threshold"].as_u64().unwrap() as u32,
            noise.try_into().unwrap(),
            matrix.try_into().unwrap(),
            case["low"].as_bool().unwrap(),
            case["threshold_single"].as_bool().unwrap(),
        )
        .unwrap();
        let stage = if case["stage"] == "single" {
            Stage::Single
        } else {
            Stage::General
        };
        let output = apply(
            &gpu,
            &input,
            &mask,
            &prepared,
            stage,
            &mut Default::default(),
        )
        .unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        let expected = read(case["file"].as_str().unwrap());
        for (i, (&a, &b)) in actual.iter().zip(&expected).enumerate() {
            assert!(a.is_finite());
            let error = (a - b).abs();
            maximum = maximum.max(error);
            changed += usize::from(a.to_bits() != b.to_bits());
            total += 1;
            assert!(error <= 0.001, "{} sample{i}: {a} vs {b}", case["file"]);
        }
    }
    eprintln!("Chroma fixtures changed={changed}/{total} max={maximum}");
}

#[test]
#[ignore = "requires GPU and CHROMA_NATIVE_BOUNDARY"]
fn full_ordered_passes_match_actual_native_boundary() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("CHROMA_NATIVE_BOUNDARY").expect("set CHROMA_NATIVE_BOUNDARY"),
    );
    let controls: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("chroma-boundaries.json")).unwrap())
            .unwrap();
    let read = |name: &str| -> Vec<f32> {
        let bytes = std::fs::read(directory.join(name)).unwrap();
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| f32::from_le_bytes(*v))
            .collect()
    };
    let gpu = Gpu::new().unwrap();
    for (name, stage) in [("single", Stage::Single), ("general", Stage::General)] {
        let c = &controls[name];
        let w = c["input"]["rgb"]["extent"][0].as_u64().unwrap() as u32;
        let h = c["input"]["rgb"]["extent"][1].as_u64().unwrap() as u32;
        let raw_records =
            std::fs::read(directory.join(format!("chroma-{name}-records.bin"))).unwrap();
        let records = raw_records
            .as_chunks::<6>()
            .0
            .iter()
            .map(|v| {
                std::array::from_fn(|i| u32::from(u16::from_le_bytes([v[i * 2], v[i * 2 + 1]])))
            })
            .collect();
        let noise: Vec<_> = c["noise"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect();
        let matrix = controls["single"]["matrix"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect::<Vec<_>>();
        let prepared = Prepared::new(
            [w, h],
            records,
            c["registers"]["x3"].as_u64().unwrap() as u32,
            noise.try_into().unwrap(),
            matrix.try_into().unwrap(),
            false,
            false,
        )
        .unwrap();
        let input = gpu
            .upload(
                "Actual chroma input",
                &read(&format!("chroma-{name}-input-rgb.f32")),
            )
            .unwrap();
        let mask: Vec<u32> = std::fs::read(directory.join(format!("chroma-{name}-input-mask.u8")))
            .unwrap()
            .into_iter()
            .map(u32::from)
            .collect();
        let mask = gpu.upload("Actual chroma mask", &mask).unwrap();
        let output = apply(
            &gpu,
            &input,
            &mask,
            &prepared,
            stage,
            &mut Default::default(),
        )
        .unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        let expected = read(&format!("chroma-{name}-output-rgb.f32"));
        assert_eq!(actual.len(), expected.len());
        let mut maximum = 0_f32;
        let mut sum = 0_f64;
        let mut changed = 0;
        for (i, (&a, &b)) in actual.iter().zip(&expected).enumerate() {
            assert!(a.is_finite());
            let error = (a - b).abs();
            maximum = maximum.max(error);
            sum += f64::from(error);
            changed += usize::from(a.to_bits() != b.to_bits());
            assert!(
                error <= 0.008_f32.max(b.abs() * f32::EPSILON * 8.),
                "{name}/{i} {a} vs {b}"
            );
        }
        eprintln!(
            "Chroma native/{name} changed={changed}/{} mean={} max={maximum}",
            actual.len(),
            sum / actual.len() as f64
        );
    }
}

#[test]
#[ignore = "requires GPU"]
fn directional_contrast_reduces_channels_after_neighbor_terms() {
    // Actual source patch whose mathematical directional contrasts tie. ARM
    // rounds each channel's four terms before reducing RGB; a twelve-term
    // accumulation chooses the other direction and changes blue by10.61codes.
    let neighbors = [
        [191., 289., 252.00056],
        [248., 366., 320.80136],
        [399., 581., 530.78467],
        [193., 307., 275.2149],
        [210., 330., 288.79483],
        [300., 397., 329.46985],
        [222., 359., 299.48468],
        [202., 321., 272.98138],
    ];
    let offsets = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ];
    let mut values = vec![100_f32; 12 * 12 * 3];
    for (v, (dx, dy)) in neighbors.into_iter().zip(offsets) {
        let i = (((6 + dy) * 12 + 6 + dx) * 3) as usize;
        values[i..i + 3].copy_from_slice(&v);
    }
    values[(6 * 12 + 6) * 3..(6 * 12 + 6) * 3 + 3].copy_from_slice(&[191., 289., 220.05423]);
    let matrix = [
        4.065333233921296,
        -3.14764404296875,
        1.3201287183648875,
        -1.880045537623623,
        3.3568267822265625,
        -1.1666118982450264,
        1.194234720425999,
        -4.088043212890625,
        5.287586888080553,
    ];
    let prepared = Prepared::new(
        [12, 12],
        vec![[6, 6, 5119]],
        0,
        [
            24.362,
            0.04835,
            -0.000000125,
            15.16425,
            0.0275,
            -0.00000005,
            1.3026875,
            0.0271375,
            0.00000005,
        ],
        matrix,
        false,
        false,
    )
    .unwrap();
    let gpu = Gpu::new().unwrap();
    let input = gpu.upload("Directional tie patch", &values).unwrap();
    let mask = gpu
        .upload("Directional tie mask", &vec![0_u32; 144])
        .unwrap();
    let output = apply(
        &gpu,
        &input,
        &mask,
        &prepared,
        Stage::Single,
        &mut Default::default(),
    )
    .unwrap();
    let bytes = gpu.readback(&output).unwrap();
    let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
    assert!((actual[(6 * 12 + 6) * 3 + 2] - 251.39392).abs() < 0.0001);
}

#[test]
fn batches_preserve_neighbor_and_duplicate_dependencies() {
    let records = vec![
        [6, 6, 0, 0],
        [20, 20, 0, 0],
        [6, 7, 0, 0],
        [6, 6, 0, 0],
        [20, 21, 0, 0],
    ];
    let levels = schedule([40, 40], records.clone());
    assert_eq!(levels, schedule([40, 1_000_000], records.clone()));
    assert_eq!(
        levels,
        vec![
            vec![records[0], records[1]],
            vec![records[2], records[4]],
            vec![records[3]]
        ]
    );
}

#[test]
#[ignore = "requires GPU"]
fn parallel_batches_match_serial_record_order_exactly() {
    let extent = [40, 40];
    let gpu = Gpu::new().unwrap();
    let values: Vec<f32> = (0..40 * 40 * 3)
        .map(|i| ((i * 73 + i * i * 7) % 4093) as f32 * 0.125)
        .collect();
    let input = gpu
        .upload("Ordered chroma dependency input", &values)
        .unwrap();
    let mask = gpu
        .upload("Ordered chroma dependency mask", &vec![0u32; 1600])
        .unwrap();
    for stage in [Stage::Single, Stage::General] {
        let bits = if matches!(stage, Stage::Single) {
            255
        } else {
            127
        };
        let records: Vec<_> = [[6, 6], [20, 20], [6, 7], [6, 6], [20, 21], [7, 8], [31, 12]]
            .map(|[y, x]| [y, x, bits | (7 << 12)])
            .into();
        let parallel = Prepared::new(
            extent,
            records.clone(),
            0,
            [0.; 9],
            [1., 0., 0., 0., 1., 0., 0., 0., 1.],
            true,
            false,
        )
        .unwrap();
        let mut serial = parallel.clone();
        serial.records = records.into_iter().map(|r| [r[0], r[1], r[2], 0]).collect();
        let stage_index = if matches!(stage, Stage::Single) { 0 } else { 1 };
        serial.batches = [vec![], vec![]];
        serial.batches[stage_index] = (0..serial.records.len() as u32).map(|i| [i, 1]).collect();
        let run = |prepared: &Prepared| {
            let output = apply(
                &gpu,
                &input,
                &mask,
                prepared,
                stage,
                &mut Default::default(),
            )
            .unwrap();
            gpu.readback(&output).unwrap()
        };
        assert_eq!(run(&parallel), run(&serial), "{stage:?}");
    }
}

#[test]
#[ignore = "requires CHROMA_NATIVE_BOUNDARY; measures metadata preparation without GPU"]
fn benchmark_ordered_chroma_preparation() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("CHROMA_NATIVE_BOUNDARY").expect("set CHROMA_NATIVE_BOUNDARY"),
    );
    let raw = std::fs::read(directory.join("chroma-single-records.bin")).unwrap();
    let records: Vec<[u32; 3]> = raw
        .as_chunks::<6>()
        .0
        .iter()
        .map(|v| std::array::from_fn(|i| u32::from(u16::from_le_bytes([v[i * 2], v[i * 2 + 1]]))))
        .collect();
    let controls: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("chroma-boundaries.json")).unwrap())
            .unwrap();
    let extent = std::array::from_fn(|i| {
        controls["single"]["input"]["rgb"]["extent"][i]
            .as_u64()
            .unwrap() as u32
    });
    let start = std::time::Instant::now();
    for _ in 0..10 {
        std::hint::black_box(
            Prepared::new(
                extent,
                records.clone(),
                0,
                [1.; 9],
                [1., 0., 0., 0., 1., 0., 0., 0., 1.],
                false,
                false,
            )
            .unwrap(),
        );
    }
    eprintln!(
        "chroma preparation {:.3} ms per call",
        start.elapsed().as_secs_f64() * 100.
    );
}

#[test]
fn dense_schedule_matches_original_sparse_dependency_levels() {
    let records: Vec<_> = (0..20_000u32)
        .map(|i| {
            let y = 2 + ((i * 73 + i * i) % 508);
            let x = 2 + ((i * 31 + i * i * 3) % 508);
            [y, x, i, 0]
        })
        .collect();
    let mut prior = std::collections::HashMap::new();
    let mut expected: Vec<Vec<[u32; 4]>> = Vec::new();
    for &record in &records {
        let [y, x, _, _] = record;
        let mut level = 0;
        for py in y - 1..=y + 1 {
            for px in x - 1..=x + 1 {
                if let Some(&previous) = prior.get(&(py, px)) {
                    level = level.max(previous + 1);
                }
            }
        }
        prior.insert((y, x), level);
        if expected.len() <= level {
            expected.resize_with(level + 1, Vec::new);
        }
        expected[level].push(record);
    }
    assert_eq!(schedule([512, 512], records.clone()), expected);
    assert_eq!(schedule([512, 40_000], records), expected);
}
