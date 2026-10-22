//! Checks ordering-preserving metadata batches and native blue-record repair fixtures.
use super::*;
#[test]
fn dependencies_separate_overlapping_searches_and_keep_distant_jobs_parallel() {
    let records = vec![
        [20, 20, 0x03ff],
        [20, 21, 0x03ff],
        [50, 50, 0x03ff],
        [21, 20, 0x03ff],
    ];
    let p = Prepared::new([80, 80], records, 0, 1., Stage::Directional).unwrap();
    assert_eq!(p.batches, [[0, 2], [2, 1], [3, 1]]);
    assert_eq!(
        p.jobs.iter().map(|r| [r[0], r[1]]).collect::<Vec<_>>(),
        [[20, 20], [50, 50], [20, 21], [21, 20]]
    );
}
#[test]
#[ignore = "requires a GPU and ORDERED_BLUE_REFERENCE"]
fn record_stages_match_independent_native_fixtures() {
    let dir = std::path::PathBuf::from(std::env::var("ORDERED_BLUE_REFERENCE").unwrap());
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("cases.json")).unwrap()).unwrap();
    let gpu = Gpu::new().unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let read = |suffix: &str| std::fs::read(dir.join(format!("{name}-{suffix}.bin"))).unwrap();
        let extent = [
            case["extent"][0].as_u64().unwrap() as u32,
            case["extent"][1].as_u64().unwrap() as u32,
        ];
        let records = case["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| std::array::from_fn(|k| r[k].as_u64().unwrap() as u32))
            .collect();
        let stage = match case["stage"].as_str().unwrap() {
            "single" => Stage::Directional,
            "offset" => Stage::DarkOffsets,
            _ => Stage::General,
        };
        let p = Prepared::new(
            extent,
            records,
            2,
            case["gain"].as_f64().unwrap() as f32,
            stage,
        )
        .unwrap();
        let input = read("input");
        let mask = read("mask-input");
        let input = gpu
            .upload(
                "Blue records fixture",
                bytemuck::cast_slice::<u8, f32>(&input),
            )
            .unwrap();
        let mask: Vec<u32> = mask.into_iter().map(u32::from).collect();
        let mask = gpu.upload("Blue mask fixture", &mask).unwrap();
        let o = apply(&gpu, &input, Some(&mask), &p, &mut Profile::default()).unwrap();
        assert_eq!(
            gpu.readback(&o.blue).unwrap(),
            read("output"),
            "{name} blue"
        );
        let mask = gpu.readback(&o.mask).unwrap();
        let mask: Vec<u8> = bytemuck::cast_slice::<u8, u32>(&mask)
            .iter()
            .map(|&v| v as u8)
            .collect();
        assert_eq!(mask, read("mask-output"), "{name} mask");
    }
}

#[test]
#[ignore = "requires GPU, QUATTRO_X3F and LUMA_NATIVE_BOUNDARIES"]
fn original_record_stages_match_native_boundaries() {
    let dir = std::path::PathBuf::from(std::env::var("LUMA_NATIVE_BOUNDARIES").unwrap());
    let data: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("luma-boundaries.json")).unwrap()).unwrap();
    let source = crate::source::Source::open(std::env::var_os("QUATTRO_X3F").unwrap()).unwrap();
    let gain = source
        .metadata
        .fixed::<3>("SensorAdjustmentGainFact")
        .unwrap()[2] as f32;
    drop(source);
    let table = std::fs::read(dir.join("luma-directional-0-records.bin")).unwrap();
    let records: Vec<[u32; 3]> = table
        .as_chunks::<6>()
        .0
        .iter()
        .map(|r| std::array::from_fn(|k| u32::from(u16::from_le_bytes([r[k * 2], r[k * 2 + 1]]))))
        .collect();
    let gpu = Gpu::new().unwrap();
    for (label, stage) in [
        ("directional", Stage::Directional),
        ("dark-offset", Stage::DarkOffsets),
        ("general", Stage::General),
    ] {
        let case = &data[label][0];
        let extent = std::array::from_fn(|k| case["input"]["extent"][k].as_u64().unwrap() as u32);
        let input = std::fs::read(dir.join(case["input"]["file"].as_str().unwrap())).unwrap();
        let input = gpu
            .upload(
                "Original blue repair",
                bytemuck::cast_slice::<u8, f32>(&input),
            )
            .unwrap();
        let mask = case.get("input_mask").map(|m| {
            let bytes = std::fs::read(dir.join(m["file"].as_str().unwrap())).unwrap();
            let words: Vec<u32> = bytes.into_iter().map(u32::from).collect();
            gpu.upload("Original blue mask", &words).unwrap()
        });
        let prepared = Prepared::new(extent, records.clone(), 0, gain, stage).unwrap();
        let mut profile = Profile::new(crate::profile::Mode::Gpu);
        let output = apply(&gpu, &input, mask.as_ref(), &prepared, &mut profile).unwrap();
        let actual = gpu.readback(&output.blue).unwrap();
        let expected = std::fs::read(dir.join(case["output"]["file"].as_str().unwrap())).unwrap();
        let mut count = 0;
        let mut maximum = 0_f32;
        let mut examples = Vec::new();
        for (i, (a, b)) in actual
            .as_chunks::<4>()
            .0
            .iter()
            .zip(expected.as_chunks::<4>().0)
            .enumerate()
        {
            if a != b {
                count += 1;
                let a = f32::from_le_bytes(*a);
                let b = f32::from_le_bytes(*b);
                maximum = maximum.max((a - b).abs());
                if examples.len() < 8 {
                    examples.push((i, a, b));
                }
            }
        }
        eprintln!(
            "{label}: {count} differences,max{maximum},{examples:?}, {}batches, {}msGPU",
            prepared.batches.len(),
            profile.gpu.iter().map(|t| t.milliseconds).sum::<f64>()
        );
        assert_eq!(count, 0, "{label}");
        if let Some(m) = case.get("output_mask") {
            let expected = std::fs::read(dir.join(m["file"].as_str().unwrap())).unwrap();
            let actual = gpu.readback(&output.mask).unwrap();
            let actual: Vec<u8> = bytemuck::cast_slice::<u8, u32>(&actual)
                .iter()
                .map(|&v| v as u8)
                .collect();
            assert_eq!(actual, expected, "{label} mask");
        }
    }
}
