//! Checks pooled signal extents, exact block means and immutable clipping coverage.
use super::*;

#[test]
fn pooling_requires_complete_physical_cells() {
    for divisor in [2, 4, 8] {
        let prepared = Prepared::new([64, 48], divisor, Signal::Rgb).unwrap();
        assert_eq!(prepared.output_extent(), [64 / divisor, 48 / divisor]);
        assert!(Prepared::new([63, 48], divisor, Signal::Rgb).is_err());
    }
    assert!(Prepared::new([64, 48], 3, Signal::Rgb).is_err());
}

#[test]
#[ignore = "requires GPU"]
fn pooling_preserves_means_coverage_and_source() {
    let gpu = Gpu::new().unwrap();
    for divisor in [2, 4, 8] {
        for signal in [Signal::Rgb, Signal::Blue, Signal::Mask] {
            let p = Prepared::new([16, 16], divisor, signal).unwrap();
            let components = p.controls[1] as usize;
            let mask = matches!(signal, Signal::Mask);
            let values: Vec<u32> = (0..16 * 16 * components)
                .map(|i| {
                    if mask {
                        if i % 17 == 0 {
                            255
                        } else {
                            0
                        }
                    } else {
                        (i as f32 - 130.).to_bits()
                    }
                })
                .collect();
            let input = gpu.upload("Pool source", &values).unwrap();
            let output = apply(&gpu, &input, &p, &mut Profile::default()).unwrap();
            let bytes = gpu.readback(&output).unwrap();
            let actual: &[u32] = bytemuck::cast_slice(&bytes);
            let [w, h] = p.output_extent();
            for y in 0..h {
                for x in 0..w {
                    for c in 0..components {
                        let cell: Vec<_> = (0..divisor)
                            .flat_map(|dy| {
                                (0..divisor).map(move |dx| {
                                    (((y * divisor + dy) * 16 + x * divisor + dx) as usize
                                        * components)
                                        + c
                                })
                            })
                            .collect();
                        let expected = if mask {
                            cell.iter().map(|i| values[*i]).max().unwrap()
                        } else {
                            (cell.iter().map(|i| f32::from_bits(values[*i])).sum::<f32>()
                                / (divisor * divisor) as f32)
                                .to_bits()
                        };
                        assert_eq!(actual[((y * w + x) as usize * components) + c], expected);
                    }
                }
            }
            assert_eq!(
                gpu.readback(&input).unwrap(),
                bytemuck::cast_slice::<u32, u8>(&values)
            );
        }
    }
}
