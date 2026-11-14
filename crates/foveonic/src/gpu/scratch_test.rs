//! Checks completed spare limits and retained-reader safety with small GPU allocations.

use super::*;

#[test]
#[ignore = "requires a working GPU adapter"]
fn completed_spares_keep_one_per_size_within_the_limit_and_remain_reusable() {
    let gpu = Gpu::new().unwrap();
    let mut scratch = Scratch::default();
    // Allocate before releasing so the pool contains real duplicate spares.
    let buffers = [16, 16, 32, 64, 256].map(|size| scratch.take(&gpu, size).unwrap());
    let first_identity = buffers[0].identity();
    gpu.wait_for_submitted_work().unwrap();
    for buffer in buffers {
        scratch.release(buffer);
    }
    assert_eq!(scratch.buffers().count(), 5);
    scratch.trim_completed(96);
    let sizes = scratch
        .buffers()
        .map(|buffer| buffer.size())
        .collect::<Vec<_>>();
    assert_eq!(sizes, [16, 32]);
    assert!(sizes.iter().sum::<u64>() <= 96);
    let recycled = scratch.take(&gpu, 16).unwrap();
    assert_eq!(recycled.identity(), first_identity);
    assert_eq!(scratch.reuses, 1);
    assert_eq!(scratch.allocations, 5);
    scratch.release(recycled);
    scratch.trim_completed(0);
    assert_eq!(scratch.buffers().count(), 0);
    let replacement = scratch.take(&gpu, 16).unwrap();
    assert_ne!(replacement.identity().0, first_identity.0);
    assert_eq!(scratch.allocations, 6);
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn trimming_spares_does_not_change_a_retained_readers_samples() {
    let gpu = Gpu::new().unwrap();
    let original = gpu
        .upload("retained scratch fixture", &[1_u32, 2, 3, 4])
        .unwrap();
    let reader = original.clone();
    let identity = reader.identity();
    let mut scratch = Scratch::default();
    gpu.wait_for_submitted_work().unwrap();
    scratch.release(original);
    scratch.trim_completed(0);
    assert_eq!(scratch.buffers().count(), 0);
    assert_eq!(reader.identity(), identity);
    assert_eq!(
        gpu.readback(&reader).unwrap(),
        bytemuck::cast_slice::<u32, u8>(&[1, 2, 3, 4])
    );
}
