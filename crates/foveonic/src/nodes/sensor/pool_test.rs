//! Checks pooled signal extents, exact block means and immutable clipping coverage.
use super::*;

#[test]
fn pooling_covers_partial_cells_at_every_power_of_two() {
    for divisor in [1, 2, 4, 8, 16, 64, 1 << 31] {
        let prepared = Prepared::new([65, 49], divisor, Signal::Rgb).unwrap();
        assert_eq!(
            prepared.output_extent(),
            [65_u32.div_ceil(divisor), 49_u32.div_ceil(divisor)]
        );
    }
    assert!(Prepared::new([64, 48], 3, Signal::Rgb).is_err());
    assert!(Prepared::new([0, 48], 2, Signal::Rgb).is_err());
}

#[test]
#[ignore = "requires GPU"]
fn pooling_preserves_means_coverage_and_source() {
    let gpu = Gpu::new().unwrap();
    for divisor in [2, 4, 8] {
        for signal in [Signal::Rgb, Signal::Blue, Signal::Mask, Signal::Flags] {
            let p = Prepared::new([16, 16], divisor, signal).unwrap();
            let components = p.controls[1] as usize;
            let mask = matches!(signal, Signal::Mask | Signal::Flags);
            let flags = matches!(signal, Signal::Flags);
            let values: Vec<u32> = (0..16 * 16 * components)
                .map(|i| {
                    if flags {
                        1 << (i % 3)
                    } else if mask {
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
                        let expected = if flags {
                            cell.iter().fold(0, |value, i| value | values[*i])
                        } else if mask {
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

#[test]
#[ignore = "requires GPU"]
fn partial_cells_and_halos_replicate_completed_means_and_flag_unions() {
    let gpu = Gpu::new().unwrap();
    for extent in [[1, 1], [17, 13]] {
        for divisor in [1, 2, 4, 16, 64, 1 << 31] {
            for signal in [Signal::Rgb, Signal::Blue, Signal::Mask, Signal::Flags] {
                let p = Prepared::new(extent, divisor, signal).unwrap();
                let components = p.controls[1] as usize;
                let values: Vec<u32> = (0..extent[0] as usize * extent[1] as usize * components)
                    .map(|i| match signal {
                        Signal::Mask => {
                            if i % 7 == 0 {
                                255
                            } else {
                                0
                            }
                        }
                        Signal::Flags => 1 << (i % 3),
                        _ => {
                            if i == 0 {
                                (-0_f32).to_bits()
                            } else {
                                (i as f32 * 0.125 - 5.).to_bits()
                            }
                        }
                    })
                    .collect();
                let input = gpu.upload("Partial-cell source", &values).unwrap();
                let plain = apply(&gpu, &input, &p, &mut Profile::default()).unwrap();
                let bytes = gpu.readback(&plain).unwrap();
                let expected: &[u32] = bytemuck::cast_slice(&bytes);
                let logical = p.logical_extent();
                for y in 0..logical[1] {
                    for x in 0..logical[0] {
                        for c in 0..components {
                            let mut count = 0;
                            let mut total = 0_f64;
                            let mut category = 0;
                            for sy in y * divisor
                                ..extent[1].min(
                                    (u64::from(y) + 1)
                                        .saturating_mul(u64::from(divisor))
                                        .min(u64::from(u32::MAX))
                                        as u32,
                                )
                            {
                                for sx in x * divisor
                                    ..extent[0].min(
                                        (u64::from(x) + 1)
                                            .saturating_mul(u64::from(divisor))
                                            .min(u64::from(u32::MAX))
                                            as u32,
                                    )
                                {
                                    let word =
                                        values[((sy * extent[0] + sx) as usize) * components + c];
                                    count += 1;
                                    match signal {
                                        Signal::Mask => category = category.max(word),
                                        Signal::Flags => category |= word,
                                        _ => total += f64::from(f32::from_bits(word)),
                                    }
                                }
                            }
                            let actual = expected[((y * logical[0] + x) as usize) * components + c];
                            match signal {
                                Signal::Mask | Signal::Flags => assert_eq!(actual, category),
                                _ if divisor == 1 => assert_eq!(
                                    actual,
                                    values[((y * extent[0] + x) as usize) * components + c]
                                ),
                                _ => assert!(
                                    (f64::from(f32::from_bits(actual)) - total / f64::from(count))
                                        .abs()
                                        <= 0.00001
                                ),
                            }
                        }
                    }
                }
                for halo in [3, 16] {
                    let output = logical.map(|v| (v + halo * 2).div_ceil(8) * 8);
                    let padded = p.with_extent(output).unwrap().with_origin([halo; 2]);
                    let result = apply(&gpu, &input, &padded, &mut Profile::default()).unwrap();
                    let bytes = gpu.readback(&result).unwrap();
                    let actual: &[u32] = bytemuck::cast_slice(&bytes);
                    for y in 0..output[1] {
                        for x in 0..output[0] {
                            for c in 0..components {
                                let sx = x.saturating_sub(halo).min(logical[0] - 1);
                                let sy = y.saturating_sub(halo).min(logical[1] - 1);
                                assert_eq!(
                                    actual[((y * output[0] + x) as usize) * components + c],
                                    expected[((sy * logical[0] + sx) as usize) * components + c]
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn pooling_maps_each_component_frame_with_its_own_origin() {
    let prepared = Prepared::new([33, 35], 4, Signal::Rgb)
        .unwrap()
        .with_extent([16, 16])
        .unwrap()
        .with_origin([2, 3]);
    let view = crate::image::View {
        crop: [3, 5, 31, 35],
        rotation: 90,
        extent: [30, 28],
    };
    assert_eq!(
        prepared.map_view(view),
        crate::image::View {
            crop: [2, 4, 10, 12],
            rotation: 90,
            extent: [8, 7]
        }
    );
}
