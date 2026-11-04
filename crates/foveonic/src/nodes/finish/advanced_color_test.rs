//! Verifies bounded saved ranges, transient preview policy and guide-selected GPU effects.

use super::*;

#[test]
fn stored_ranges_are_compact_bounded_and_validated() {
    let mut controls = Controls::default();
    controls.push(Range {
        hue: 230.,
        adjustment: Adjustment {
            lightness: 0.5,
            ..Default::default()
        },
        ..Default::default()
    });
    let json = serde_json::to_string(&controls).unwrap();
    assert!(json.len() < 300);
    assert_eq!(serde_json::from_str::<Controls>(&json).unwrap(), controls);
    let too_many = serde_json::to_string(&vec![Range::default(); 31]).unwrap();
    assert!(serde_json::from_str::<Controls>(&too_many).is_err());
    controls.ranges[0].saturation = [0.9, 0.2];
    assert!(controls.validate().is_err());
    controls.ranges[0] = Range {
        hue: f32::NAN,
        ..Default::default()
    };
    assert!(controls.validate().is_err());
}

#[test]
fn ranges_follow_section_scopes_and_preview_is_not_authored() {
    use crate::render::{EditScope, EditSection, Settings};
    let mut settings = Settings::default();
    settings.photographic.color_editor.advanced.push(Range {
        name: "Sky".try_into().unwrap(),
        adjustment: Adjustment {
            lightness: 0.5,
            ..Default::default()
        },
        ..Default::default()
    });
    settings.color_range_preview = Some(0);
    let json = serde_json::to_string(&settings).unwrap();
    let restored: Settings = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.color_range_preview, None);
    assert_eq!(
        restored.photographic.color_editor,
        settings.photographic.color_editor
    );
    assert_eq!(settings.photographic_recipe().color_range_preview, None);
    assert!(settings.color_editor_active());
    assert!(settings.color_editor_chromatic_active());
    let mut target = Settings::default();
    EditScope::Tool(EditSection::ColorEditor).apply(
        &settings,
        &mut target,
        crate::source::ProcessingFamily::Merrill,
    );
    assert_eq!(
        target.photographic.color_editor,
        settings.photographic.color_editor
    );
    assert_eq!(target.color_range_preview, None);
    target.color_editor_enabled = false;
    assert!(!target.color_editor_active());
    assert!(!target.advanced_color_active());
    assert_eq!(settings.dng_settings().color_range_preview, None);
}

fn run(
    gpu: &Gpu,
    source: &[f32],
    guide: &[f32],
    ranges: &[Range],
    mono: bool,
    preview: Option<usize>,
) -> Vec<f32> {
    let input = gpu.upload("Advanced input", source).unwrap();
    let guide = gpu.upload("Advanced guide", guide).unwrap();
    let extent = [(source.len() / 3) as u32, 1];
    let mut controls = Controls::default();
    for range in ranges {
        controls.push(*range);
    }
    let mut params = Params::new(
        extent,
        controls,
        ColorSpace::Srgb,
        1.,
        1.,
        0.,
        mono,
        preview,
    );
    params.guide_matrix = [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]];
    let output = apply(gpu, &input, &guide, params, extent, &mut Profile::default()).unwrap();
    assert_eq!(
        gpu.readback(&input).unwrap(),
        bytemuck::cast_slice::<f32, u8>(source)
    );
    bytemuck::cast_slice::<u8, f32>(&gpu.readback(&output).unwrap()).to_vec()
}

#[test]
#[ignore = "requires a GPU"]
fn advanced_color_selects_guide_protects_neutrals_and_combines_lifts() {
    let gpu = Gpu::new().unwrap();
    let range = Range {
        adjustment: Adjustment {
            lightness: 0.5,
            ..Default::default()
        },
        ..Default::default()
    };
    let pixels = [
        0.125, 0.125, 0.125, 0.4, 0.2, 0.3, 0.2, 0.3, 0.5, 0., 0., 0.,
    ];
    let guide = [0., 0., 1., 1., 0., 0., 0.4, 0.4, 0.4, 0., 0., 1.];
    let once = run(&gpu, &pixels, &guide, &[range], false, None);
    let twice = run(&gpu, &pixels, &guide, &[range, range], false, None);
    assert!(twice[0] > once[0], "overlapping lifts must combine");
    assert_eq!(
        &once[3..9],
        &pixels[3..9],
        "unselected and neutral guide samples must be bit exact"
    );
    assert!(once[9..].iter().all(|v| v.abs() < 0.00001));
    let muted = run(
        &gpu,
        &pixels,
        &guide,
        &[Range {
            enabled: false,
            ..range
        }],
        false,
        None,
    );
    assert_eq!(muted, pixels);
    let mono = run(&gpu, &pixels, &guide, &[range], true, None);
    assert!(mono[..3].iter().all(|v| (*v - 0.25).abs() < 0.000001));
    assert_eq!(mono[0], mono[1]);
    assert_eq!(mono[1], mono[2]);
    assert_eq!(&mono[9..], &[0.; 3]);
}

#[test]
#[ignore = "requires a GPU"]
fn advanced_color_wraps_hue_and_preview_preserves_lightness() {
    let gpu = Gpu::new().unwrap();
    let range = Range {
        hue: 0.,
        width: 40.,
        saturation: [0.2, 0.8],
        smoothness: 0.2,
        adjustment: Adjustment {
            lightness: 0.5,
            ..Default::default()
        },
        ..Default::default()
    };
    let source = [0.1, 0.2, 0.3, 0.1, 0.2, 0.3, 0.1, 0.2, 0.3];
    let guide = [1., 0.5, 0.6, 1., 0.6, 0.5, 0., 1., 0.];
    let result = run(&gpu, &source, &guide, &[range], false, None);
    assert_eq!(&result[..3], &result[3..6]);
    assert_eq!(&result[6..], &source[6..]);
    let range = Range {
        adjustment: Adjustment::default(),
        ..range
    };
    let preview = run(&gpu, &source, &guide, &[range], false, Some(0));
    assert!((preview[6] - preview[7]).abs() < 0.00001 && (preview[7] - preview[8]).abs() < 0.00001);
    let lab = |rgb: &[f32]| {
        let rgb: Vec<f64> = rgb.iter().copied().map(f64::from).collect();
        let l = (0.4122214708 * rgb[0] + 0.5363325363 * rgb[1] + 0.0514459929 * rgb[2]).cbrt();
        let m = (0.2119034982 * rgb[0] + 0.6806995451 * rgb[1] + 0.1073969566 * rgb[2]).cbrt();
        let s = (0.0883024619 * rgb[0] + 0.2817188376 * rgb[1] + 0.6299787005 * rgb[2]).cbrt();
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s
    };
    assert!((lab(&preview[6..]) - lab(&source[6..])).abs() < 0.00001);
}

#[test]
#[ignore = "requires a GPU"]
fn advanced_color_lightness_retains_chromatic_components_in_working_spaces() {
    let gpu = Gpu::new().unwrap();
    let lab = |rgb: [f32; 3]| {
        let [r, g, b] = rgb.map(f64::from);
        let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
        let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
        let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
        [
            0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
            1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
            0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
        ]
    };
    let transform = |rgb: [f32; 3], rows: [[f32; 4]; 3]| {
        rows.map(|row| (0..3).map(|c| rgb[c] * row[c]).sum::<f32>())
    };
    let rgb = [0.04, 0.1, 0.2];
    let before = lab(rgb);
    for space in [
        ColorSpace::Srgb,
        ColorSpace::AdobeRgb,
        ColorSpace::ProPhotoRgb,
    ] {
        for (white, exposure) in [(1., 0.), (4095., 2.), (65535., -2.)] {
            let mut controls = Controls::default();
            controls.push(Range {
                width: 360.,
                adjustment: Adjustment {
                    lightness: 0.5,
                    ..Default::default()
                },
                ..Default::default()
            });
            let mut params = Params::new([1, 1], controls, space, white, 1., exposure, false, None);
            params.guide_matrix = [[1., 0., 0., 0.], [0., 1., 0., 0.], [0., 0., 1., 0.]];
            let values = transform(rgb, params.from_srgb).map(|v| v * params.normalization[1]);
            let input = gpu.upload("Colored lightness", &values).unwrap();
            let guide = gpu.upload("Blue selection", &[0_f32, 0., 1.]).unwrap();
            let output = apply(
                &gpu,
                &input,
                &guide,
                params,
                [1, 1],
                &mut Profile::default(),
            )
            .unwrap();
            let bytes = gpu.readback(&output).unwrap();
            let pixels: &[f32; 3] = bytemuck::from_bytes(&bytes);
            let after = lab(transform(
                pixels.map(|v| v * params.normalization[0]),
                params.to_srgb,
            ));
            for (actual, expected) in
                after
                    .into_iter()
                    .zip([before[0] * 2_f64.powf(1. / 3.), before[1], before[2]])
            {
                assert!(
                    (actual - expected).abs() < 0.0001,
                    "{space:?}: {actual} != {expected}"
                );
            }
        }
    }
}

#[test]
fn names_round_trip_with_ranges_without_changing_gpu_parameters() {
    let mut controls = Controls::default();
    controls.push(Range::default());
    let parameters =
        |controls| Params::new([1, 1], controls, ColorSpace::Srgb, 1., 1., 0., false, None).key();
    let before = parameters(controls);
    controls.ranges[0].name = "Sky".try_into().unwrap();
    assert!(
        parameters(controls) == before,
        "names must not invalidate pixel parameters"
    );
    let restored: Controls =
        serde_json::from_str(&serde_json::to_string(&controls).unwrap()).unwrap();
    assert_eq!(restored.ranges[0].name.as_str(), "Sky");
    controls.push(Range {
        name: "Leaves".try_into().unwrap(),
        ..Default::default()
    });
    controls.remove(0);
    assert_eq!(controls.ranges[0].name.as_str(), "Leaves");
    let unicode = serde_json::json!([{ "name": "🌿".repeat(MAX_NAME_CHARS) }]);
    assert!(serde_json::from_value::<Controls>(unicode).is_ok());
    let too_long = serde_json::json!([{ "name": "a".repeat(MAX_NAME_CHARS + 1) }]);
    assert!(serde_json::from_value::<Controls>(too_long).is_err());
}
