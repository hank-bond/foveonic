//! Checks shared readers, exclusive writes, guarded recycling and submission-ordered content lifetime.

use super::*;
use crate::{
    gpu::{Gpu, Kernel},
    scratch::Scratch,
};
use std::panic::{catch_unwind, AssertUnwindSafe};

#[test]
#[ignore = "requires a working GPU adapter"]
fn readers_block_writes_and_recycling_until_the_last_owner() {
    let gpu = Gpu::new().unwrap();
    let mut original = gpu.upload("guarded original", &[1_u32, 2, 3, 4]).unwrap();
    let reader = original.clone();
    let identity = reader.identity();
    assert_eq!(original.readers(), 2);
    assert!(catch_unwind(AssertUnwindSafe(|| {
        let _ = original.write();
    }))
    .is_err());
    assert_eq!(original.identity(), identity);

    let mut scratch = Scratch::default();
    scratch.release(original);
    let other = scratch.take(&gpu, reader.size()).unwrap();
    assert_ne!(other.identity().0, identity.0);
    assert_eq!(scratch.reuses, 0);
    assert_eq!(
        gpu.readback(&reader).unwrap(),
        bytemuck::cast_slice::<u32, u8>(&[1, 2, 3, 4])
    );
    drop(other);

    scratch.release(reader);
    let mut recycled = scratch.take(&gpu, 16).unwrap();
    assert_eq!(recycled.identity(), identity);
    assert_eq!(scratch.reuses, 1);
    let replacement = gpu.upload("new contents", &[5_u32, 6, 7, 8]).unwrap();
    gpu.copy(&replacement, recycled.write(), 16);
    assert_eq!(recycled.identity(), (identity.0, identity.1 + 1));
    assert_eq!(
        gpu.readback(&recycled).unwrap(),
        gpu.readback(&replacement).unwrap()
    );
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn queued_read_precedes_guarded_overwrite_without_completion_fences() {
    let gpu = Gpu::new().unwrap();
    let mut source = gpu.upload("original", &[1_u32, 2, 3, 4]).unwrap();
    let replacement = gpu.upload("replacement", &[5_u32, 6, 7, 8]).unwrap();
    let mut retained = gpu.storage("retained copy", 16).unwrap();
    gpu.copy(&source, retained.write(), 16);
    gpu.copy(&replacement, source.write(), 16);
    assert_eq!(
        gpu.readback(&retained).unwrap(),
        bytemuck::cast_slice::<u32, u8>(&[1, 2, 3, 4])
    );
    assert_eq!(
        gpu.readback(&source).unwrap(),
        gpu.readback(&replacement).unwrap()
    );
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn shader_write_cannot_be_mislabeled_as_a_read_binding() {
    let gpu = Gpu::new().unwrap();
    let source = gpu.upload("input", &[1_f32; 3]).unwrap();
    let params = gpu.uniform(&[0_u32; 32]);
    let output = gpu.storage("output", 12).unwrap();
    assert!(catch_unwind(AssertUnwindSafe(|| gpu.dispatch(
        Kernel::Color,
        &mut [
            (0, Binding::Read(&source)),
            (1, Binding::Read(&params)),
            (2, Binding::Read(&output))
        ],
        [1, 1],
        &mut Default::default(),
    )))
    .is_err());
}
