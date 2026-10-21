//! Lists visual outputs and comparable operation inputs for the selected Classic route.

use super::route::Route;
use crate::{render::Settings, source::Metadata};

pub(crate) fn outputs(metadata: &Metadata, settings: &Settings) -> Vec<&'static str> {
    let route = Route::new(metadata, settings);
    let mut names = vec!["final", "sensor.layers", "calibration.layers"];
    if route.repair {
        names.push("repair.layers");
    }
    if route.red_detail().unwrap_or(false) {
        names.push("red-detail.layers");
    }
    if route.highlights().unwrap_or(false) {
        names.push("highlight.layers");
    }
    if route.despeckle {
        names.push("despeckle.layers");
    }
    if route.high_iso().unwrap_or(false) {
        names.push("reconstruction.codes");
    } else {
        if route.small_chroma().unwrap_or(false) {
            names.push("chroma.small-layers");
        }
        if route.large_chroma().unwrap_or(false) {
            names.push("chroma.layers");
        }
        names.extend(["color.codes", "color.corrected-layers"]);
        if route.logarithmic().unwrap_or(false) {
            names.push("reconstruction.codes");
        }
        if route.restore_gain().unwrap_or(false) {
            names.push("color.gained-codes");
        }
        if route.recursive_chroma().unwrap_or(false) {
            names.push("chroma.rgb-codes");
        }
    }
    names.extend(["root.rgb", "detail.signal", "tone.rgb"]);
    names
}

pub(crate) fn comparison(
    metadata: &Metadata,
    settings: &Settings,
    output: &str,
) -> Option<&'static str> {
    let names = outputs(metadata, settings);
    if !names.contains(&output) {
        return None;
    }
    let mut input = "calibration.layers";
    // Original sensor codes and calibrated codes have different meanings.
    for stage in [
        "repair.layers",
        "red-detail.layers",
        "highlight.layers",
        "despeckle.layers",
        "chroma.small-layers",
        "chroma.layers",
    ] {
        if names.contains(&stage) {
            if stage == output {
                return Some(input);
            }
            input = stage;
        }
    }
    match output {
        "color.corrected-layers" if names.contains(&output) => Some(input),
        "color.gained-codes" => Some("color.codes"),
        "reconstruction.codes" if names.contains(&"color.codes") => Some("color.codes"),
        "chroma.rgb-codes" if names.contains(&output) => {
            Some(if names.contains(&"reconstruction.codes") {
                "reconstruction.codes"
            } else {
                "color.codes"
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{Matrix, MatrixEncoding};

    fn metadata(iso: f64, blur: f64, fir_disabled: f64) -> Metadata {
        Metadata {
            width: 2304,
            height: 1531,
            rotation: 0,
            properties: [
                ("CAMMODEL".into(), "SIGMA SD10".into()),
                ("ISO".into(), iso.to_string()),
            ]
            .into(),
            lists: Default::default(),
            matrices: [
                ("ChromaBlur", blur),
                ("FIR_DisableStep", fir_disabled),
                ("RedSharpen", 48.),
            ]
            .into_iter()
            .map(|(name, value)| {
                (
                    name.into(),
                    Matrix {
                        encoding: MatrixEncoding::Float32,
                        representation: "native",
                        dimensions: vec![1],
                        values: vec![value],
                    },
                )
            })
            .collect(),
        }
    }

    #[test]
    fn catalogue_follows_iso_and_enabled_operations() {
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.reconstruction = Some(Default::default());
        let low = metadata(100., 128., 0.);
        let names = outputs(&low, &settings);
        assert!(names.contains(&"red-detail.layers"));
        assert!(names.contains(&"color.codes"));
        assert!(names.contains(&"chroma.rgb-codes"));
        assert!(!names.contains(&"noise.layers"));
        assert_eq!(
            comparison(&low, &settings, "chroma.rgb-codes"),
            Some("reconstruction.codes")
        );
        let disabled = metadata(100., 128., 1.);
        assert_eq!(
            comparison(&disabled, &settings, "chroma.rgb-codes"),
            Some("color.codes")
        );
        let high = metadata(400., 128., 0.);
        let names = outputs(&high, &settings);
        assert!(names.contains(&"reconstruction.codes"));
        assert!(!names.contains(&"color.codes"));
        assert!(!names.contains(&"chroma.layers"));
        assert_eq!(comparison(&high, &settings, "reconstruction.codes"), None);
        settings.reconstruction = None;
        assert!(!outputs(&low, &settings).contains(&"reconstruction.codes"));
        assert!(!outputs(&low, &settings).contains(&"chroma.layers"));
        assert!(outputs(&low, &settings).contains(&"color.gained-codes"));
        assert!(!outputs(&disabled, &settings).contains(&"color.gained-codes"));
        assert_eq!(
            comparison(&low, &settings, "color.gained-codes"),
            Some("color.codes")
        );
        for name in outputs(&low, &settings) {
            if let Some(input) = comparison(&low, &settings, name) {
                assert!(outputs(&low, &settings).contains(&input));
            }
        }
        assert_eq!(comparison(&low, &settings, "calibration.layers"), None);
        assert_eq!(comparison(&low, &settings, "root.rgb"), None);
    }

    #[test]
    fn monochrome_catalogue_matches_its_functional_stage_selection() {
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.rendering = crate::render::Rendering::Monochrome;
        settings.reconstruction = Some(Default::default());
        let low = metadata(100., 128., 0.);
        let names = outputs(&low, &settings);
        assert!(names.contains(&"chroma.small-layers"));
        assert!(names.contains(&"color.codes"));
        for absent in [
            "red-detail.layers",
            "highlight.layers",
            "chroma.layers",
            "reconstruction.codes",
            "chroma.rgb-codes",
            "color.gained-codes",
        ] {
            assert!(
                !names.contains(&absent),
                "unexpected monochrome stage {absent}"
            );
        }
        let high = metadata(3200., 128., 0.);
        assert!(outputs(&high, &settings).contains(&"reconstruction.codes"));
        assert!(!outputs(&high, &settings).contains(&"color.codes"));
        settings.reconstruction = None;
        assert!(!outputs(&low, &settings).contains(&"chroma.small-layers"));
        assert!(!outputs(&low, &settings).contains(&"color.gained-codes"));
        // The high-ISO color conversion still runs without spatial filtering.
        assert!(outputs(&high, &settings).contains(&"reconstruction.codes"));
    }

    #[test]
    #[ignore = "requires CLASSIC_X3F original and a GPU"]
    fn every_catalogued_output_renders() {
        let source = crate::source::Source::open(std::env::var("CLASSIC_X3F").unwrap()).unwrap();
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.reconstruction = Some(Default::default());
        let names = outputs(&source.metadata, &settings);
        // Final aliases the photographic tone output. Request each alias separately.
        let intermediate: Vec<_> = names
            .iter()
            .copied()
            .filter(|name| *name != "final")
            .collect();
        let engine = crate::render::Engine::new().unwrap();
        let artifacts = engine
            .render_artifacts(&source, &settings, &intermediate, &mut Default::default())
            .unwrap();
        assert_eq!(artifacts.len(), intermediate.len());
        assert_eq!(
            engine
                .render_artifacts(&source, &settings, &["final"], &mut Default::default())
                .unwrap()
                .len(),
            1
        );
        for name in &names {
            if let Some(input) = comparison(&source.metadata, &settings, name) {
                assert!(names.contains(&input));
            }
        }
    }
}
