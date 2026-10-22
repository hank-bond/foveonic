//! Checks phase-preserving padded local contrast against independent lifting equations.
use super::*;
#[test]
fn rejects_nondivisible_padding() {
    assert!(Prepared::new(
        [64, 64],
        [65, 64],
        [0, 0, 64, 64],
        [BandControl {
            gain: 1.,
            limit: 100.,
            slope: 0.5
        }; 6],
        [0.; 9],
        0.35
    )
    .is_err());
}
#[test]
#[ignore = "requires GPU and WAVELET_REFERENCE"]
fn phase_preserving_contrast_matches_independent_equations() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("WAVELET_REFERENCE").expect("set WAVELET_REFERENCE"),
    );
    let cases: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("cases.json")).unwrap()).unwrap();
    let read = |name: String| -> Vec<f32> {
        let b = std::fs::read(directory.join(name)).unwrap();
        b.as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect()
    };
    let gpu = Gpu::new().unwrap();
    for c in cases.as_array().unwrap() {
        let n = c["n"].as_u64().unwrap();
        let array = |name: &str| {
            c[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect::<Vec<_>>()
        };
        let ints = |name: &str| {
            array(name)
                .into_iter()
                .map(|v| v as u32)
                .collect::<Vec<_>>()
        };
        let floats = |name: &str| {
            array(name)
                .into_iter()
                .map(|v| v as f32)
                .collect::<Vec<_>>()
        };
        let prepared = Prepared::new(
            ints("extent").try_into().unwrap(),
            ints("padded").try_into().unwrap(),
            ints("crop").try_into().unwrap(),
            std::array::from_fn(|i| BandControl {
                gain: floats("gains")[i],
                limit: [100., 100., 100., 100., 25., 25.][i],
                slope: [0.5, 0.55, 0.6, 0.6, 0.7, 0.8][i],
            }),
            floats("kernel").try_into().unwrap(),
            0.35,
        )
        .unwrap();
        let input = gpu
            .upload("Independent packed Lab", &read(format!("{n}-input.f32")))
            .unwrap();
        let output = apply(&gpu, &input, &prepared, &mut Default::default()).unwrap();
        let b = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&b);
        let expected = read(format!("{n}-output.f32"));
        let mut differences = 0;
        let mut maximum = 0_f32;
        for (i, (&a, &b)) in actual.iter().zip(&expected).enumerate() {
            let e = (a - b).abs();
            differences += usize::from(a.to_bits() != b.to_bits());
            maximum = maximum.max(e);
            assert_eq!(a, b, "case{n} sample{i} {a} vs {b}");
        }
        eprintln!("Wavelet case{n} differences{differences} max{maximum}");
    }
}
