//! Compares ordered GPU camera repair to native mask/order fixtures and regresses the leaf defect block.

use crate::gpu::Binding::{Read, Write};
use crate::{
    gpu::{Gpu, Kernel},
    source::{Matrix, MatrixEncoding, Metadata},
};

fn metadata(width: usize, height: usize, records: &[Vec<f64>]) -> Metadata {
    Metadata {
        width,
        height,
        rotation: 0,
        properties: Default::default(),
        lists: Default::default(),
        matrices: [(
            "BadPixelsF20".into(),
            Matrix {
                encoding: MatrixEncoding::Uint16,
                representation: "u16",
                dimensions: vec![3, records.len()],
                values: records.iter().flatten().copied().collect(),
            },
        )]
        .into(),
    }
}

#[test]
fn preparation_preserves_duplicate_order_masks_and_validates_records() {
    let mut m = metadata(
        8,
        8,
        &[
            vec![3., 3., 0x43ff as f64],
            vec![3., 4., 0x4397 as f64],
            vec![3., 3., 0x13a0 as f64],
            vec![4., 4., 0x41ff as f64],
        ],
    );
    let p = super::prepare(&m, 2).unwrap();
    assert_eq!(p.records, [[27, 0x43ff], [28, 0x4397], [27, 0x13a0]]);
    assert_eq!(p.params, [8, 8, 3, 0]);
    m.matrices.get_mut("BadPixelsF20").unwrap().values[1] = 8.;
    assert!(super::prepare(&m, 2).is_err());
    m.matrices.get_mut("BadPixelsF20").unwrap().values[1] = 3.5;
    assert!(super::prepare(&m, 2).is_err());
    assert!(super::prepare(&metadata(2, 8, &[]), 0).is_err());
    assert!(super::prepare(&metadata(8, 8, &[]), 0)
        .unwrap()
        .records
        .is_empty());
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn gpu_camera_repair_matches_reference_masks_order_borders_and_sentinels() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("../tests/data/ordered-repair-reference.json")).unwrap();
    let w = f["width"].as_u64().unwrap() as usize;
    let h = f["height"].as_u64().unwrap() as usize;
    let records: Vec<Vec<f64>> = f["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            r.as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect()
        })
        .collect();
    let input: Vec<f32> = f["input"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap() as f32)
        .collect();
    let gpu = Gpu::new().unwrap();
    for (reverse, key) in [(false, "output"), (true, "reverse_output")] {
        let mut selected = records.clone();
        if reverse {
            selected.reverse();
        }
        let p = super::prepare(&metadata(w, h, &selected), 2).unwrap();
        for gain in [0., 1., 1.3] {
            let values: Vec<_> = input.iter().map(|v| v * gain).collect();
            let source = gpu.upload("Immutable repair fixture", &values).unwrap();
            let mut output = gpu
                .storage("Ordered repair output", (values.len() * 4) as u64)
                .unwrap();
            gpu.copy(&source, output.write(), source.size());
            let rec = gpu.upload("Ordered camera records", &p.records).unwrap();
            let uniform = gpu.uniform(&p.params);
            gpu.dispatch(
                Kernel::Repair,
                &mut [
                    (1, Read(&rec)),
                    (2, Read(&uniform)),
                    (3, Write(output.write())),
                ],
                [1, 1],
                &mut Default::default(),
            );
            let bytes = gpu.readback(&output).unwrap();
            let result = bytemuck::cast_slice::<u8, f32>(&bytes);
            for (i, (actual, reference)) in
                result.iter().zip(f[key].as_array().unwrap()).enumerate()
            {
                let expected = reference.as_f64().unwrap() as f32 * gain;
                assert!(
                    (actual - expected).abs() < 0.003,
                    "{key} gain {gain} sample {i}: {actual} != {expected}"
                );
            }
            assert_eq!(
                gpu.readback(&source).unwrap(),
                bytemuck::cast_slice::<f32, u8>(&values)
            );
        }
    }
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn leaf_camera_record_block_is_repaired_before_despeckle_with_independent_artifacts() {
    use crate::{
        inspection::Content,
        profile::Profile,
        render::{Engine, Settings},
        source::Source,
    };
    let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
        .join("glossy-leaves-and-buds.x3f");
    let source = Source::open(path).unwrap();
    let gpu = Gpu::new().unwrap();
    let engine = Engine::from_device(
        gpu.device.clone(),
        gpu.queue.clone(),
        gpu.adapter_name.clone(),
    )
    .unwrap();
    let settings = Settings::new("Sunlight", "Standard");
    assert_eq!(
        settings.repair.unwrap().resolve(&source.metadata).unwrap(),
        2
    );
    let artifacts = engine
        .render_artifacts(
            &source,
            &settings,
            &["calibration.layers", "repair.layers", "despeckle.layers"],
            &mut Profile::default(),
        )
        .unwrap();
    let samples: Vec<_> = artifacts
        .iter()
        .map(|a| {
            let Content::Image(i) = &a.content else {
                panic!()
            };
            let bytes = gpu.readback(i.buffer()).unwrap();
            bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()
        })
        .collect();
    for (x, y) in [(2800, 1589), (2801, 1589), (2800, 1590), (2801, 1590)] {
        let p = (y * source.metadata.width + x) * 3;
        assert!(
            samples[1][p] < samples[0][p] * 0.7,
            "({x},{y}): {} -> {}",
            samples[0][p],
            samples[1][p]
        );
        assert!(
            (samples[1][p] - 320.).abs() < 35.,
            "({x},{y}): {}",
            samples[1][p]
        );
        assert_eq!(&samples[0][p + 1..p + 3], &samples[1][p + 1..p + 3]);
        assert_eq!(&samples[1][p..p + 3], &samples[2][p..p + 3]);
    }
    let mut off = settings.clone();
    off.repair = None;
    off.despeckle = false;
    let again = engine
        .render_artifacts(
            &source,
            &off,
            &["calibration.layers"],
            &mut Profile::default(),
        )
        .unwrap();
    let Content::Image(i) = &again[0].content else {
        panic!()
    };
    assert_eq!(
        gpu.readback(i.buffer()).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&samples[0])
    );
    let Content::Image(retained) = &artifacts[1].content else {
        panic!()
    };
    assert_eq!(
        gpu.readback(retained.buffer()).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&samples[1])
    );
}
