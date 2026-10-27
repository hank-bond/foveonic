//! Checks that DNG encoding preserves channel ratios, rounds predictably, and rejects invalid inputs.

use super::*;
use crate::{
    dng::appearance::Matrix,
    image::{ColorSpace, Description, View},
    render::Settings,
    source::Source,
};
#[test]
#[ignore = "requires a GPU and FOVEONIC_FIXTURE_DIR"]
fn camera_encoding_uses_one_linear_range_and_rejects_invalid_samples() {
    let engine = Engine::new().unwrap();
    let source = Source::open(
        std::path::PathBuf::from(std::env::var_os("FOVEONIC_FIXTURE_DIR").unwrap())
            .join("blue-parking-sign.x3f"),
    )
    .unwrap();
    let settings = Settings::new("Sunlight", "Standard");
    let mut prepared = Prepared::new(&source.metadata, &settings).unwrap();
    prepared.mapping = Matrix::diagonal([1. / 4095.; 3]);
    prepared.neutral = [0.5, 1., 0.25];
    let image = |values: &[f32]| RenderedImage {
        buffer: engine.gpu.upload("DNG encoding fixture", values).unwrap(),
        description: Description {
            linear_white: 4095,
            view_baked: false,
            extent: [2, 1],
            layout: Layout::Rgb32Float,
            space: Space::LinearRgb(ColorSpace::Srgb),
        },
        view: View {
            crop: [0, 0, 2, 1],
            rotation: 0,
            extent: [2, 1],
        },
        metadata: source.metadata.clone(),
        settings: settings.clone(),
        owner: engine.owner.clone(),
    };
    let base = image(&[-4095., 2047.5, 8190., 4095., 4095., 4095.]);
    let (packed, scale) = engine
        .encode_dng_base(&base, &prepared, &mut Profile::default())
        .unwrap();
    assert_eq!(scale, 8_f32 * 1.001);
    let actual: Vec<u16> = packed
        .as_chunks::<8>()
        .0
        .iter()
        .flat_map(|p| [0, 2, 4].map(|i| u16::from_le_bytes([p[i], p[i + 1]])))
        .collect();
    let expected: Vec<u16> = [0., 0.5, 2., 1., 1., 1.]
        .map(|v| (v / scale * 65535. + 0.5).floor() as u16)
        .to_vec();
    assert_eq!(actual, expected);
    let black = image(&[0.; 6]);
    let (pixels, scale) = engine
        .encode_dng_base(&black, &prepared, &mut Profile::default())
        .unwrap();
    assert_eq!(scale, 1.001);
    assert!(pixels.iter().all(|v| *v == 0));
    let invalid = image(&[f32::NAN, 0., 0., 0., 0., 0.]);
    assert!(engine
        .encode_dng_base(&invalid, &prepared, &mut Profile::default())
        .is_err());
    let other = Engine::new().unwrap();
    assert_eq!(
        other
            .encode_dng_base(&base, &prepared, &mut Profile::default())
            .unwrap_err()
            .to_string(),
        "image belongs to another engine"
    );
}
