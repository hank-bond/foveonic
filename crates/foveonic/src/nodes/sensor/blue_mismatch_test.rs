//! Checks odd-phase run repairs, including a run that starts on the first scanned row.
use super::*;
#[test]
fn rejects_missing_halo() {
    assert!(Prepared::new([20, 20], [1, 1, 18, 18], 13995.).is_err());
}
/// Marks column 5 eligible on `rows` and returns the input and the repair.
fn repair(gpu: &Gpu, rows: &[usize]) -> (Vec<f32>, Vec<f32>) {
    let prepared = Prepared::new([20, 20], [3, 1, 16, 18], 13995.).unwrap();
    let mut input = vec![500_f32; 400];
    for y in rows {
        input[y * 20 + 4] = 15000.;
        input[y * 20 + 5] = 400.;
    }
    let buffer = gpu.upload("Blue mismatch runs", &input).unwrap();
    let output = apply(gpu, &buffer, &prepared, &mut Default::default()).unwrap();
    let repaired = bytemuck::cast_slice::<u8, f32>(&gpu.readback(&output).unwrap()).to_vec();
    (input, repaired)
}
/// The input with column 5 restored to its neighbors on `rows`.
fn restored(mut input: Vec<f32>, rows: &[usize]) -> Vec<f32> {
    for y in rows {
        input[y * 20 + 5] = 500.;
    }
    input
}
#[test]
#[ignore = "requires GPU"]
fn consecutive_runs_are_repaired() {
    let gpu = Gpu::new().unwrap();
    let rows = [3, 5, 7, 9];
    let (input, repaired) = repair(&gpu, &rows);
    assert_eq!(repaired, restored(input, &rows));
}
#[test]
#[ignore = "requires GPU"]
fn an_eligible_first_row_starts_a_run() {
    let gpu = Gpu::new().unwrap();
    let rows = [1, 3, 5];
    let (input, repaired) = repair(&gpu, &rows);
    assert_eq!(repaired, restored(input, &rows));
    // Counting starts from zero, so two eligible rows are still too short.
    let (input, repaired) = repair(&gpu, &[1, 3]);
    assert_eq!(repaired, input);
}
