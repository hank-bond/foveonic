//! Compares GPU clipped-layer restoration, immutable flag connectivity, density, and gain handling with native fixtures.

use super::*;
use crate::gpu::Binding::{Read, Write};
use crate::source::Matrix;
fn values(v: &serde_json::Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect()
}
#[test]
#[ignore = "requires a working GPU adapter"]
fn donor_bin_grouping_is_stable_across_blocks_partial_tail_and_empty_bins() {
    let gpu = Gpu::new().unwrap();
    let rgb: Vec<f32> = (0..600)
        .flat_map(|i| [((i % 13 + 1) * 10) as f32, 100., (7 + i % 5) as f32])
        .collect();
    let ids: Vec<u32> = (0..270).map(|i| i * 2).collect();
    let bucket = |p: u32| {
        let c = &rgb[p as usize * 3..][..3];
        (c[0] * 255. / (c[0] + c[1])) as u32
    };
    let mut expected = ids.clone();
    expected.sort_by_key(|p| bucket(*p));
    let input = gpu.upload("Bin RGB", &rgb).unwrap();
    let donors = gpu.upload("Bin donors", &ids).unwrap();
    let counts = gpu.upload("Bin donor total", &[0_u32, 0, 0, 270]).unwrap();
    let mut marks = gpu.storage("Reused bin scratch", 600 * 4).unwrap();
    let mut histogram = gpu.storage("Bin histograms", 3 * 256 * 4).unwrap();
    let mut starts = gpu.storage("Bin offsets", 257 * 4).unwrap();
    let mut sorted = gpu.storage("Stable bin output", 600 * 4).unwrap();
    let params = gpu.uniform(&Params {
        source: [30, 20, 600, 3],
        area: [0; 4],
        value: [0., 1., 0., 0.],
    });
    gpu.dispatch(
        Kernel::HighlightBinHistogram,
        &mut [
            (2, Read(&params)),
            (5, Read(&input)),
            (6, Write(marks.write())),
            (7, Read(&counts)),
            (8, Read(&donors)),
            (11, Write(histogram.write())),
        ],
        [3, 1],
        &mut Default::default(),
    );
    gpu.dispatch(
        Kernel::HighlightBinPrefix,
        &mut [
            (2, Read(&params)),
            (7, Read(&counts)),
            (11, Write(histogram.write())),
            (12, Write(starts.write())),
        ],
        [1, 1],
        &mut Default::default(),
    );
    gpu.dispatch(
        Kernel::HighlightBinScatter,
        &mut [
            (2, Read(&params)),
            (6, Read(&marks)),
            (7, Read(&counts)),
            (8, Read(&donors)),
            (10, Write(sorted.write())),
            (11, Read(&histogram)),
            (12, Read(&starts)),
        ],
        [3, 1],
        &mut Default::default(),
    );
    let bytes = gpu.readback(&sorted).unwrap();
    assert_eq!(&bytemuck::cast_slice::<u8, u32>(&bytes)[..270], expected);
    let bytes = gpu.readback(&starts).unwrap();
    for (b, &start) in bytemuck::cast_slice::<u8, u32>(&bytes).iter().enumerate() {
        assert_eq!(
            start as usize,
            ids.iter().filter(|p| bucket(**p) < b as u32).count()
        );
    }
    let bytes = gpu.readback(&marks).unwrap();
    assert_eq!(
        &bytemuck::cast_slice::<u8, u32>(&bytes)[..270],
        ids.iter().map(|p| bucket(*p)).collect::<Vec<_>>()
    );
}

#[test]
fn camera_saturation_maps_preserve_original_positions_and_validate_coverage() {
    let mut m = Metadata {
        width: 8,
        height: 8,
        rotation: 0,
        properties: Default::default(),
        lists: Default::default(),
        matrices: Default::default(),
    };
    let matrix = |values: Vec<f64>| Matrix {
        encoding: MatrixEncoding::Uint32,
        representation: "u32",
        dimensions: vec![values.len()],
        values,
    };
    m.matrices
        .insert("SatMapR".into(), matrix(vec![1., 2., 61.]));
    m.matrices
        .insert("SatMapFullG".into(), matrix(vec![2., 7.]));
    m.matrices.insert("SatMapB".into(), matrix(vec![64.]));
    let flags = saturation(&m).unwrap();
    assert_eq!(&flags[..8], &[0, 4, 6, 0, 0, 0, 0, 2]);
    m.matrices.get_mut("SatMapR").unwrap().values[2] = 60.;
    assert!(saturation(&m).is_err());
    m.matrices.remove("SatMapR");
    assert!(saturation(&m).is_err());
}
#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn highlight_density_inspection_skips_photographic_prefix_and_retains_original_provenance() {
    use crate::{
        inspection::Content,
        render::{Engine, Settings},
        source::Source,
    };
    let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
        .join("sunlit-red-flower.x3f");
    let mut source = Source::open(path).unwrap();
    let metadata = std::sync::Arc::make_mut(&mut source.metadata);
    metadata.lists.clear(); // No WB/color choices are needed to inspect original flags.
    metadata.matrices.remove("BadnessTable");
    let settings = Settings::new("missing WB", "missing mode");
    let engine = Engine::new().unwrap();
    let mut profile = Profile::new(crate::profile::Mode::Gpu);
    let artifacts = engine
        .render_artifacts(&source, &settings, &["highlight.density"], &mut profile)
        .unwrap();
    assert!(!profile.host.iter().any(|t| [
        "prepare black",
        "prepare calibration",
        "prepare repair records",
        "prepare reference noise",
        "prepare color matrix"
    ]
    .contains(&t.stage)));
    if engine
        .device()
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY)
    {
        assert_eq!(profile.gpu.len(), 3);
    }
    let Content::Image(image) = &artifacts[0].content else {
        panic!()
    };
    assert_eq!(
        image.description.layout,
        crate::image::Layout::Scalar32Float
    );
    let gpu = Gpu::from_device(
        engine.device().clone(),
        engine.queue().clone(),
        "test".into(),
    )
    .unwrap();
    let before = gpu.readback(image.buffer()).unwrap();
    let values = bytemuck::cast_slice::<u8, f32>(&before);
    assert!(values.iter().all(|v| (0. ..=1.).contains(v)));
    assert!(values.contains(&1.));
    let mut changed = settings;
    changed.sensor_gain = 0.;
    engine
        .render_artifacts(
            &source,
            &changed,
            &["highlight.density"],
            &mut Profile::default(),
        )
        .unwrap();
    assert!(
        gpu.readback(image.buffer()).unwrap() == before,
        "retained original clipping density changed"
    );
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn gpu_highlight_restoration_matches_native_saturation_density_and_gain() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("../tests/data/highlight-reference.json")).unwrap();
    let w = f["width"].as_u64().unwrap() as u32;
    let h = f["height"].as_u64().unwrap() as u32;
    let raw = values(&f["flags"]);
    let flags: Vec<_> = raw
        .as_chunks::<3>()
        .0
        .iter()
        .map(|c| (c[0] as u32) * 4 + (c[1] as u32) * 2 + c[2] as u32)
        .collect();
    let mut p = Prepared {
        params: Params {
            source: [w, h, w * h, (w * h).div_ceil(256)],
            area: values(&f["area"])
                .into_iter()
                .map(|v| v as u32)
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            value: [5500., 1., 0., 0.],
        },
        flags,
    };
    let gpu = Gpu::new().unwrap();
    for gain in [0., 1., 1.3] {
        p.params.value[1] = gain;
        let samples: Vec<_> = values(&f["input"])
            .into_iter()
            .map(|v| v as f32 * gain)
            .collect();
        let input = gpu.upload("Immutable highlight fixture", &samples).unwrap();
        let result = apply(&gpu, Some(&input), &p, true, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(result.pixels.as_ref().unwrap()).unwrap();
        for (i, (&actual, reference)) in bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .zip(values(&f["output"]))
            .enumerate()
        {
            assert!(
                (actual - reference as f32 * gain).abs() < 0.012,
                "gain {gain} sample {i}: {actual} != {}",
                reference as f32 * gain
            );
        }
        let bytes = gpu.readback(result.density.as_ref().unwrap()).unwrap();
        for (&actual, reference) in bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .zip(values(&f["density"]))
        {
            assert!((actual - reference as f32).abs() < 0.000001);
        }
        assert_eq!(
            gpu.readback(&input).unwrap(),
            bytemuck::cast_slice::<f32, u8>(&samples)
        );
    }
    let result = apply(&gpu, None, &p, true, &mut Profile::default()).unwrap();
    assert!(result.pixels.is_none());
    assert!(result.density.is_some());
}
