//! Checks odd-phase run repairs and source initialization requirements.
use super::*;
#[test]
fn rejects_missing_halo() {
    assert!(Prepared::new([20, 20], [1, 1, 18, 18], 13995.).is_err());
}
#[test]
#[ignore = "requires GPU"]
fn consecutive_runs_and_first_row_precondition() {
    let gpu = Gpu::new().unwrap();
    let prepared = Prepared::new([20, 20], [3, 1, 16, 18], 13995.).unwrap();
    let mut input = vec![500_f32; 400];
    for y in [3, 5, 7, 9] {
        input[y * 20 + 4] = 15000.;
        input[y * 20 + 5] = 400.;
    }
    let buffer = gpu.upload("Blue mismatch active runs", &input).unwrap();
    let output = apply(&gpu, &buffer, &prepared, &mut Default::default()).unwrap();
    let b = gpu.readback(&output).unwrap();
    let actual = bytemuck::cast_slice::<u8, f32>(&b);
    for y in [3, 5, 7, 9] {
        input[y * 20 + 5] = 500.;
    }
    assert_eq!(actual, input);
    input[20 + 4] = 15000.;
    input[20 + 5] = 400.;
    let buffer = gpu
        .upload("Blue mismatch uninitialized first row", &input)
        .unwrap();
    assert!(apply(&gpu, &buffer, &prepared, &mut Default::default()).is_err());
}
