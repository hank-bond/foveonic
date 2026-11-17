//! Compares streamed linear reduction with materialized pixels and profiles transient allocations.

use super::*;

fn preview(extent: [u32; 2], color_space: TiffColorSpace) -> Preview {
    Preview {
        rgb: (0..extent[0] as usize * extent[1] as usize * 3)
            .map(|i| (i as u16).wrapping_mul(7919))
            .collect(),
        extent,
        native_extent: extent,
        color_space,
        rotation: 0,
    }
}

#[test]
fn streamed_reduction_matches_materialized_linear_samples_exactly() {
    for space in TiffColorSpace::ALL {
        for extent in [[1, 1], [601, 397], [397, 601]] {
            let preview = preview(extent, space);
            let native = preview.linear_image().unwrap();
            for side in [1, 7, 256, 1024] {
                let actual = preview.linear_resized(side).unwrap();
                let expected = resize_linear(&native, [actual.width(), actual.height()]);
                assert_eq!(
                    actual.as_raw(),
                    expected.as_raw(),
                    "{space:?} {extent:?} {side}"
                );
                let encoded = preview.clone().resized(side).unwrap();
                assert_eq!(
                    encoded.rgb,
                    expected
                        .as_raw()
                        .iter()
                        .map(|v| encode_sample(space, *v))
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}

#[test]
fn streamed_reduction_rejects_invalid_geometry_before_sampling() {
    let mut input = preview([2, 1], TiffColorSpace::Srgb);
    assert!(input.linear_resized(0).is_err());
    input.rgb.pop();
    assert!(input.linear_resized(256).is_err());
    input.extent = [0, 1];
    assert!(input.linear_resized(256).is_err());
    input.extent = [u32::MAX; 2];
    assert!(input.linear_resized(256).is_err());
}

#[test]
#[ignore = "explicit allocation probe, FOVEONIC_PREVIEW_ALLOCATION_PATH=materialized or streamed"]
fn profile_preview_reduction_allocations() {
    let input = preview([4096, 3072], TiffColorSpace::Srgb);
    let mode =
        std::env::var("FOVEONIC_PREVIEW_ALLOCATION_PATH").unwrap_or_else(|_| "streamed".into());
    let began = std::time::Instant::now();
    let image = match mode.as_str() {
        "streamed" => input.linear_resized(2048).unwrap(),
        "materialized" => resize_linear(&input.linear_image().unwrap(), [2048, 1536]),
        _ => panic!("unknown preview allocation probe mode"),
    };
    println!(
        "{mode}: {}x{}, {:.1} ms, crc32={:08x}",
        image.width(),
        image.height(),
        began.elapsed().as_secs_f64() * 1000.,
        crc32fast::hash(bytemuck::cast_slice(image.as_raw()))
    );
    std::hint::black_box(image);
}
