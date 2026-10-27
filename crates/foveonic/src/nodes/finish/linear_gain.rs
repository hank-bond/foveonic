//! Transfers a known scalar brightness edit between two linear RGB representations.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    source::Result,
};

/// `after` must be `before` multiplied by a per-pixel scalar, in the same RGB
/// basis. Shared linear brightness edits satisfy this contract. Use the strongest
/// component to recover that gain without dividing by near-zero luminance or
/// cancelling signed RGB values. A zero reference vector leaves the source alone.
pub(crate) fn apply(
    gpu: &Gpu,
    source: &ReadGuard,
    before: &ReadGuard,
    after: &ReadGuard,
    extent: [u32; 2],
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let bytes = u64::from(extent[0]) * u64::from(extent[1]) * 12;
    if extent.contains(&0)
        || [source, before, after]
            .iter()
            .any(|input| input.size() != bytes)
    {
        return Err("linear gain transfer requires matching RGB geometry".into());
    }
    let mut output = gpu.storage("Baked linear brightness edits", bytes)?;
    let params = gpu.uniform(&[extent[0], extent[1], 0, 0]);
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::TransferLinearGain,
        &mut [
            (0, Read(source)),
            (1, Read(before)),
            (2, Read(after)),
            (3, Read(&params)),
            (4, Write(output.write())),
        ],
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a GPU"]
    fn gain_transfer_preserves_signed_samples_and_handles_zero_luminance() {
        let gpu = Gpu::new().unwrap();
        let source = [
            [-5_f32, 10., 20.],
            [100., 200., 300.],
            [0., -50., 0.],
            [2., 3., 4.],
            [5000., -200., 100.],
        ];
        let before = [
            [-1_f32, 1., 0.],
            [1e-8, 1e-7, -3e-6],
            [0., 0., -200.],
            [0.; 3],
            [9000., 10., 50.],
        ];
        let gains = [2_f32, 0.125, 4., 1., 0.];
        let after: Vec<[f32; 3]> = before
            .iter()
            .zip(gains)
            .map(|(v, g)| v.map(|v| v * g))
            .collect();
        let input = gpu.upload("Signed DNG source", &source).unwrap();
        let reference = gpu.upload("Unedited reference", &before).unwrap();
        let edited = gpu.upload("Scalar-edited reference", &after).unwrap();
        let result = apply(
            &gpu,
            &input,
            &reference,
            &edited,
            [5, 1],
            &mut Profile::default(),
        )
        .unwrap();
        let bytes = gpu.readback(&result).unwrap();
        let values: &[[f32; 3]] = bytemuck::cast_slice(&bytes);
        for i in 0..5 {
            for c in 0..3 {
                let expected = source[i][c] * gains[i];
                assert!((values[i][c] - expected).abs() < 0.00001 + expected.abs() * 0.000002);
            }
        }
        assert_eq!(
            gpu.readback(&input).unwrap(),
            bytemuck::cast_slice::<[f32; 3], u8>(&source)
        );
        assert!(apply(
            &gpu,
            &input,
            &reference,
            &edited,
            [4, 1],
            &mut Profile::default()
        )
        .is_err());
    }
}
