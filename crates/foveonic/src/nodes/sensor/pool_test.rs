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
    for bounds in [[2, 0, 2, 3], [0, 2, 3, 1], [0, 0, 6, 4], [0, 0, 4, 6]] {
        assert!(Prepared::rgb_region([5, 5], bounds, 2).is_err());
    }
}

#[test]
#[ignore = "requires GPU"]
fn pooling_preserves_means_coverage_and_source() {
    let gpu = Gpu::test_executor();
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
    let rgb = |v: f32| [v, -v, 2. * v];
    let pixels: Vec<_> = (0..6)
        .flat_map(|y| {
            (0..8).map(move |x| {
                if (1..6).contains(&x) && (1..4).contains(&y) {
                    rgb((10 * y + x) as f32)
                } else {
                    [f32::NAN; 3]
                }
            })
        })
        .collect();
    let input = gpu.upload("Offset valid RGB rectangle", &pixels).unwrap();
    let bounded = Prepared::rgb_region([8, 6], [1, 1, 6, 4], 2).unwrap();
    let output = apply(&gpu, &input, &bounded, &mut Profile::default()).unwrap();
    let bytes = gpu.readback(&output).unwrap();
    assert_eq!(
        bytemuck::cast_slice::<u8, [f32; 3]>(&bytes),
        [16.5, 18.5, 20., 31.5, 33.5, 35.].map(rgb)
    );
}

#[test]
#[ignore = "requires GPU"]
fn partial_cells_and_halos_replicate_completed_means_and_flag_unions() {
    let gpu = Gpu::test_executor();
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
#[ignore = "requires a GPU"]
fn full_quattro_extreme_partial_cell_is_replicated_into_halo() {
    let gpu = Gpu::test_executor();
    let extent = [2944, 1836];
    for split in [false, true] {
        let source: Vec<f32> = (0..extent[1])
            .flat_map(|y| {
                std::iter::repeat_n(
                    if split && y >= extent[1] / 2 { 3. } else { 1. },
                    extent[0] as usize,
                )
            })
            .collect();
        let input = gpu.upload("Extreme partial-cell source", &source).unwrap();
        let prepared = Prepared::new(extent, 1 << 31, Signal::Blue).unwrap();
        let plain = apply(&gpu, &input, &prepared, &mut Profile::default()).unwrap();
        let plain_bytes = gpu.readback(&plain).unwrap();
        let plain_value = bytemuck::cast_slice::<u8, f32>(&plain_bytes)[0];
        let expected = if split { 2. } else { 1. };
        assert!(
            (plain_value - expected).abs() < 0.000001,
            "split={split}, mean={plain_value}, expected={expected}"
        );
        let padded = prepared.with_extent([3, 3]).unwrap().with_origin([1, 1]);
        let halo = apply(&gpu, &input, &padded, &mut Profile::default()).unwrap();
        let halo_bytes = gpu.readback(&halo).unwrap();
        for &value in bytemuck::cast_slice::<u8, f32>(&halo_bytes) {
            assert_eq!(value.to_bits(), plain_value.to_bits());
        }
        assert_eq!(
            gpu.readback(&input).unwrap(),
            bytemuck::cast_slice::<f32, u8>(&source)
        );
    }
}

#[test]
#[ignore = "requires GPU"]
fn full_quattro_rgb_extreme_cell_preserves_balanced_mean_and_halo() {
    let gpu = Gpu::test_executor();
    let extent = [2944, 1836];
    let source: Vec<f32> = (0..extent[1])
        .flat_map(|y| {
            let value = if y < extent[1] / 2 { 1. } else { 3. };
            std::iter::repeat_n([value, 2. * value, 3. * value], extent[0] as usize).flatten()
        })
        .collect();
    let input = gpu.upload("Extreme RGB pooling source", &source).unwrap();
    let prepared = Prepared::new(extent, 1 << 31, Signal::Rgb)
        .unwrap()
        .with_extent([136, 136])
        .unwrap()
        .with_origin([64, 64]);
    let output = apply(&gpu, &input, &prepared, &mut Profile::default()).unwrap();
    let bytes = gpu.readback(&output).unwrap();
    let values: &[[f32; 3]] = bytemuck::cast_slice(&bytes);
    for value in values {
        for (actual, expected) in value.iter().zip([2., 4., 6.]) {
            assert!((actual - expected).abs() < 0.000001, "{value:?}");
        }
        assert_eq!(value.map(f32::to_bits), values[0].map(f32::to_bits));
    }
    assert_eq!(
        gpu.readback(&input).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&source)
    );
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
    let bounded = Prepared::rgb_region([33, 35], view.crop, 4).unwrap();
    assert_eq!(
        bounded.map_view(view),
        crate::image::View {
            crop: [0, 0, 7, 8],
            rotation: 90,
            extent: [8, 7]
        }
    );
}

#[test]
fn area_noise_and_signed_rounding_follow_the_working_grid() {
    use crate::render::Scale;
    for divisor in [1, 2, 4, 8, 16, 64, 1 << 31] {
        assert_eq!(
            independent_noise_std(Scale::new(divisor).unwrap()),
            1. / divisor as f32
        );
    }
    let float = Prepared::new([8, 8], 2, Signal::Rgb).unwrap();
    let signed = float.with_signed_codes();
    assert_ne!(float.key(), signed.key());
    assert_eq!(float.output_extent(), signed.output_extent());
}

#[test]
#[ignore = "requires GPU"]
fn signed_area_means_round_once_without_negative_bias() {
    let gpu = Gpu::test_executor();
    for extent in [[8, 8], [9, 7]] {
        let pixels: Vec<f32> = (0..extent[1])
            .flat_map(|y| {
                (0..extent[0]).flat_map(move |x| {
                    let v = if x % 2 == 0 && y % 2 == 0 { 1. } else { 0. };
                    [v, -2. * v, -32768.]
                })
            })
            .collect();
        let input = gpu.upload("Signed area fixture", &pixels).unwrap();
        let prepared = Prepared::new(extent, 2, Signal::Rgb)
            .unwrap()
            .with_signed_codes();
        let output = apply(&gpu, &input, &prepared, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        let [w, h] = prepared.output_extent();
        for y in 0..h {
            for x in 0..w {
                let count = (extent[0] - x * 2).min(2) * (extent[1] - y * 2).min(2);
                let expected = [1. / count as f32, -2. / count as f32, -32768.].map(f32::round);
                let i = ((y * w + x) * 3) as usize;
                assert_eq!(&actual[i..i + 3], expected);
            }
        }
    }
}

#[test]
#[ignore = "requires GPU"]
fn a_sensor_spike_reaches_one_logical_cell_with_one_dispatch() {
    let gpu = Gpu::test_executor();
    let extent = [17_u32, 13];
    for point in [[0, 0], [7, 5], [16, 12]] {
        let mut pixels = vec![100_f32; (extent[0] * extent[1] * 3) as usize];
        pixels[((point[1] * extent[0] + point[0]) * 3) as usize] += 4000.;
        let input = gpu.upload("Localized sensor spike", &pixels).unwrap();
        for divisor in [2, 4, 8, 64] {
            let base = Prepared::new(extent, divisor, Signal::Rgb).unwrap();
            let logical = base.logical_extent();
            let p = base
                .with_extent(logical.map(|v| v + 6))
                .unwrap()
                .with_origin([3, 3]);
            let before = gpu.activity();
            let output = apply(&gpu, &input, &p, &mut Profile::default()).unwrap();
            let activity = gpu.activity().since(before);
            assert_eq!(activity.dispatches, 1);
            assert_eq!(
                activity.storage_bytes,
                output.size(),
                "no intermediate image allocation"
            );
            let bytes = gpu.readback(&output).unwrap();
            let actual: &[f32] = bytemuck::cast_slice(&bytes);
            let [w, h] = p.output_extent();
            let cell = point.map(|v| v / divisor);
            let count = (extent[0] - cell[0] * divisor).min(divisor)
                * (extent[1] - cell[1] * divisor).min(divisor);
            let mut changed = 0;
            for y in 0..h {
                for x in 0..w {
                    let logical_xy = [
                        x.saturating_sub(3).min(logical[0] - 1),
                        y.saturating_sub(3).min(logical[1] - 1),
                    ];
                    let i = ((y * w + x) * 3) as usize;
                    let expected = if logical_xy == cell {
                        100. + 4000. / count as f32
                    } else {
                        100.
                    };
                    assert!((actual[i] - expected).abs() < 0.001);
                    assert_eq!(&actual[i + 1..i + 3], [100., 100.]);
                    if x >= 3
                        && x < 3 + logical[0]
                        && y >= 3
                        && y < 3 + logical[1]
                        && actual[i] != 100.
                    {
                        changed += 1;
                    }
                }
            }
            assert_eq!(changed, 1);
        }
        assert_eq!(
            gpu.readback(&input).unwrap(),
            bytemuck::cast_slice::<f32, u8>(&pixels)
        );
    }
}
