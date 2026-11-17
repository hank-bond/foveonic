//! Checks guided log reconstruction contracts and bounded native outputs.
use super::*;
#[test]
fn rejects_invalid_log_reconstruction_controls() {
    assert!(Prepared::new([16, 16], [0.; 10]).is_err());
    assert!(Prepared::new([2, 2], [1.; 10]).is_err());
}

#[test]
fn log_decay_uses_native_physical_extent_and_ignores_allocation_padding() {
    use crate::render::{Scale, WorkingGrid};
    let controls = [0.0011, 0.0019, 0.019, 1., 0.55, 5., 7., 0.7, 3., 1.];
    let native = Prepared::new([2640, 1760], controls).unwrap();
    for (index, control) in controls.iter().enumerate().take(3) {
        let original =
            1. / (std::f32::consts::LN_2 / (((2640 + 1760) >> 2) as f32 * control)).exp();
        assert_eq!(native.decay(index).to_bits(), original.to_bits());
    }
    for level in 0..32 {
        let scale = Scale::new(1 << level).unwrap();
        let grid = WorkingGrid::new([2640, 1760], scale, 8)
            .unwrap()
            .with_halo(8)
            .unwrap();
        let padded = WorkingGrid::new([2640, 1760], scale, 8)
            .unwrap()
            .with_halo(64)
            .unwrap();
        let first = native.clone().on_grid(grid).unwrap();
        let second = native.clone().on_grid(padded).unwrap();
        assert_eq!(first.extent, grid.extent);
        for index in 0..3 {
            assert_eq!(first.decay(index), second.decay(index));
            assert!(first.decay(index).is_finite());
            let expected = native.decay(index).powf(scale.divisor() as f32);
            assert!((first.decay(index) - expected).abs() < 1e-5);
        }
        if scale.is_reduced() {
            assert_ne!(
                first.key(),
                second.key(),
                "allocation geometry stays part of identity"
            );
        }
    }
    let wrong = WorkingGrid::new([2632, 1760], Scale::new(2).unwrap(), 8).unwrap();
    assert!(native.on_grid(wrong).is_err());
}
#[test]
#[ignore = "requires a GPU"]
fn gpu_log_reconstruction_matches_bounded_native_output() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/data/log-reconstruction-reference.json"
    ))
    .unwrap();
    let gpu = Gpu::test_executor();
    for case in f["cases"].as_array().unwrap() {
        let array = |name: &str| {
            case[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap() as f32)
                .collect::<Vec<_>>()
        };
        let original = gpu
            .upload("Native log reconstruction original", &array("original"))
            .unwrap();
        let filtered = gpu
            .upload("Native log reconstruction filtered", &array("filtered"))
            .unwrap();
        let p = Prepared::new(
            [
                case["width"].as_u64().unwrap() as u32,
                case["height"].as_u64().unwrap() as u32,
            ],
            std::array::from_fn(|i| case["controls"][i].as_f64().unwrap() as f32),
        )
        .unwrap();
        let out = apply(&gpu, &original, &filtered, &p, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(&out).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        let expected = array("expected");
        let error = actual
            .iter()
            .zip(&expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0_f32, f32::max);
        eprintln!(
            "log reconstruction {:?}: max {error}, changed {} / {}",
            p.extent,
            actual.iter().zip(&expected).filter(|(a, b)| a != b).count(),
            actual.len()
        );
        assert!(actual.iter().all(|v| v.is_finite()));
        assert!(
            error <= 2.,
            "shape {:?}, maximum signed-code error {error}",
            p.extent
        );
    }
}

#[test]
#[ignore = "requires a GPU and FOVEONIC_LOG_CAPTURE native boundary directory"]
fn gpu_log_reconstruction_matches_original_camera_boundary() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("FOVEONIC_LOG_CAPTURE")
            .expect("set FOVEONIC_LOG_CAPTURE to an explicit native boundary directory"),
    );
    let f: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("fir.json")).unwrap()).unwrap();
    let extent = [
        f["original"]["width"].as_u64().unwrap() as u32,
        f["original"]["height"].as_u64().unwrap() as u32,
    ];
    assert_eq!(
        f["original"]["stride"].as_u64().unwrap(),
        u64::from(extent[0]) * 6
    );
    let read = |name: &str| {
        std::fs::read(directory.join(name))
            .unwrap()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| i16::from_le_bytes([v[0], v[1]]) as f32)
            .collect::<Vec<_>>()
    };
    let gpu = Gpu::new().unwrap();
    let original = gpu
        .upload("Actual camera FIR input", &read("original.i16"))
        .unwrap();
    let filtered = gpu
        .upload(
            "Actual camera FIR filtered reference",
            &read("filtered.i16"),
        )
        .unwrap();
    let p = Prepared::new(
        extent,
        std::array::from_fn(|i| f["parameters"][i].as_f64().unwrap() as f32),
    )
    .unwrap();
    let out = apply(&gpu, &original, &filtered, &p, &mut Profile::default()).unwrap();
    let expected = read("output.i16");
    assert_eq!(expected.len(), extent[0] as usize * extent[1] as usize * 3);
    let bytes = gpu.readback(&out).unwrap();
    let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
    let mut maximum = 0_f32;
    let mut changed = 0;
    let mut sum = 0_f64;
    for (a, b) in actual.iter().zip(&expected) {
        assert!(a.is_finite());
        let d = (a - b).abs();
        maximum = maximum.max(d);
        sum += f64::from(d);
        changed += usize::from(d != 0.);
    }
    eprintln!(
        "Actual camera log reconstruction: {changed}/{} changed, max {maximum}, mean {}",
        actual.len(),
        sum / actual.len() as f64
    );
    assert!(maximum <= 2., "maximum signed code error {maximum}");
}
