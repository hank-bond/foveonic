//! Checks shadow-specific color reduction, brightness preservation and neutral bypass.

use super::*;

#[test]
fn neutral_default_and_shadow_controls_are_validated() {
    assert!(Controls::default().is_neutral());
    assert!(Controls::default().validate().is_ok());
    assert!(Controls {
        strength: f32::NAN,
        ..Controls::default()
    }
    .validate()
    .is_err());
    assert!(Controls {
        color_tolerance: 0.,
        ..Controls::default()
    }
    .validate()
    .is_err());
    assert!(Controls {
        shadow_ev: 0.,
        ..Controls::default()
    }
    .validate()
    .is_err());
}

#[test]
#[ignore = "requires a GPU"]
fn shadow_neutralization_preserves_brightness_and_protects_lights_and_strong_color() {
    let gpu = Gpu::new().unwrap();
    let extent = [128, 64];
    let colors = [
        [100., 95., 100.],
        [95., 100., 95.],
        [2000., 1900., 2000.],
        [0., 200., 0.],
    ];
    let values: Vec<f32> = (0..extent[0] * extent[1])
        .flat_map(|i| {
            let mut v = colors[((i % extent[0]) / 32) as usize];
            let grain = (i % 7) as f32 * 0.1;
            for c in &mut v {
                *c += grain;
            }
            v
        })
        .collect();
    let input = gpu.upload("Shadow color fixture", &values).unwrap();
    let p = Params::new(
        Controls {
            strength: 1.,
            color_tolerance: 0.1,
            ..Controls::default()
        },
        ColorSpace::Srgb,
        1.,
    );
    let result = apply(&gpu, &input, p, extent, &mut Profile::default()).unwrap();
    let bytes = gpu.readback(&result).unwrap();
    let result: &[f32] = bytemuck::cast_slice(&bytes);
    for (old, new) in values
        .as_chunks::<3>()
        .0
        .iter()
        .zip(result.as_chunks::<3>().0)
    {
        let luma = |v: &[f32; 3]| v[0] * 0.2126 + v[1] * 0.7152 + v[2] * 0.0722;
        assert!(
            (luma(old) - luma(new)).abs() < 0.002,
            "brightness changed: {old:?} -> {new:?}"
        );
    }
    let sample = |x: usize| &result[(32 * 128 + x) * 3..(32 * 128 + x) * 3 + 3];
    let purple = sample(16);
    assert!(
        (0. ..4.).contains(&(purple[0] - purple[1])),
        "near-neutral purple should move toward gray: {purple:?}"
    );
    let green = sample(48);
    assert!(
        (0. ..4.).contains(&(green[1] - green[0])),
        "near-neutral green should move toward gray: {green:?}"
    );
    assert_eq!(
        sample(80),
        &values[(32 * 128 + 80) * 3..(32 * 128 + 80) * 3 + 3]
    );
    assert!((sample(112)[1] - values[(32 * 128 + 112) * 3 + 1]).abs() < 2.);
    assert_eq!(
        gpu.readback(&input).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&values)
    );
    let bypass = apply(
        &gpu,
        &input,
        Params::new(Controls::default(), ColorSpace::Srgb, 1.),
        extent,
        &mut Profile::default(),
    )
    .unwrap();
    assert_eq!(
        gpu.readback(&bypass).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&values)
    );
}
