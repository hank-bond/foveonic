//! Checks that family control validation and saved-setting normalization agree.

use super::*;

fn capabilities(model: &str) -> ControlCapabilities {
    ControlCapabilities::from_metadata(&Metadata {
        processing_version: None,
        width: 1,
        height: 1,
        rotation: 0,
        properties: [("CAMMODEL".into(), model.into())].into(),
        matrices: Default::default(),
        lists: Default::default(),
    })
    .unwrap()
}

#[test]
fn classic_models_share_supported_controls_and_preserve_effect_selections() {
    for model in [
        "SIGMA SD9",
        "SIGMA SD10",
        "SIGMA SD14",
        "SIGMA SD15",
        "SIGMA DP1",
        "SIGMA DP1S",
        "SIGMA DP1X",
        "SIGMA DP2",
        "SIGMA DP2S",
        "SIGMA DP2X",
    ] {
        let capabilities = capabilities(model);
        assert_eq!(capabilities.family, ProcessingFamily::Classic);
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.rendering = Rendering::Monochrome;
        settings.row_correction = false;
        settings.highlight_restoration = false;
        settings.highlight_threshold_ev = Some(1.);
        settings.color_dq_gain = Some(2.);
        settings.exposure = 0.7;
        assert!(capabilities.validate(&settings).is_err());
        assert!(capabilities.normalize(&mut settings));
        assert!(capabilities.validate(&settings).is_ok());
        assert!(!capabilities.normalize(&mut settings));
        assert!(!settings.highlight_restoration);
        assert!(settings.reconstruction.is_none());
        assert_eq!(settings.exposure, 0.7);
    }
}

#[test]
fn classic_resets_noise_adjustments_without_disabling_reconstruction() {
    let capabilities = capabilities("SIGMA SD14");
    let mut settings = Settings::new("Sunlight", "Standard");
    let noise = crate::noise::Controls {
        chroma: 4,
        ..Default::default()
    };
    settings.reconstruction = Some(noise);
    assert!(capabilities.validate(&settings).is_err());
    assert!(capabilities.normalize(&mut settings));
    assert_eq!(settings.reconstruction, Some(Default::default()));
    assert!(capabilities.validate(&settings).is_ok());
}

#[test]
fn merrill_retains_its_supported_adjustments() {
    let capabilities = capabilities("SIGMA DP2 Merrill");
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.rendering = Rendering::Monochrome;
    settings.row_correction = false;
    settings.highlight_threshold_ev = Some(1.);
    settings.color_dq_gain = Some(2.);
    assert!(capabilities.monochrome && capabilities.noise_adjustments);
    assert!(!capabilities.normalize(&mut settings));
    assert!(capabilities.validate(&settings).is_ok());
}

#[test]
fn every_unsupported_classic_adjustment_is_rejected_and_normalized() {
    let capabilities = capabilities("SIGMA SD14");
    let base = Settings::new("Sunlight", "Standard");
    let mut requests = vec![base.clone(); 7];
    requests[0].row_correction = false;
    requests[1].highlight_threshold_ev = Some(1.);
    requests[2].color_dq_gain = Some(1.);
    requests[3].color_field.selectivity = 0.5;
    requests[4].highlight_mode = HighlightMode::ColorField;
    requests[5].repair = Some(RepairThreshold::Manual(1));
    let noise = crate::noise::Controls {
        chroma: 4,
        ..Default::default()
    };
    requests[6].reconstruction = Some(noise);
    for mut settings in requests {
        assert!(capabilities.validate(&settings).is_err(), "{settings:?}");
        assert!(capabilities.normalize(&mut settings));
        assert!(capabilities.validate(&settings).is_ok());
    }
}

#[test]
fn classic_monochrome_preserves_color_preferences_and_effect_selections() {
    let capabilities = capabilities("SIGMA DP2");
    let mut settings = Settings::new("Shade", "Vivid");
    assert!(capabilities.white_balance_active(&settings));
    assert!(capabilities.highlights_active(&settings));
    settings.rendering = Rendering::Monochrome;
    let before = settings.clone();
    assert!(capabilities.monochrome);
    assert!(!capabilities.white_balance_active(&settings));
    assert!(capabilities.color_mode_active(&settings));
    assert!(!capabilities.highlights_active(&settings));
    assert!(!capabilities.normalize(&mut settings));
    assert!(capabilities.validate(&settings).is_ok());
    assert_eq!(settings, before);
}

#[test]
fn classic_normalizes_saved_merrill_monochrome_adjustments() {
    let capabilities = capabilities("SIGMA DP2");
    for adjustment in 0..3 {
        let mut settings = Settings::new("Shade", "Vivid");
        settings.rendering = Rendering::Monochrome;
        match adjustment {
            0 => settings.monochrome.mix = [0., 0., 1.],
            1 => settings.monochrome.noise = 4,
            _ => settings.monochrome.banding = 0,
        }
        let saved = crate::sidecar::encode(&settings).unwrap();
        let mut restored = crate::sidecar::decode(&saved).unwrap();
        assert!(capabilities.validate(&restored).is_err());
        assert!(capabilities.normalize(&mut restored));
        let mut expected = settings;
        expected.monochrome = Default::default();
        assert_eq!(restored, expected);
        assert!(capabilities.validate(&restored).is_ok());
        assert!(!capabilities.normalize(&mut restored));
    }
}
