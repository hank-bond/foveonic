//! Checks bounded box local contrast against completed native wrappers.
use super::*;
#[test]
fn prepared_controls_keep_iso_and_each_pass_in_the_cache_key() {
    let a = Prepared::new([64, 64], [(7, 0.4), (3, 0.2)], 8.).unwrap();
    let b = Prepared::new([64, 64], [(7, 0.4), (3, 0.2)], 32.).unwrap();
    assert!(a.key() != b.key());
    assert!(Prepared::new([64, 64], [(0, 0.4), (3, 0.2)], 8.).is_err());
}

#[test]
#[ignore = "requires GPU and STAGE3_NATIVE completed high-ISO main/sharpen boundaries"]
fn complete_native_two_average_passes_match_the_active_image() {
    let directory = std::path::PathBuf::from(std::env::var_os("STAGE3_NATIVE").unwrap());
    let helper = std::fs::read(directory.join("helper.bin")).unwrap();
    let iso = f32::from_le_bytes(helper[160..164].try_into().unwrap());
    let level = i32::from_le_bytes(helper[252..256].try_into().unwrap());
    assert!(iso >= 8. && (0..=6).contains(&level));
    let bytes = std::fs::read(directory.join("main-input-rgb.u16")).unwrap();
    let values = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|v| f32::from(u16::from_le_bytes(*v)))
        .collect::<Vec<_>>();
    let gpu = Gpu::new().unwrap();
    let input = gpu
        .upload("Native before spatial averages", &values)
        .unwrap();
    let control = level as f32 / 3.;
    let prepared =
        Prepared::new([5888, 3672], [(7, control * 0.2), (3, control * 0.1)], iso).unwrap();
    let output = apply(&gpu, &input, &prepared, &mut Default::default()).unwrap();
    let bytes = gpu.readback(&output).unwrap();
    let actual: &[f32] = bytemuck::cast_slice(&bytes);
    let bytes = std::fs::read(directory.join("sharp-input-rgb.u16")).unwrap();
    let expected = bytes.as_chunks::<2>().0;
    let mut changed = 0;
    let mut maximum = 0_f32;
    let mut sum = 0_f64;
    let mut worst = (0, 0_f32, 0_f32);
    for y in 28..=3643 {
        for x in 216..=5639 {
            for ch in 0..3 {
                let i = (y * 5888 + x) * 3 + ch;
                let expected = f32::from(u16::from_le_bytes(expected[i]));
                let difference = (actual[i] - expected).abs();
                changed += usize::from(difference != 0.);
                sum += f64::from(difference);
                if difference > maximum {
                    maximum = difference;
                    worst = (i, actual[i], expected);
                }
            }
        }
    }
    eprintln!(
        "Complete two-average active RGB: changed{changed}, max{maximum}, mean{}, worst{worst:?}",
        sum / (5424. * 3616. * 3.)
    );
    assert_eq!(maximum, 0.);
}
