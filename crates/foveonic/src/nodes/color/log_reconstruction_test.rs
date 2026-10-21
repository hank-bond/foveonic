//! Checks guided log reconstruction contracts and bounded native outputs.
use super::*;
#[test]
fn rejects_invalid_log_reconstruction_controls() {
    assert!(Prepared::new([16, 16], [0.; 10]).is_err());
    assert!(Prepared::new([2, 2], [1.; 10]).is_err());
}
#[test]
#[ignore = "requires a GPU"]
fn gpu_log_reconstruction_matches_bounded_native_output() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/data/log-reconstruction-reference.json"
    ))
    .unwrap();
    let gpu = Gpu::new().unwrap();
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
