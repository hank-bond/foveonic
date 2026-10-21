//! Checks signed gain quantization, clipping, and overflow limits against wide integer arithmetic.

use super::*;

#[test]
fn rejects_invalid_parameters() {
    for gain in [0., -1., 64.01, f32::NAN, f32::INFINITY] {
        assert!(Prepared::new([17, 3], gain).is_err());
    }
    assert!(Prepared::new([0, 3], 1.).is_err());
    assert!(Prepared::new([17, 0], 1.).is_err());
}

#[test]
#[ignore = "requires a GPU"]
fn signed_ramps_match_wide_integer_gain() {
    let gpu = Gpu::new().unwrap();
    // Complete signed input coverage includes fractional negatives, clipping
    // thresholds, and the extrema that overflow a direct i32 Q12 product.
    let extent = [256, 256];
    let samples: Vec<f32> = (i16::MIN..=i16::MAX)
        .flat_map(|sample| [f32::from(sample); 3])
        .collect();
    let input = gpu.upload("Signed gain ramp", &samples).unwrap();
    for gain in [1., 1.675, 2.5, 63.999, 64.] {
        let prepared = Prepared::new(extent, gain).unwrap();
        let output = apply(&gpu, &input, &prepared, &mut Default::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        let q12 = (gain * 4096.) as i64;
        for (index, (&sample, &actual)) in samples.iter().zip(actual).enumerate() {
            let expected = ((sample as i64 * q12) >> 12).clamp(-500, 24000) as f32;
            assert_eq!(actual, expected, "gain {gain}, sample {index}: {sample}");
        }
    }
}
