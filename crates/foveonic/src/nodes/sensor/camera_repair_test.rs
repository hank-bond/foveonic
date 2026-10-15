//! Regresses complete camera list order, raw-signal detection, packed cluster matching, and the reported dark flower defect.

use super::*;
use crate::gpu::Binding::{Read, Write};
use crate::source::{Matrix, Metadata};
fn matrix(dimensions: Vec<usize>, values: Vec<f64>, encoding: MatrixEncoding) -> Matrix {
    Matrix {
        dimensions,
        values,
        encoding,
        representation: "fixture",
    }
}
fn metadata(
    w: usize,
    h: usize,
    main: Vec<f64>,
    jpeg: Vec<f64>,
    model: Vec<f64>,
    words: Vec<f64>,
) -> Metadata {
    Metadata {
        width: w,
        height: h,
        rotation: 0,
        properties: Default::default(),
        lists: Default::default(),
        matrices: [
            (
                "BadPixelsF20".into(),
                matrix(vec![3, main.len() / 3], main, MatrixEncoding::Uint16),
            ),
            (
                "Jpeg_BadClusters".into(),
                matrix(vec![3, jpeg.len() / 3], jpeg, MatrixEncoding::Uint32),
            ),
            (
                "ColorNoiseModel".into(),
                matrix(vec![3, 3], model, MatrixEncoding::Float32),
            ),
            (
                "BadClustersF20".into(),
                matrix(vec![words.len(), 1], words, MatrixEncoding::Uint32),
            ),
        ]
        .into(),
    }
}
fn numbers(v: &serde_json::Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect()
}
fn rows(v: &serde_json::Value) -> Vec<f64> {
    v.as_array().unwrap().iter().flat_map(numbers).collect()
}
fn fixture() -> (Metadata, serde_json::Value) {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/data/complete-camera-reference.json"
    ))
    .unwrap();
    (
        metadata(
            f["width"].as_u64().unwrap() as usize,
            f["height"].as_u64().unwrap() as usize,
            rows(&f["main"]),
            rows(&f["jpeg"]),
            numbers(&f["model"]),
            numbers(&f["cluster_words"]),
        ),
        f,
    )
}
fn assert_phase_dependencies(p: &Prepared) {
    let w = p.params.source[0];
    let h = p.params.source[1];
    let base = p.params.selection[2] as usize;
    let mut assigned = vec![None; base];
    let mut end = 0;
    for (level, phase) in p.phases.iter().enumerate() {
        assert_eq!(phase[0] as usize, end);
        end += phase[1] as usize;
        for &index in &p.repair_order[phase[0] as usize..end] {
            assert!(assigned[index as usize].replace(level).is_none());
        }
    }
    assert_eq!(end, p.repair_order.len());
    let mut previous = std::collections::HashMap::<u32, usize>::new();
    for (index, r) in p.records[..base].iter().enumerate() {
        let x = r[0] % w;
        let y = r[0] / w;
        let selected = ((r[1] >> 8) & 15) >= p.params.selection[0]
            && r[1] & 255 != 0
            && x >= 2
            && y >= 2
            && x < w - 1
            && y < h - 1;
        if !selected {
            assert!(assigned[index].is_none());
            continue;
        }
        let level = assigned[index].expect("eligible record missing from schedule");
        for dy in -1_i32..=1 {
            for dx in -1_i32..=1 {
                let q = (y as i32 + dy) as u32 * w + (x as i32 + dx) as u32;
                if let Some(old) = previous.get(&q) {
                    assert!(*old < level, "neighbor/duplicate/anti-dependency reordered");
                }
            }
        }
        previous.insert(r[0], level);
    }
}
#[test]
fn phases_preserve_neighbors_duplicates_anti_dependencies_and_skip_invalid_records() {
    let mut p = Prepared {
        params: Params {
            source: [10, 10, 9, 0],
            selection: [2, 999, 9, 0],
            ..Default::default()
        },
        records: vec![
            [33, 0x43ff],
            [77, 0x43ff],
            [34, 0x43ff],
            [33, 0x43ff],
            [43, 0x43ff],
            [31, 0x43ff],
            [76, 0x43ff],
            [99, 0x43ff],
            [55, 0x4300],
        ],
        ..Default::default()
    };
    prepare_phases(&mut p);
    assert_eq!(p.repair_order, [0, 1, 2, 6, 3, 4]);
    assert_eq!(p.phases, [[0, 2], [2, 2], [4, 1], [5, 1]]);
    assert_phase_dependencies(&p);
}
#[test]
#[ignore = "requires a working GPU adapter"]
fn capped_integer_prefixes_match_ordered_counts_across_blocks_and_partial_tails() {
    let gpu = Gpu::new().unwrap();
    for n in [1_u32, 255, 256, 257, 777] {
        for capacity in [0_u32, 7, 999] {
            let p = Params {
                source: [64, 64, n, 0],
                selection: [0, capacity, n, 0],
                ..Default::default()
            };
            let uniform = gpu.uniform(&p);
            let masks: Vec<u32> = (0..n)
                .map(|i| [0, 255, 0, 3, 1, 128, 15][i as usize % 7])
                .collect();
            let input = gpu.upload("Prefix integer mask fixture", &masks).unwrap();
            let mut local = gpu
                .storage("Local prefix fixture", u64::from(n) * 4)
                .unwrap();
            let mut blocks = gpu
                .storage("Block prefix fixture", u64::from(n.div_ceil(256)) * 4)
                .unwrap();
            let mut records = gpu
                .storage(
                    "Final capped counter fixture",
                    u64::from(n + capacity + 1) * 8,
                )
                .unwrap();
            let mut trace = Default::default();
            gpu.dispatch(
                Kernel::CameraPrefix,
                &mut [
                    (2, Read(&uniform)),
                    (4, Read(&input)),
                    (5, Write(local.write())),
                    (8, Write(blocks.write())),
                ],
                [n.div_ceil(256), 1],
                &mut trace,
            );
            gpu.dispatch(
                Kernel::CameraBlocks,
                &mut [
                    (1, Write(records.write())),
                    (2, Read(&uniform)),
                    (8, Write(blocks.write())),
                ],
                [1, 1],
                &mut trace,
            );
            let local_bytes = gpu.readback(&local).unwrap();
            let local = bytemuck::cast_slice::<u8, u32>(&local_bytes);
            let block_bytes = gpu.readback(&blocks).unwrap();
            let offsets = bytemuck::cast_slice::<u8, u32>(&block_bytes);
            let bytes = gpu.readback(&records).unwrap();
            let records = bytemuck::cast_slice::<u8, [u32; 2]>(&bytes);
            let mut count = 0;
            for (i, mask) in masks.iter().enumerate() {
                assert_eq!((local[i] + offsets[i / 256]).min(capacity), count);
                count = (count + mask.count_ones()).min(capacity);
            }
            assert_eq!(records[(n + capacity) as usize], [count, 0]);
        }
    }
}

#[test]
#[ignore = "requires FOVEONIC_FIXTURE_DIR"]
fn original_metadata_repair_dependency_phase_counts() {
    let root = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap());
    for name in [
        "blue-parking-sign",
        "glossy-leaves-and-buds",
        "rocks-and-tree-shadows",
        "sunlit-red-flower",
        "turquoise-still-life",
        "yellow-flowers-and-stroller",
    ] {
        let source = crate::source::Source::open(root.join(format!("{name}.x3f"))).unwrap();
        for threshold in [
            Threshold::Camera,
            Threshold::Manual(0),
            Threshold::Manual(4),
        ] {
            let p = prepare(&source.metadata, threshold).unwrap();
            assert_phase_dependencies(&p);
            println!(
                "{name}: selected {} phase_count {}",
                p.repair_order.len(),
                p.phases.len()
            );
        }
    }
}

#[test]
fn preparation_preserves_full_low_badness_lists_and_cluster_quality_order() {
    let (m, _) = fixture();
    let p = prepare(&m, Threshold::Manual(2)).unwrap();
    assert_eq!(p.params.source, [64, 64, 6, 5]);
    assert_eq!(p.params.selection, [2, 999, 11, 0]);
    assert_eq!(p.records[1], p.records[2]);
    assert_eq!(p.records[1][1], 0x40df);
    assert_eq!(p.clusters.len(), 2);
    assert_eq!(p.clusters[0].indices[1], 6);
    for cluster in &p.clusters {
        let start = cluster.indices[0] as usize;
        let end = start + cluster.indices[1] as usize;
        assert!(p.members[start..end].windows(2).all(|m| m[0][2] >= m[1][2]));
    }
    let mut no_jpeg = m.clone();
    no_jpeg.matrices.remove("Jpeg_BadClusters");
    assert_eq!(
        prepare(&no_jpeg, Threshold::Manual(2))
            .unwrap()
            .params
            .source[3],
        0
    );
    let mut malformed = m.clone();
    malformed
        .matrices
        .get_mut("BadClustersF20")
        .unwrap()
        .values
        .pop();
    assert!(prepare(&malformed, Threshold::Manual(2)).is_err());
    let mut malformed = m.clone();
    malformed.matrices.get_mut("BadClustersF20").unwrap().values[4] = 0.;
    assert!(prepare(&malformed, Threshold::Manual(2)).is_err());
    let mut malformed = m;
    malformed
        .matrices
        .get_mut("ColorNoiseModel")
        .unwrap()
        .values[0] = f64::INFINITY;
    assert!(prepare(&malformed, Threshold::Manual(2)).is_err());
}
#[test]
#[ignore = "requires a working GPU adapter"]
fn cooperative_cluster_search_preserves_first_tie_empty_search_and_uniform_padding() {
    let gpu = Gpu::new().unwrap();
    let mut pixels = [10_f32, 20., 30.].repeat(96);
    pixels[13 * 3..13 * 3 + 3].copy_from_slice(&[111., 222., 333.]);
    pixels[14 * 3..14 * 3 + 3].copy_from_slice(&[999., 888., 777.]);
    let clusters = gpu
        .upload(
            "Tie cluster",
            &[Cluster {
                rect: [0, 0, 12, 8],
                indices: [0, 1, 0, 0],
            }],
        )
        .unwrap();
    let members = gpu.upload("Tie member", &[[69_u32, 69, 0, 0]]).unwrap();
    let params = gpu.uniform(&[12_u32, 8, 1, 2]);
    for (known, expected) in [(256_u32, [111_f32, 222., 333.]), (0, [0., 0., 0.])] {
        let mut output = gpu.upload("Tie pixels", &pixels).unwrap();
        let mut masks = gpu.upload("Tie masks", &[known; 96]).unwrap();
        gpu.dispatch(
            Kernel::ClusterRepair,
            &mut [
                (0, Write(output.write())),
                (1, Read(&clusters)),
                (2, Read(&members)),
                (3, Write(masks.write())),
                (4, Read(&params)),
            ],
            [1, 1],
            &mut Default::default(),
        );
        let bytes = gpu.readback(&output).unwrap();
        let values = bytemuck::cast_slice::<u8, f32>(&bytes);
        assert_eq!(&values[69 * 3..69 * 3 + 3], &expected);
        assert_eq!(&values[13 * 3..13 * 3 + 3], &pixels[13 * 3..13 * 3 + 3]);
        let bytes = gpu.readback(&masks).unwrap();
        assert_eq!(bytemuck::cast_slice::<u8, u32>(&bytes)[69], known | 7);
    }
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn gpu_complete_camera_matches_native_raw_detection_lists_and_clusters() {
    let (m, f) = fixture();
    let p = prepare(&m, Threshold::Manual(2)).unwrap();
    let gpu = Gpu::new().unwrap();
    let raw: Vec<_> = numbers(&f["original"])
        .into_iter()
        .map(|v| v as f32)
        .collect();
    let original = gpu.upload("Immutable original fixture", &raw).unwrap();
    for gain in [0., 1., 1.3] {
        let samples: Vec<_> = numbers(&f["input"])
            .into_iter()
            .map(|v| v as f32 * gain)
            .collect();
        let input = gpu
            .upload("Immutable calibrated fixture", &samples)
            .unwrap();
        let result = apply(&gpu, &original, &input, &p, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(&result).unwrap();
        for (i, (&actual, expected)) in bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .zip(numbers(&f["output"]))
            .enumerate()
        {
            assert!(
                (actual - expected as f32 * gain).abs() < 0.004,
                "gain {gain} sample {i}: {actual} != {}",
                expected as f32 * gain
            );
        }
        assert_eq!(
            gpu.readback(&input).unwrap(),
            bytemuck::cast_slice::<f32, u8>(&samples)
        );
        assert_eq!(
            gpu.readback(&original).unwrap(),
            bytemuck::cast_slice::<f32, u8>(&raw)
        );
    }
    // Independently inspect runtime main overrides and the ordered dynamic list.
    let mut output = gpu
        .upload(
            "Detector output backing",
            &numbers(&f["input"])
                .into_iter()
                .map(|v| v as f32)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    let mut records = gpu.upload("Detector records", &p.records).unwrap();
    let uniform = gpu.uniform(&p.params);
    select_bad_reds(
        &gpu,
        &original,
        &mut records,
        &uniform,
        p.params,
        &mut Default::default(),
    )
    .unwrap();
    correct_lists(
        &gpu,
        &records,
        &mut output,
        &uniform,
        &p,
        &mut Default::default(),
    )
    .unwrap();
    let bytes = gpu.readback(&records).unwrap();
    let r = bytemuck::cast_slice::<u8, [u32; 2]>(&bytes);
    for (i, v) in numbers(&f["overrides"]).iter().enumerate() {
        assert_eq!(r[i][1] & 255, *v as u32);
    }
    let base = p.params.selection[2] as usize;
    assert_eq!(
        r[base + 999][0] as usize,
        f["detected"].as_array().unwrap().len()
    );
    for (i, expected) in f["detected"].as_array().unwrap().iter().enumerate() {
        let e = numbers(expected);
        assert_eq!(r[base + i], [(e[0] as u32) * 64 + e[1] as u32, e[2] as u32]);
    }
}
#[test]
#[ignore = "requires a working GPU adapter"]
fn gpu_bad_red_detection_reserves_final_slot_and_preserves_exhausted_masks() {
    let m = metadata(
        32,
        32,
        [10., 10., 255.].repeat(150),
        vec![],
        vec![0.; 9],
        vec![],
    );
    let p = prepare(&m, Threshold::Manual(2)).unwrap();
    let gpu = Gpu::new().unwrap();
    let original = gpu
        .upload("Capacity raw signal", &[10_f32, 0., 0.].repeat(32 * 32))
        .unwrap();
    let mut output = gpu
        .upload(
            "Capacity calibrated output",
            &[30_f32, 20., 10.].repeat(32 * 32),
        )
        .unwrap();
    let mut records = gpu.upload("Capacity records", &p.records).unwrap();
    let uniform = gpu.uniform(&p.params);
    select_bad_reds(
        &gpu,
        &original,
        &mut records,
        &uniform,
        p.params,
        &mut Default::default(),
    )
    .unwrap();
    correct_lists(
        &gpu,
        &records,
        &mut output,
        &uniform,
        &p,
        &mut Default::default(),
    )
    .unwrap();
    let bytes = gpu.readback(&records).unwrap();
    let records = bytemuck::cast_slice::<u8, [u32; 2]>(&bytes);
    assert_eq!(records[150 + 999], [999, 0]);
    assert!(records[..124].iter().all(|r| r[1] == 255));
    assert_eq!(records[124][1], 128);
    assert!(records[125..150].iter().all(|r| r[1] == 255));
    assert_eq!(records[150 + 998], [11 * 32 + 10, 0x43fd]);
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn gpu_jpeg_camera_list_repairs_reported_dark_block() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/data/dark-defect-reference.json"
    ))
    .unwrap();
    let jpeg: Vec<_> = f["jpeg_records"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| {
            [
                r[1].as_u64().unwrap() as f64 - 2012.,
                r[0].as_u64().unwrap() as f64 - 864.,
                u32::from_str_radix(r[2].as_str().unwrap().trim_start_matches("0x"), 16).unwrap()
                    as f64,
            ]
        })
        .collect();
    let m = metadata(9, 9, vec![], jpeg, vec![0.; 9], vec![]);
    let p = prepare(&m, Threshold::Manual(2)).unwrap();
    let gpu = Gpu::new().unwrap();
    let original = gpu
        .upload(
            "Original independent detector backing",
            &vec![0_f32; 9 * 9 * 3],
        )
        .unwrap();
    for gain in [0., 1., 1.3] {
        let samples: Vec<_> = numbers(&f["before"])
            .into_iter()
            .map(|v| v as f32 * gain)
            .collect();
        let input = gpu
            .upload("Dark defect camera code fixture", &samples)
            .unwrap();
        let output = apply(&gpu, &original, &input, &p, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        for (&actual, expected) in bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .zip(numbers(&f["after"]))
        {
            assert!((actual - expected as f32 * gain).abs() < 0.002);
        }
        assert_eq!(
            gpu.readback(&input).unwrap(),
            bytemuck::cast_slice::<f32, u8>(&samples)
        );
    }
}
#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn flower_dark_defect_is_restored_by_complete_camera_repair() {
    use crate::{
        inspection::Content,
        render::{Engine, Settings},
        source::Source,
    };
    let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
        .join("sunlit-red-flower.x3f");
    let source = Source::open(path).unwrap();
    let gpu = Gpu::new().unwrap();
    let engine = Engine::from_device(
        gpu.device.clone(),
        gpu.queue.clone(),
        gpu.adapter_name.clone(),
    )
    .unwrap();
    let settings = Settings::new("Sunlight", "Standard");
    let artifacts = engine
        .render_artifacts(
            &source,
            &settings,
            &["calibration.layers", "repair.layers", "despeckle.layers"],
            &mut Profile::default(),
        )
        .unwrap();
    let data: Vec<_> = artifacts
        .iter()
        .map(|a| {
            let Content::Image(i) = &a.content else {
                panic!()
            };
            let bytes = gpu.readback(i.buffer()).unwrap();
            bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()
        })
        .collect();
    let p = (2016 * source.metadata.width + 868) * 3;
    // The JPEG-list fixture isolates that repair. The full sensor route must
    // also apply the overlapping cluster afterward to match the native result.
    for (c, expected) in [392.66168, 406.26483, 328.91763].into_iter().enumerate() {
        assert!(
            (data[1][p + c] - expected).abs() < 0.01,
            "channel {c}: {} != {expected}",
            data[1][p + c]
        );
        assert!(data[1][p + c] > data[0][p + c] * 1.9);
        assert_eq!(data[1][p + c], data[2][p + c]);
    }
    let Content::Image(i) = &artifacts[0].content else {
        panic!()
    };
    assert_eq!(
        gpu.readback(i.buffer()).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&data[0])
    );
}
