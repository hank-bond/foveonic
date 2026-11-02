//! Checks scoped recipe isolation, neutral optional reconstruction and family-default precedence.

use super::*;
use crate::render::{MerrillNoiseMethod, PhotographicMethod, PreviewQuality, TiffColorSpace};

#[test]
fn directional_banding_follows_only_merrill_noise_scope_and_reset() {
    let mut source = Settings::default();
    source.merrill_banding.enabled = true;
    source.merrill_banding.method = crate::render::MerrillBandingMethod::Directional;
    source.row_correction = false;
    source.merrill_banding.rows.bands[0].strength = 1.5;
    source.merrill_banding.columns.bands[1].size = 65;
    let mut target = Settings::default();
    Scope::Tool(Section::Detail).apply(&source, &mut target, ProcessingFamily::Merrill);
    assert_eq!(target.merrill_banding, Default::default());
    Scope::Tool(Section::Noise).apply(&source, &mut target, ProcessingFamily::Quattro);
    assert_eq!(target.merrill_banding, Default::default());
    Scope::Tool(Section::Noise).apply(&source, &mut target, ProcessingFamily::Merrill);
    assert_eq!(target.merrill_banding, source.merrill_banding);
    assert!(!target.row_correction);
    Scope::Tool(Section::Corrections).apply(
        &Settings::default(),
        &mut target,
        ProcessingFamily::Merrill,
    );
    assert!(!target.row_correction);
    Scope::Tool(Section::Noise).apply(&Settings::default(), &mut target, ProcessingFamily::Merrill);
    assert_eq!(target.merrill_banding, Default::default());
    assert!(target.row_correction);
    for family in [ProcessingFamily::Classic, ProcessingFamily::Quattro] {
        Scope::Tool(Section::Noise).apply(&source, &mut target, family);
        assert!(target.row_correction);
        Scope::Tool(Section::Corrections).apply(&source, &mut target, family);
        assert!(!target.row_correction);
        target.row_correction = true;
    }
}

#[test]
fn noise_recipe_copies_merrill_method_without_affecting_detail_or_other_families() {
    let source = Settings {
        merrill_noise_method: MerrillNoiseMethod::MultiscaleRowsAndColumns,
        ..Default::default()
    };
    let mut target = Settings {
        merrill_noise_method: MerrillNoiseMethod::MultiscaleRows,
        ..Default::default()
    };
    Scope::Tool(Section::Detail).apply(&source, &mut target, ProcessingFamily::Merrill);
    assert_eq!(
        target.merrill_noise_method,
        MerrillNoiseMethod::MultiscaleRows
    );
    Scope::Tool(Section::Noise).apply(&source, &mut target, ProcessingFamily::Quattro);
    assert_eq!(
        target.merrill_noise_method,
        MerrillNoiseMethod::MultiscaleRows
    );
    Scope::Tool(Section::Noise).apply(&source, &mut target, ProcessingFamily::Merrill);
    assert_eq!(
        target.merrill_noise_method,
        MerrillNoiseMethod::MultiscaleRowsAndColumns
    );
    assert!(target.reconstruction.is_none());
    Scope::Tool(Section::Noise).apply(&Settings::default(), &mut target, ProcessingFamily::Merrill);
    assert_eq!(target.merrill_noise_method, MerrillNoiseMethod::Reference);
}

#[test]
fn tool_recipes_do_not_overwrite_other_tools_or_output_preferences() {
    let mut source = Settings {
        exposure: 1.,
        ..Default::default()
    };
    source.photographic.tone_method = PhotographicMethod::Universal;
    source.photographic.saturation_method = crate::render::PhotographicMethod::Universal;
    source.photographic.perceptual.saturation = 0.3;
    source.photographic.perceptual.contrast = -0.4;
    source.photographic.brightness = 0.6;
    source.highlight_restoration = false;
    let mut target = Settings {
        preview_quality: PreviewQuality::Draft,
        preview_region: Some([0, 0, 20, 30]),
        output_size: Some([200, 300]),
        tiff_color_space: Some(TiffColorSpace::ProPhotoRgb),
        ..Default::default()
    };
    Scope::Tool(Section::Tone).apply(&source, &mut target, ProcessingFamily::Merrill);
    assert_eq!(target.exposure, 1.);
    assert_eq!(
        target.photographic.brightness,
        source.photographic.brightness
    );
    assert_eq!(target.photographic.perceptual, Default::default());
    Scope::Tool(Section::Saturation).apply(&source, &mut target, ProcessingFamily::Merrill);
    assert_eq!(
        target.photographic.perceptual,
        source.photographic.perceptual
    );
    assert_eq!(
        target.photographic.saturation_method,
        source.photographic.saturation_method
    );
    assert!(target.highlight_restoration);
    assert_eq!(
        target.photographic.tone_method,
        PhotographicMethod::Universal
    );
    Scope::All.apply(&source, &mut target, ProcessingFamily::Merrill);
    assert_eq!(target.preview_quality, PreviewQuality::Draft);
    assert_eq!(target.preview_region, Some([0, 0, 20, 30]));
    assert_eq!(target.output_size, Some([200, 300]));
    assert_eq!(target.tiff_color_space, Some(TiffColorSpace::ProPhotoRgb));
    assert!(!target.highlight_restoration);
}

#[test]
fn separate_noise_and_detail_preserve_each_others_saved_values_and_off_state() {
    let mut source = Settings::default();
    let mut controls = crate::noise::Controls {
        chroma: 4,
        ..Default::default()
    };
    controls.texture.fine = 1.5;
    source.reconstruction = Some(controls);
    let mut target = Settings::default();
    Scope::Tool(Section::Noise).apply(&source, &mut target, ProcessingFamily::Merrill);
    let noise = target.reconstruction.unwrap();
    assert_eq!(noise.chroma, 4);
    assert!(!noise.detail_enabled);
    assert_eq!(noise.texture.fine, 1.);
    Scope::Tool(Section::Detail).apply(&source, &mut target, ProcessingFamily::Merrill);
    assert_eq!(target.reconstruction.unwrap().chroma, 4);
    assert_eq!(target.reconstruction.unwrap().texture.fine, 1.5);
    Scope::Tool(Section::Noise).apply(&Settings::default(), &mut target, ProcessingFamily::Merrill);
    assert!(!target.reconstruction.unwrap().noise_enabled);
    assert_eq!(target.reconstruction.unwrap().texture.fine, 1.5);
    Scope::Tool(Section::Noise).apply(&source, &mut target, ProcessingFamily::Quattro);
    assert_eq!(target.reconstruction, source.reconstruction);
}

#[test]
fn family_defaults_overlay_capture_settings_without_changing_saved_inputs() {
    let mut defaults = Defaults::default();
    let camera = Settings::default();
    let mut selected = camera.clone();
    selected.photographic.tone_method = PhotographicMethod::Universal;
    selected.exposure = 0.5;
    defaults.set(
        ProcessingFamily::Merrill,
        Scope::Tool(Section::Tone),
        &selected,
    );
    let mut capture = camera.clone();
    capture.highlight_threshold_ev = Some(0.3);
    let applied = defaults.apply(ProcessingFamily::Merrill, &capture);
    assert_eq!(applied.exposure, 0.5);
    assert_eq!(applied.highlight_threshold_ev, Some(0.3));
    assert_eq!(defaults.apply(ProcessingFamily::Quattro, &capture), capture);
    assert_eq!(capture.exposure, 0.);
    defaults.set(ProcessingFamily::Merrill, Scope::All, &camera);
    assert_eq!(
        defaults.apply(ProcessingFamily::Merrill, &capture).exposure,
        0.
    );
}

#[test]
fn merrill_speckle_controls_belong_to_noise_and_leave_corrections_independent() {
    let mut source = Settings {
        despeckle: false,
        ..Settings::default()
    };
    source.merrill_speckles.method = crate::render::MerrillSpeckleMethod::Compact;
    source.merrill_speckles.threshold = 0.25;
    source.merrill_speckles.size = 5;
    let mut target = Settings::default();
    Scope::Tool(Section::Corrections).apply(&source, &mut target, ProcessingFamily::Merrill);
    assert!(target.despeckle);
    assert_eq!(target.merrill_speckles, Default::default());
    Scope::Tool(Section::Noise).apply(&source, &mut target, ProcessingFamily::Merrill);
    assert!(!target.despeckle);
    assert_eq!(target.merrill_speckles, source.merrill_speckles);
    let decoded: Settings = serde_json::from_value(serde_json::to_value(&target).unwrap()).unwrap();
    assert_eq!(decoded.merrill_speckles, target.merrill_speckles);
}

#[test]
fn shadow_neutralization_follows_merrill_noise_scope() {
    let mut source = Settings::default();
    source.neutralize_shadows.strength = 0.8;
    source.neutralize_shadows.color_tolerance = 0.15;
    let mut target = Settings::default();
    Scope::Tool(Section::Corrections).apply(&source, &mut target, ProcessingFamily::Merrill);
    assert!(target.neutralize_shadows.is_neutral());
    Scope::Tool(Section::Noise).apply(&source, &mut target, ProcessingFamily::Merrill);
    assert_eq!(target.neutralize_shadows, source.neutralize_shadows);
    let original = target.clone();
    target.processing_sections.noise = false;
    assert!(target
        .processing_settings(ProcessingFamily::Merrill)
        .neutralize_shadows
        .is_neutral());
    assert_eq!(target.neutralize_shadows, original.neutralize_shadows);
}
