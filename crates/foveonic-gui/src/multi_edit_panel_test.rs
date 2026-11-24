//! Checks that the editing panel's controls reach every editing photo through edits.

use super::*;
use crate::preferences::EditingSection;
use eframe::egui;
use foveonic::render::RepairThreshold;

const PANEL_SIZE: egui::Vec2 = egui::vec2(600., 4000.);

/// The two editing photos, drawn and edited the way the app does each frame.
struct Panel {
    ctx: egui::Context,
    runtime: Runtime,
    sidecars: sidecars::Session,
    range_preview: Option<usize>,
    time: f64,
    output: egui::FullOutput,
}

fn path(index: usize) -> PathBuf {
    PathBuf::from(["a.x3f", "b.x3f"][index])
}

fn metadata(compensation: &str) -> Metadata {
    use foveonic::source::PropertyList;
    let mut metadata = crate::metadata::processing_metadata(
        [
            ("CAMMODEL".into(), "SIGMA DP2 Merrill".into()),
            ("EXPCOMP".into(), compensation.into()),
        ]
        .into(),
    );
    for (list, names) in [
        ("WhiteBalanceGains", ["Sunlight", "Shade"]),
        ("ColorModeCompensations", ["Standard", "Vivid"]),
    ] {
        let entries = names.map(|name| (name.to_owned(), name.to_lowercase()));
        metadata
            .lists
            .insert(list.into(), PropertyList::new(entries.to_vec()).unwrap());
    }
    metadata
}

/// Merrill photos with every switch and checkbox on, differing in unrelated values.
fn photos() -> [Settings; 2] {
    let mut a = Settings::new("Sunlight", "Standard");
    a.reconstruction = Some(foveonic::noise::Controls {
        noise_enabled: true,
        detail_enabled: true,
        ..Default::default()
    });
    a.despeckle = true;
    a.highlight_restoration = true;
    a.repair = Some(RepairThreshold::Camera);
    a.row_correction = true;
    a.merrill_banding.enabled = true;
    a.merrill_banding.method = foveonic::render::MerrillBandingMethod::Reference;
    let mut b = a.clone();
    a.exposure = 0.5;
    b.exposure = -0.3;
    b.tone.contrast = 0.4;
    [a, b]
}

impl Panel {
    fn new(settings: [Settings; 2]) -> Self {
        let ctx = egui::Context::default();
        crate::widgets::install_style(&ctx);
        let mut sidecars = sidecars::Session::new(
            ctx.clone(),
            false,
            Arc::new(foveonic::decode_cache::Cache::new()),
        );
        sidecars.set_memory_only(true);
        let runtime = Runtime::rendered(settings.into_iter().zip(["-1", "-2"]).enumerate().map(
            |(index, (settings, compensation))| {
                let mut numeric = settings.clone();
                numeric.sharpen_amount = numeric.sharpen_amount.or(Some(0.3));
                numeric.color_dq_gain = numeric.color_dq_gain.or(Some(1.));
                numeric.highlight_threshold_ev = numeric.highlight_threshold_ev.or(Some(0.));
                let metadata = Some(Arc::new(metadata(compensation)));
                (path(index), settings, numeric, metadata)
            },
        ));
        let mut panel = Self {
            ctx,
            runtime,
            sidecars,
            range_preview: None,
            time: 0.,
            output: Default::default(),
        };
        panel.frame(vec![]);
        panel
    }

    fn settings(&self, index: usize) -> Settings {
        self.runtime.settings(&path(index)).unwrap().clone()
    }

    fn saved(&self, index: usize) -> Option<&Settings> {
        self.sidecars.requested_settings(&path(index))
    }

    /// Draw one frame of the panel and apply its edits to the editing photos.
    fn frame(&mut self, events: Vec<egui::Event>) {
        self.time += 0.1;
        let Self {
            ctx,
            runtime,
            sidecars,
            range_preview,
            time,
            ..
        } = self;
        self.output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, PANEL_SIZE)),
                time: Some(*time),
                events,
                ..Default::default()
            },
            |ctx| {
                let paths = runtime.editing_paths.clone();
                let settings = runtime.settings(&paths[0]).unwrap().clone();
                let metadata = runtime.metadata(&paths[0]);
                let summary = foveonic::source::MetadataSummary::from_properties(
                    metadata.as_ref().unwrap().properties.clone(),
                );
                runtime.begin_panel(ctx);
                egui::CentralPanel::default().show(ctx, |ui| {
                    crate::controls::processing(
                        ui,
                        &settings,
                        range_preview,
                        metadata.as_deref(),
                        Some(&summary),
                        &mut EditingSection::ALL.to_vec(),
                        &mut crate::editor::Menus {
                            prevent_color_clipping: true,
                            exposure_auto_compensation: Default::default(),
                            family: summary.family,
                            several_photos: false,
                            library: &Default::default(),
                            state: &mut Default::default(),
                        },
                    );
                });
                let edits = multi_edit_controls::finish(ctx);
                runtime.edit(&paths, &Change::Fields(edits), sidecars);
            },
        );
    }

    fn texts(&self, label: &str) -> Vec<egui::Pos2> {
        self.output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .collect()
    }

    fn click(&mut self, pos: egui::Pos2) {
        // A second apart, so consecutive clicks are never a double-click.
        self.time += 1.;
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        self.frame(vec![egui::Event::PointerMoved(pos), button(true)]);
        self.frame(vec![button(false)]);
    }
}

#[test]
fn switches_and_checkboxes_reach_every_editing_photo() {
    type Toggle = fn(&mut Settings);
    let cases: [(&str, bool, Toggle); 10] = [
        ("Tone", false, |s| s.tone_enabled = !s.tone_enabled),
        ("Saturation", false, |s| {
            s.saturation_enabled = !s.saturation_enabled
        }),
        ("Color Range", false, |s| {
            s.color_editor_enabled = !s.color_editor_enabled
        }),
        ("Sharpening", false, |s| {
            s.sharpening_enabled = !s.sharpening_enabled
        }),
        ("Corrections", false, |s| {
            s.corrections_enabled = !s.corrections_enabled
        }),
        ("Speckles", false, |s| s.despeckle = !s.despeckle),
        // The Tone section's Highlights row comes first.
        ("Highlights", true, |s| {
            s.highlight_restoration = !s.highlight_restoration
        }),
        ("Bad pixels", false, |s| s.repair = None),
        ("Correct row offsets", false, |s| {
            s.row_correction = !s.row_correction
        }),
        ("Banding", false, |s| {
            s.merrill_banding.enabled = !s.merrill_banding.enabled
        }),
    ];
    for (label, last, toggle) in cases {
        let original = photos();
        let mut panel = Panel::new(original.clone());
        if label == "Bad pixels" {
            // Corrections' Advanced disclosure comes last.
            panel.click(*panel.texts("Advanced").last().unwrap());
            panel.frame(vec![]);
        }
        let texts = panel.texts(label);
        let point = if last { texts.last() } else { texts.first() };
        panel.click(*point.unwrap_or_else(|| panic!("{label} is drawn")));
        for (index, mut expected) in original.into_iter().enumerate() {
            toggle(&mut expected);
            assert_eq!(panel.settings(index), expected, "{label}, photo {index}");
            assert_eq!(
                panel.saved(index),
                Some(&expected),
                "{label}, photo {index}"
            );
        }
    }
}

#[test]
fn a_white_balance_choice_resets_every_photos_bias() {
    let mut original = photos();
    original[1].white_balance_bias.warmth = 0.4;
    let mut panel = Panel::new(original);
    let mode = panel.texts("Mode")[0];
    let current = *panel
        .texts("Sunlight")
        .iter()
        .find(|point| (point.y - mode.y).abs() < 2.)
        .unwrap();
    panel.click(current);
    panel.frame(vec![]);
    let shade = panel.texts("Shade")[0];
    panel.click(shade);
    for index in 0..2 {
        let settings = panel.settings(index);
        assert_eq!(settings.white_balance, "Shade", "photo {index}");
        assert!(
            settings.white_balance_bias.is_neutral(),
            "photo {index} keeps its bias {:?}",
            settings.white_balance_bias
        );
    }
}

/// The Exposure row's Auto button.
fn exposure_auto(panel: &Panel) -> egui::Pos2 {
    let row = panel.texts("Exposure")[0].y;
    *panel
        .texts("A")
        .iter()
        .find(|point| (point.y - row).abs() < 2.)
        .unwrap()
}

#[test]
fn auto_exposure_turns_on_and_off_for_every_photo_with_its_own_value() {
    let mut panel = Panel::new(photos());
    panel.click(exposure_auto(&panel));
    for index in 0..2 {
        assert!(panel.settings(index).exposure_auto, "photo {index}");
    }
    panel.frame(vec![]);
    panel.click(exposure_auto(&panel));
    // Each photo's capture compensation gives its own automatic exposure.
    for (index, calculated) in [1., 2.].into_iter().enumerate() {
        let settings = panel.settings(index);
        assert!(!settings.exposure_auto, "photo {index}");
        assert_eq!(settings.exposure, calculated, "photo {index}");
    }
}

/// Show the Advanced color list, as its mode button does.
fn advanced(panel: &mut Panel) {
    let mut state = crate::editor::advanced_color::state(&panel.ctx);
    state.advanced = true;
    crate::editor::advanced_color::store(&panel.ctx, state);
    panel.frame(vec![]);
}

#[test]
fn advanced_ranges_are_added_and_removed_on_every_photo() {
    let mut panel = Panel::new(photos());
    advanced(&mut panel);
    panel.click(panel.texts("+")[0]);
    for index in 0..2 {
        let ranges = panel.settings(index).photographic.color_editor.advanced;
        assert_eq!(ranges.count, 1, "photo {index}");
        assert_eq!(panel.saved(index), Some(&panel.settings(index)));
    }
    panel.frame(vec![]);
    panel.click(panel.texts("−")[0]);
    for index in 0..2 {
        let ranges = panel.settings(index).photographic.color_editor.advanced;
        assert_eq!(ranges.count, 0, "photo {index}");
    }
}

#[test]
fn the_range_preview_is_view_state_and_never_an_edit() {
    let mut original = photos();
    for settings in &mut original {
        settings
            .photographic
            .color_editor
            .advanced
            .push(Default::default())
            .unwrap();
    }
    let mut panel = Panel::new(original.clone());
    advanced(&mut panel);
    // The eye sits one gap before the range's swatch.
    let hue = foveonic::render::AdvancedColorRange::default().hue;
    let color: egui::Color32 = egui::ecolor::Hsva::new(hue / 360., 0.65, 0.8, 1.).into();
    let swatch = panel
        .output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.fill == color => Some(rect.rect),
            _ => None,
        })
        .expect("range swatch");
    let side = crate::widgets::ControlSize::Medium.side();
    let eye = egui::pos2(
        swatch.left() - crate::widgets::MEDIUM_CONTROL_GAP - side * 0.5,
        swatch.center().y,
    );
    panel.click(eye);
    assert_eq!(panel.range_preview, Some(0));
    // Basic mode shows no range, so it stops viewing one.
    let mut state = crate::editor::advanced_color::state(&panel.ctx);
    state.advanced = false;
    crate::editor::advanced_color::store(&panel.ctx, state);
    panel.frame(vec![]);
    assert_eq!(panel.range_preview, None);
    for (index, expected) in original.iter().enumerate() {
        assert_eq!(&panel.settings(index), expected, "photo {index}");
        assert_eq!(panel.saved(index), None, "photo {index}");
    }
}

#[test]
fn drawing_a_stored_out_of_range_value_saves_nothing() {
    let mut original = photos();
    original[0].tone.contrast = 2.5;
    let mut panel = Panel::new(original.clone());
    panel.frame(vec![]);
    for (index, expected) in original.iter().enumerate() {
        assert_eq!(&panel.settings(index), expected, "photo {index}");
        assert_eq!(panel.saved(index), None, "photo {index}");
    }
}

#[test]
fn a_value_that_differs_between_photos_reads_multi() {
    let [a, _] = photos();
    let same = Panel::new([a.clone(), a]);
    assert!(same.texts("Multi").is_empty());
    let mixed = Panel::new(photos());
    assert!(!mixed.texts("Multi").is_empty());
}
