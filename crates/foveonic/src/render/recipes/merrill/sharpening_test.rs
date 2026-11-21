use super::applied_sharpen_amount;
use crate::{
    render::{Scale, Settings},
    source::Metadata,
};

fn metadata(resolution: Option<&str>) -> Metadata {
    let mut properties: std::collections::BTreeMap<String, String> =
        [("CAMMODEL".into(), "SIGMA DP2 Merrill".into())].into();
    if let Some(resolution) = resolution {
        properties.insert("RESOLUTION".into(), resolution.into());
    }
    Metadata {
        processing_version: None,
        width: 4928,
        height: 3264,
        rotation: 0,
        properties,
        matrices: Default::default(),
        lists: Default::default(),
    }
}

#[test]
fn reduced_raw_resolutions_and_working_sizes_sharpen_less() {
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.sharpen_amount = Some(1.);
    let applied = |settings: &Settings, resolution| {
        applied_sharpen_amount(settings, &metadata(resolution)).unwrap()
    };
    assert_eq!(applied(&settings, None), 1.);
    assert_eq!(applied(&settings, Some("HI")), 1.);
    assert!((applied(&settings, Some("MED")) - 2_f32.powf(-0.25)).abs() < 1e-6);
    assert!((applied(&settings, Some("LOW")) - 2_f32.powf(-0.5)).abs() < 1e-6);
    settings.working_scale = Scale::new(4).unwrap();
    assert_eq!(
        applied(&settings, None),
        0.5,
        "a quarter-size render sharpens by half"
    );
    settings.working_scale = Scale::NATIVE;
    settings.output_size = Some([100, 66]);
    assert_eq!(
        applied(&settings, None),
        1.,
        "export size is not a working size"
    );
}
