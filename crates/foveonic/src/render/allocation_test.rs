//! Checks audited allocation eligibility, arithmetic limits and resolved output selection.

use super::*;
use crate::{color_space::ColorSpace, nodes::sensor::calibration::ColorParams, noise::Parameters};
use bytemuck::Zeroable;

fn parameters() -> Parameters {
    let mut params: Parameters = bytemuck::Zeroable::zeroed();
    params.source = [128, 96, 2, 3];
    params.tile = [80, 48, 0, 0];
    params.window = [0, 0, 2, 3];
    params.region = [0, 0, 128, 96];
    params
}

#[test]
fn unsupported_outputs_modes_and_geometry_do_not_become_zero_estimates() {
    let tone = Operation::Tone {
        curve: vec![0_f32; 4096].into(),
        exposure: 1.,
        space: ColorSpace::Srgb,
    };
    assert!(estimate(&tone, &[true], Some([128, 96]), Mode::Off, false).is_some());
    for mode in [Mode::Wall, Mode::Gpu] {
        assert!(estimate(&tone, &[true], Some([128, 96]), mode, false).is_none());
    }
    assert!(estimate(&tone, &[true], Some([128, 96]), Mode::Off, true).is_none());
    assert!(estimate(&tone, &[true], None, Mode::Off, false).is_none());
    for required in [&[][..], &[false], &[true, false]] {
        assert!(estimate(&tone, required, Some([128, 96]), Mode::Off, false).is_none());
    }
    assert!(estimate(
        &Operation::NoiseBase(parameters()),
        &[true],
        None,
        Mode::Off,
        false
    )
    .is_none());
    assert_eq!(
        estimate(&Operation::LinearCodeBase, &[true], None, Mode::Off, false),
        Some(GpuAllocation::default()),
    );

    let make_color = |params| Operation::TiledColor {
        params,
        color: ColorParams::zeroed(),
        space: ColorSpace::Srgb,
    };
    assert!(estimate(
        &make_color(parameters()),
        &[true, false],
        None,
        Mode::Off,
        false
    )
    .is_some());
    for mask in [[true, true], [false, true], [false, false]] {
        assert!(estimate(&make_color(parameters()), &mask, None, Mode::Off, false).is_none());
    }
    let mut regional = parameters();
    regional.region[0] = 1;
    assert!(estimate(
        &make_color(regional),
        &[true, false],
        None,
        Mode::Off,
        false
    )
    .is_none());
}

#[test]
fn fresh_output_contracts_cover_atlas_halos_and_real_table_lengths() {
    let p = parameters();
    let atlas = u64::from(80_u32 * 48 * 2 * 3) * 12;
    let color = reconstruction::color_allocation(p).unwrap();
    assert_eq!(color.output_bytes, atlas);
    assert_ne!(color.output_bytes, 128 * 96 * 12);
    let projection = reconstruction::project_color_allocation(p).unwrap();
    assert_eq!(projection.output_bytes, 128 * 96 * 12);
    for length in [1, 512 * 3, 8192 * 3] {
        let table = crate::nodes::color::color_dq::Table {
            period: 512,
            values: vec![0; length],
        };
        let bound = reconstruction::color_dq_allocation(p, &table).unwrap();
        assert_eq!(bound.output_bytes, atlas);
        assert_eq!(bound.temporary_bytes, 16 + length as u64 * 4);
    }
}

#[test]
fn overflow_and_upload_padding_fail_closed() {
    assert_eq!(uploaded_bytes::<u8>(0), Some(4));
    assert_eq!(uploaded_bytes::<u8>(5), Some(8));
    assert_eq!(uploaded_bytes::<u32>(3), Some(12));
    assert_eq!(uploaded_bytes::<u64>(usize::MAX), None);
    assert_eq!(image_bytes([u32::MAX; 2], 3), None);
    assert_eq!(GpuAllocation::new(u64::MAX, 1), None);
    assert_eq!(
        GpuAllocation::new(u64::MAX - 1, 0)
            .unwrap()
            .checked_add(GpuAllocation::new(1, 1).unwrap()),
        None,
    );
    let mut p = parameters();
    p.source[2..].fill(u32::MAX);
    p.window[2..].fill(u32::MAX);
    assert!(reconstruction::color_allocation(p).is_none());
}
