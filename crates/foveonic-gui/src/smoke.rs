//! Captures opt-in native workspace checks and the recipe-driven README image using isolated real GPU frames.

use crate::{
    preferences::{Organizer, Scope, Section},
    App,
};
use eframe::egui;
use std::path::PathBuf;

const CROP_TARGETS: &[&str] = &[
    "preview",
    "final",
    "crop-ready",
    "crop-drag",
    "crop-applied",
    "crop-hand",
    "crop-reopen",
    "crop-fast",
    "crop-smart",
    "crop-tile",
    "crop-detail",
];
const STATUS_TARGETS: &[&str] = &[
    "preview",
    "final",
    "status-error",
    "status-log",
    "status-read",
    "status-new-error",
];
const FAST_PREVIEW_TARGETS: &[&str] = &[
    "preview",
    "final",
    "fast",
    "fast-zoom",
    "fast-edit",
    "full-again",
    "fast-again",
];
const SMART_TARGETS: &[&str] = &[
    "preview",
    "final",
    "smart-overview",
    "smart-100",
    "smart-detail",
    "smart-full",
    "smart-pan",
    "smart-edit",
    "smart-fit",
    "cache-settings",
    "cache-list",
    "cache-reopen",
];

const DISPLAY_COLOR_TARGETS: &[&str] = &[
    "preview",
    "final",
    "gamut-monitor",
    "gamut-adobe",
    "gamut-srgb",
    "gamut-romm",
    "gamut-monitor-again",
    "gamut-settings",
];

const SCREENSHOT_TARGETS: &[&str] = &["preview", "readme"];
const WORKFLOW_TARGETS: &[&str] = &[
    "preview",
    "final",
    "workflow-current",
    "workflow-camera",
    "workflow-restored",
    "workflow-defaults",
    "workflow-restored-again",
    "workflow-wb",
    "workflow-default-reset",
    "workflow-camera-reset",
];
const BROWSER_TARGETS: &[&str] = &[
    "preview",
    "final",
    "browser-grid",
    "browser-list",
    "browser-large",
];
const COMPARISON_TARGETS: &[&str] = &["preview", "final", "comparison", "comparison-switch"];
const BASE_RESPONSE_TARGETS: &[&str] = &[
    "preview",
    "final",
    "base-camera",
    "base-natural",
    "base-strong",
    "base-limits",
    "base-hdr",
    "base-monochrome",
];
const ADVANCED_COLOR_TARGETS: &[&str] = &[
    "preview",
    "final",
    "advanced-ready",
    "advanced-picked",
    "advanced-ranges",
    "advanced-name",
    "advanced-selection",
    "advanced-monochrome",
];
const DEHAZE_TARGETS: &[&str] = &[
    "preview",
    "final",
    "dehaze-auto",
    "dehaze-freeze",
    "dehaze-picked",
    "dehaze-manual",
    "dehaze-add",
    "dehaze-monochrome",
    "dehaze-off",
    "dehaze-reset",
];
const DIRECT_COLOR_TARGETS: &[&str] = &[
    "preview",
    "final",
    "direct-color-ready",
    "direct-color-edited",
    "direct-color-settings",
    "direct-color-exit",
];
const PERCEPTUAL_TARGETS: &[&str] = &[
    "preview",
    "final",
    "perceptual-neutral",
    "perceptual-grade",
    "perceptual-extended",
    "perceptual-unlocked",
    "perceptual-mask",
    "perceptual-editor",
    "perceptual-mono",
];
const EXPOSURE_AUTO_TARGETS: &[&str] = &[
    "preview",
    "final",
    "exposure-auto-reference",
    "exposure-auto-universal",
    "exposure-auto-dial",
    "exposure-auto-bracket",
    "exposure-auto-monochrome",
    "exposure-auto-manual",
];
const IMPORT_WIDE_BROWSER_WIDTH: f32 = 600.;
const IMPORT_TARGETS: &[&str] = &[
    "preview",
    "final",
    "browse-settings",
    "browse-processed",
    "browse-processed-again",
    "import-review",
    "import-render-options",
    "import-copied",
];

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenshotRecipe {
    pub source: String,
    pub window_size: [f32; 2],
    pub organizer_width: f32,
    pub tools_width: f32,
    zoom: f32,
    pan: [f32; 2],
    settings: foveonic::render::Settings,
}

pub fn screenshot_active() -> bool {
    std::env::var_os("FOVEONIC_GUI_SMOKE_DIR").is_some()
        && std::env::var_os("FOVEONIC_GUI_SMOKE_SCREENSHOT").is_some()
}

pub fn screenshot_recipe() -> ScreenshotRecipe {
    serde_json::from_str(include_str!("../assets/gui-screenshot.json"))
        .expect("Invalid screenshot recipe")
}

const TARGETS: [&str; 10] = [
    "preview",
    "final",
    "list",
    "parade",
    "waveform",
    "zoom",
    "reordered",
    "collapsed",
    "sensor.layers",
    "calibration.layers",
];

const TIGER_TARGETS: &[&str] = &[
    "preview",
    "final",
    "highlight-restoration.recovery-mask",
    "raw-color.rgb",
    "reconstruction.rgb",
];

const VIEWPORT_SCOPE_TARGETS: &[&str] = &[
    "preview",
    "final",
    "viewport-fit",
    "viewport-zoom",
    "viewport-pan",
    "viewport-parade",
    "viewport-waveform",
    "viewport-resize",
    "viewport-whole",
    "display-wide",
    "display-narrow",
    "viewport-sensor",
    "viewport-fit-again",
];

const DIFFERENCE_TARGETS: &[&str] = &[
    "preview",
    "final",
    "diff-layer",
    "diff-amplified",
    "diff-layer-two",
    "diff-output",
    "compare-input",
    "compare-output",
    "diff-rgb",
    "compare-rgb-input",
    "compare-rgb-output",
    "diff-final",
];

const DETAIL_TARGETS: &[&str] = &[
    "preview",
    "final",
    "reconstructed",
    "slide-detail-0.0",
    "slide-detail-0.2",
    "slide-detail-0.4",
    "slide-detail-0.6",
    "slide-detail-0.8",
    "slide-detail-1.0",
    "slide-detail-1.2",
    "slide-detail-1.4",
    "slide-detail-1.6",
    "slide-detail-1.8",
];

const NOISE_TARGETS: &[&str] = &[
    "preview",
    "final",
    "reconstructed",
    "noise.layers",
    "noise.edges",
    "detail.ratio",
    "color.rgb",
    "reconstruction.rgb",
    "detail.signal",
    "detail-zero",
    "noise-low",
    "noise-high",
    "color-dq",
    "color-dq-off",
    "parade",
    "waveform",
    "zoom",
    "final-neutral",
];

const EDITOR_TARGETS: &[&str] = &[
    "preview",
    "final",
    "reconstructed",
    "fine-soft",
    "coarse-large",
    "neutral-detail",
    "rgb-blue",
    "layers-two",
    "repair-auto",
    "repair-zero",
    "repair-zoom",
    "bolt",
    "white-edge",
    "final-neutral",
];

const SPECKLE_TARGETS: &[&str] = &[
    "preview",
    "final",
    "reconstructed",
    "camera-only",
    "bolt-before",
    "white-before",
    "bolt-after",
    "white-after",
    "despeckle.layers",
    "final-disabled",
    "final-default",
];

const REPAIR_TARGETS: &[&str] = &[
    "preview",
    "final",
    "leaf-before",
    "leaf-after",
    "reconstructed",
    "final-default",
];

pub fn editor_review_active() -> bool {
    std::env::var_os("FOVEONIC_GUI_SMOKE_DIR").is_some()
        && std::env::var_os("FOVEONIC_GUI_SMOKE_EDITOR").is_some()
}

pub struct Smoke {
    directory: PathBuf,
    targets: &'static [&'static str],
    index: usize,
    awaiting: bool,
    direct_events: std::collections::VecDeque<Vec<egui::Event>>,
    close_at: Option<std::time::Instant>,
    settled_frames: u8,
    fast_region: Option<crate::viewport::PixelRegion>,
    smart_full_revision: Option<u64>,
    display_revision: Option<u64>,
    authored: Option<foveonic::render::Settings>,
    name_revision: Option<u64>,
    current_texture: Option<std::sync::Arc<foveonic::image::ImageTexture>>,
}
impl Smoke {
    pub fn inject_direct_color(&mut self, input: &mut egui::RawInput, ctx: &egui::Context) {
        if self.targets == DIRECT_COLOR_TARGETS
            || self.targets == DEHAZE_TARGETS
            || self.targets == ADVANCED_COLOR_TARGETS
            || self.targets == CROP_TARGETS
            || self.targets == STATUS_TARGETS
        {
            // Inject focused input without moving the owner's system pointer or focus.
            input.focused = true;
        }
        if let Some(events) = self.direct_events.pop_front() {
            input.events.push(egui::Event::WindowFocused(true));
            input.events.extend(events);
            ctx.request_repaint();
        }
    }
    pub fn from_env() -> Option<Self> {
        std::env::var_os("FOVEONIC_GUI_SMOKE_DIR").map(|directory| Self {
            directory: directory.into(),
            targets: if std::env::var_os("FOVEONIC_GUI_SMOKE_STATUS").is_some() {
                STATUS_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_CROP").is_some() {
                CROP_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_EXPOSURE_AUTO").is_some() {
                EXPOSURE_AUTO_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_DEHAZE").is_some() {
                DEHAZE_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_BASE_RESPONSE").is_some() {
                BASE_RESPONSE_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_ADVANCED_COLOR").is_some() {
                ADVANCED_COLOR_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_DIRECT_COLOR").is_some() {
                DIRECT_COLOR_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_PERCEPTUAL").is_some() {
                PERCEPTUAL_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_COMPARISON").is_some() {
                COMPARISON_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_WORKFLOW").is_some() {
                WORKFLOW_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_DISPLAY").is_some() {
                DISPLAY_COLOR_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_BROWSER").is_some() {
                BROWSER_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_IMPORT").is_some() {
                IMPORT_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_SMART").is_some() {
                SMART_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_FAST").is_some() {
                FAST_PREVIEW_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_TIGER").is_some() {
                TIGER_TARGETS
            } else if screenshot_active() {
                SCREENSHOT_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_SCOPES").is_some() {
                VIEWPORT_SCOPE_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_INSPECTION").is_some() {
                DIFFERENCE_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_DETAIL").is_some() {
                DETAIL_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_REPAIR").is_some() {
                REPAIR_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_DESPECKLE").is_some() {
                SPECKLE_TARGETS
            } else if editor_review_active() {
                EDITOR_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_NOISE").is_some() {
                NOISE_TARGETS
            } else {
                &TARGETS
            },
            index: 0,
            awaiting: false,
            direct_events: Default::default(),
            close_at: None,
            settled_frames: 0,
            fast_region: None,
            smart_full_revision: None,
            display_revision: None,
            authored: None,
            name_revision: None,
            current_texture: None,
        })
    }
    /// Hold processing replies until the embedded preview has been captured.
    pub fn preview_pending(&self) -> bool {
        self.index == 0
    }
}

impl App {
    pub(super) fn smoke_check_scope_retention(&self) {
        let Some(smoke) = &self.smoke else {
            return;
        };
        if smoke.targets != VIEWPORT_SCOPE_TARGETS || smoke.index >= smoke.targets.len() {
            return;
        }
        let target = smoke.targets[smoke.index];
        if target.starts_with("viewport-") && target != "viewport-sensor" {
            // Check immediately after scheduling, before the UI can receive a reply.
            assert!(
                self.scope_snapshot.is_some(),
                "scope refresh blanked the counts"
            );
            assert!(
                self.scope_images.is_some(),
                "scope refresh blanked the textures"
            );
        }
    }

    pub(super) fn smoke_tick(&mut self, ctx: &egui::Context) {
        let mapping = self.displayed_mapping();
        let quattro_smart = self.frame.as_ref().is_some_and(|frame| {
            frame.artifact.metadata.processing_family().ok()
                == Some(foveonic::source::ProcessingFamily::Quattro)
        });
        let regional_smart = self.frame.as_ref().is_some_and(|frame| {
            frame
                .artifact
                .settings
                .regional_preview_available(&frame.artifact.metadata)
        });
        let Some(smoke) = &mut self.smoke else {
            return;
        };
        if let Some(close_at) = smoke.close_at {
            if std::time::Instant::now() >= close_at {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
            return;
        }
        // A background repaint may arrive before the window handles Close.
        if smoke.index >= smoke.targets.len() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let screenshot = ctx.input(|i| {
            i.events.iter().find_map(|e| {
                if let egui::Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = screenshot {
            if smoke.targets == STATUS_TARGETS {
                match smoke.targets[smoke.index] {
                    "status-error" | "status-new-error" => assert!(self.activity.unread_errors()),
                    "status-log" => {
                        assert!(self.activity.is_open());
                        assert!(!self.activity.unread_errors());
                    }
                    "status-read" => {
                        assert!(!self.activity.is_open());
                        assert!(!self.activity.unread_errors());
                    }
                    _ => {}
                }
            }
            if smoke.targets == CROP_TARGETS && !smoke.preview_pending() {
                let frame = self.frame.as_ref().unwrap();
                let target = smoke.targets[smoke.index];
                if target.starts_with("crop-") {
                    assert_eq!(
                        frame.crop_preview,
                        matches!(
                            target,
                            "crop-ready" | "crop-drag" | "crop-applied" | "crop-reopen"
                        )
                    );
                    assert_eq!(crate::crop_tool::active(ctx), frame.crop_preview);
                    assert_eq!(
                        self.settings.crop.is_full(),
                        matches!(target, "crop-ready" | "crop-drag")
                    );
                    assert_eq!(frame.artifact.settings.crop, self.settings.crop);
                    if target == "crop-tile" && regional_smart {
                        let detail = self.detail.as_ref().unwrap();
                        let visible = self.viewport.visible_pixels().unwrap();
                        let window = frame.display_window(false);
                        assert!(crate::smart::contains(
                            detail.artifact.settings.preview_region.unwrap(),
                            [
                                visible.top_left[0] + window[0],
                                visible.top_left[1] + window[1],
                                visible.bottom_right[0] + 1 + window[0],
                                visible.bottom_right[1] + 1 + window[1],
                            ]
                        ));
                    }
                    if frame.crop_preview {
                        assert_eq!(frame.display_extent(false), frame.uncropped_extent());
                    }
                    std::fs::write(
                        smoke.directory.join(format!("{target}.json")),
                        serde_json::to_vec_pretty(&self.settings.crop).unwrap(),
                    )
                    .unwrap();
                }
            }
            if smoke.targets == COMPARISON_TARGETS
                && smoke.targets[smoke.index] == "comparison-switch"
            {
                let selected = self.browser.processing_selection(self.selected.as_deref());
                assert!(selected.len() > 1);
                assert_eq!(selected.get(1), self.selected.as_ref());
            }
            if smoke.targets == WORKFLOW_TARGETS {
                let frame = self.frame.as_ref();
                let target = smoke.targets[smoke.index];
                if target == "workflow-current" {
                    smoke.authored = Some(self.settings.clone());
                    smoke.fast_region = self.viewport.visible_pixels();
                    smoke.display_revision = frame.map(|f| f.revision);
                    smoke.current_texture = frame.map(|f| f.texture.clone());
                }
                if matches!(
                    target,
                    "workflow-camera"
                        | "workflow-defaults"
                        | "workflow-restored"
                        | "workflow-restored-again"
                ) {
                    assert_eq!(
                        Some(&self.settings),
                        smoke.authored.as_ref(),
                        "Before must preserve authored edits"
                    );
                    assert_eq!(
                        self.viewport.visible_pixels(),
                        smoke.fast_region,
                        "Before must preserve viewport geometry"
                    );
                    if target.starts_with("workflow-restored") {
                        assert!(
                            std::sync::Arc::ptr_eq(
                                &frame.unwrap().texture,
                                smoke.current_texture.as_ref().unwrap()
                            ),
                            "Restore must reuse current frame"
                        );
                    } else {
                        assert_ne!(frame.unwrap().rendition, crate::editor::Rendition::Current);
                        assert_ne!(
                            self.browser.rendered_revision(&frame.unwrap().path),
                            frame.map(|f| f.revision)
                        );
                    }
                }
            }
            if smoke.targets == BROWSER_TARGETS
                && smoke.targets[smoke.index].starts_with("browser-")
            {
                assert_eq!(
                    self.frame.as_ref().map(|frame| frame.revision),
                    smoke.display_revision,
                    "browser changes must not rerender the image"
                );
            }
            std::fs::create_dir_all(&smoke.directory).unwrap();
            let bytes: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            if smoke.targets == DEHAZE_TARGETS {
                use image::{
                    codecs::png::{CompressionType, FilterType, PngEncoder},
                    ImageEncoder,
                };
                // Lossless fast encoding keeps debug capture from blocking native interaction.
                let file = std::fs::File::create(
                    smoke
                        .directory
                        .join(format!("{}.png", smoke.targets[smoke.index])),
                )
                .unwrap();
                PngEncoder::new_with_quality(
                    file,
                    CompressionType::Uncompressed,
                    FilterType::NoFilter,
                )
                .write_image(
                    &bytes,
                    image.width() as u32,
                    image.height() as u32,
                    image::ExtendedColorType::Rgba8,
                )
                .unwrap();
            } else {
                image::save_buffer(
                    smoke
                        .directory
                        .join(format!("{}.png", smoke.targets[smoke.index])),
                    &bytes,
                    image.width() as u32,
                    image.height() as u32,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
            if smoke.targets == SMART_TARGETS && smoke.preview_pending() {
                assert_eq!(
                    self.preferences.preview_quality,
                    crate::preferences::PreviewMode::Smart
                );
                let support = crate::inspect::preview_support(
                    &self.settings,
                    self.metadata.as_deref(),
                    self.capture.properties.as_ref(),
                );
                assert_ne!(support, Some(false));
                let preview = self
                    .browser
                    .thumbnail(self.selected.as_ref().unwrap())
                    .unwrap();
                let report = serde_json::json!({"preview_choice":"Smart", "render_received":self.frame.is_some(), "support":support,"native_extent":preview.native_extent,"thumbnail_extent":preview.texture.size()});
                std::fs::write(
                    smoke.directory.join("preview.json"),
                    serde_json::to_vec_pretty(&report).unwrap(),
                )
                .unwrap();
            }
            if smoke.targets == VIEWPORT_SCOPE_TARGETS && !smoke.preview_pending() {
                let frame = self.frame.as_ref().unwrap();
                let snapshot = self.scope_snapshot.as_ref().unwrap();
                let key = snapshot.key;
                assert_eq!(self.scope_key, Some(key));
                let data = &snapshot.data;
                assert_eq!(key.revision, frame.revision);
                let expected_region = self.preferences.scope_viewport.then(|| {
                    let pixels = self.viewport.visible_pixels().unwrap();
                    [
                        pixels.top_left[0],
                        pixels.top_left[1],
                        pixels.bottom_right[0] + 1,
                        pixels.bottom_right[1] + 1,
                    ]
                });
                assert_eq!(key.region, expected_region);
                assert_eq!(
                    data.histogram(0).iter().sum::<u32>(),
                    data.samples[0] * data.samples[1]
                );
                let density_weight: u32 = data.density(0).iter().sum();
                assert_eq!(
                    density_weight,
                    data.histogram(0).iter().sum::<u32>() * foveonic::scopes::DENSITY_SCALE
                );
                let report = serde_json::json!({"revision":key.revision,"region":key.region,"samples":data.samples,"histogram":data.histogram(0),"density_weight":density_weight,"density_scale":foveonic::scopes::DENSITY_SCALE});
                std::fs::write(
                    smoke
                        .directory
                        .join(format!("{}.json", smoke.targets[smoke.index])),
                    serde_json::to_vec_pretty(&report).unwrap(),
                )
                .unwrap();
            }
            if smoke.targets == FAST_PREVIEW_TARGETS && !smoke.preview_pending() {
                let frame = self.frame.as_ref().unwrap();
                let target = smoke.targets[smoke.index];
                let fast = target.starts_with("fast");
                assert_eq!(
                    frame.artifact.settings.preview_quality,
                    if fast {
                        foveonic::render::PreviewQuality::Fast
                    } else {
                        foveonic::render::PreviewQuality::Full
                    }
                );
                let native = frame.display_extent(false);
                assert_eq!(
                    frame.texture.description().extent,
                    if fast { native.map(|v| v / 2) } else { native }
                );
                let visible = self.viewport.visible_pixels().unwrap();
                if target == "fast-edit" {
                    smoke.fast_region = Some(visible);
                }
                if target == "full-again" || target == "fast-again" {
                    assert_eq!(
                        Some(visible),
                        smoke.fast_region,
                        "quality switch moved the viewport"
                    );
                }
                let actual = self
                    .viewport
                    .visible_pixels_for(frame.texture.description().extent)
                    .unwrap();
                let expected_scope = [
                    actual.top_left[0],
                    actual.top_left[1],
                    actual.bottom_right[0] + 1,
                    actual.bottom_right[1] + 1,
                ];
                if target != "final" {
                    assert_eq!(self.scope_key.unwrap().region, Some(expected_scope));
                }
                let progress = self.worker.progress.snapshot().unwrap();
                assert_eq!(progress.revision, frame.revision);
                assert_eq!(progress.fraction, 1.);
                assert!(!progress.running());
                assert!(progress.milliseconds() >= frame.milliseconds);
                let report = serde_json::json!({"total_ms":progress.milliseconds(),"progress":progress.fraction,"target":target,"quality":frame.quality_label(),"rendered_extent":frame.texture.description().extent,"native_extent":native,"visible_native":[visible.top_left,visible.bottom_right],"scope_region":self.scope_key.unwrap().region,"thumbnail_revision":self.browser.rendered_revision(&frame.path),"processing_ms":frame.milliseconds});
                std::fs::write(
                    smoke.directory.join(format!("{target}.json")),
                    serde_json::to_vec_pretty(&report).unwrap(),
                )
                .unwrap();
            }
            if smoke.targets == SMART_TARGETS && !smoke.preview_pending() {
                let target = smoke.targets[smoke.index];
                let regional = target == "smart-detail" && regional_smart;
                let frame = if regional {
                    self.detail.as_ref().unwrap()
                } else {
                    self.frame.as_ref().unwrap()
                };
                let visible = self.viewport.visible_pixels().unwrap();
                let region = frame.artifact.settings.preview_region;
                if !quattro_smart || !matches!(target, "final" | "cache-reopen") {
                    assert!(self.settings.reconstruction.is_some());
                    assert!(frame.artifact.settings.reconstruction.is_some());
                }
                if regional {
                    let rect = region.unwrap();
                    assert!(crate::smart::contains(
                        rect,
                        [
                            visible.top_left[0],
                            visible.top_left[1],
                            visible.bottom_right[0] + 1,
                            visible.bottom_right[1] + 1
                        ]
                    ));
                    assert_eq!(
                        frame.texture.description().extent,
                        [rect[2] - rect[0], rect[3] - rect[1]]
                    );
                    assert_eq!(
                        frame.artifact.settings.preview_quality,
                        foveonic::render::PreviewQuality::Full
                    );
                } else {
                    let full = matches!(
                        target,
                        "final"
                            | "smart-detail"
                            | "smart-full"
                            | "smart-pan"
                            | "smart-edit"
                            | "smart-fit"
                            | "cache-settings"
                            | "cache-list"
                    );
                    assert_eq!(
                        frame.artifact.settings.preview_quality,
                        if full {
                            foveonic::render::PreviewQuality::Full
                        } else {
                            foveonic::render::PreviewQuality::Fast
                        }
                    );
                    assert_eq!(frame.artifact.settings.preview_region, None);
                    if target == "smart-pan" {
                        assert_eq!(
                            smoke.smart_full_revision,
                            Some(frame.revision),
                            "panning submitted more render work"
                        );
                    }
                }
                if target == "smart-full" {
                    smoke.smart_full_revision = Some(frame.revision);
                }
                if target == "cache-reopen" {
                    assert!(self.worker.cache.status().hits > 0);
                }
                let report = serde_json::json!({"target":target,"quality":frame.quality_label(),"region":region,"regional_supported":regional_smart,"native_extent":self.frame.as_ref().unwrap().display_extent(false),"rendered_extent":frame.texture.description().extent,
                    "zoom_percent":self.viewport.zoom_percent(ctx.pixels_per_point()),"thumbnail_revision":self.browser.rendered_revision(&frame.path),"processing_ms":frame.milliseconds,"scope_revision":self.scope_key.map(|k|k.revision),"image_revision":frame.revision,"cache_hits":self.worker.cache.status().hits,"cache_ready":self.worker.cache.status().ready.len(),"continuation_running":self.busy});
                std::fs::write(
                    smoke.directory.join(format!("{target}.json")),
                    serde_json::to_vec_pretty(&report).unwrap(),
                )
                .unwrap();
            }
            if smoke.targets == DISPLAY_COLOR_TARGETS && !smoke.preview_pending() {
                let frame = self.frame.as_ref().unwrap();
                let revision = *smoke.display_revision.get_or_insert(frame.revision);
                assert_eq!(
                    frame.revision, revision,
                    "display choice submitted photographic work"
                );
                assert!(!self.busy);
                let report = serde_json::json!({"gamut":self.preferences.display_gamut,
                    "image_revision":revision,"thumbnail_revision":self.browser.rendered_revision(&frame.path),
                    "format":format!("{:?}",self.render_state.target_format)});
                std::fs::write(
                    smoke
                        .directory
                        .join(format!("{}.json", smoke.targets[smoke.index])),
                    serde_json::to_vec_pretty(&report).unwrap(),
                )
                .unwrap();
            }
            if smoke.targets == DEHAZE_TARGETS && !smoke.preview_pending() {
                let frame = self.frame.as_ref().unwrap();
                let target = smoke.targets[smoke.index];
                if target.starts_with("dehaze-") && target != "dehaze-off" {
                    assert_eq!(
                        frame.artifact.settings.preview_quality,
                        foveonic::render::PreviewQuality::Full
                    );
                    assert_eq!(frame.artifact.settings.preview_region, None);
                    assert_eq!(frame.artifact.settings.dehaze, self.settings.dehaze);
                }
                let estimated = crate::editor::dehaze::auto_color(ctx, &self.settings);
                if target == "final" {
                    assert!(!self.settings.dehaze_auto());
                    assert!(frame.artifact.dehaze_estimate.is_none());
                }
                if target == "dehaze-auto" {
                    assert_eq!(estimated, frame.artifact.dehaze_estimate);
                    assert!(estimated.is_some(), "fixture needs an automatic haze color");
                }
                if target == "dehaze-freeze" {
                    assert_eq!(self.settings.dehaze.shadow, estimated);
                }
                if target == "dehaze-reset" {
                    assert!(self.settings.dehaze.shadow.is_some());
                    assert!(!self.settings.dehaze_auto());
                }
                let report = serde_json::json!({"target":target,"dehaze":self.settings.dehaze,"automatic_color":estimated,"shown_color":self.settings.dehaze.shadow.or(estimated),"rendering":self.settings.rendering,"quality":frame.quality_label(),"processing_ms":frame.milliseconds});
                std::fs::write(
                    smoke.directory.join(format!("{target}.json")),
                    serde_json::to_vec_pretty(&report).unwrap(),
                )
                .unwrap();
            }
            smoke.index += 1;
            smoke.awaiting = false;
            if smoke.index == smoke.targets.len() {
                if smoke.targets == SCREENSHOT_TARGETS {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
                // Exercise native preference restoration on a subsequent isolated launch.
                self.preferences.organizer = Organizer::List;
                self.preferences.scope = Scope::Waveform;
                self.preferences
                    .move_section(Section::Metadata, Section::Navigator, false);
                self.preferences.collapsed = [Section::Editing].into();
                self.settings.exposure = 1.25; // deliberately session-only
                if let Some(seconds) = std::env::var_os("FOVEONIC_GUI_SMOKE_HOLD_SECONDS")
                    .and_then(|s| s.to_str().and_then(|s| s.parse::<u64>().ok()))
                {
                    smoke.close_at =
                        Some(std::time::Instant::now() + std::time::Duration::from_secs(seconds));
                    ctx.request_repaint();
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                return;
            }
            match smoke.targets[smoke.index] {
                "status-error" | "status-new-error" => {
                    let path = smoke.directory.join(smoke.targets[smoke.index]);
                    self.open(path, crate::preferences::FolderMode::Browse);
                }
                "status-log" => self.activity.open(),
                "status-read" => {
                    smoke.direct_events.push_back(
                        [true, false]
                            .map(|pressed| egui::Event::Key {
                                key: egui::Key::Escape,
                                physical_key: None,
                                pressed,
                                repeat: false,
                                modifiers: Default::default(),
                            })
                            .into(),
                    );
                }
                "crop-fast" => {
                    crate::crop_tool::exit(ctx);
                    self.preferences.preview_quality = crate::preferences::PreviewMode::Fast;
                }
                "crop-smart" => {
                    self.preferences.preview_quality = crate::preferences::PreviewMode::Smart;
                    self.viewport.one_to_one = true;
                    self.submit(None);
                }
                "crop-tile" => self.viewport.zoom_by(2.),
                "crop-detail" => {}
                "crop-ready" | "crop-reopen" | "crop-hand" => {
                    let key = if smoke.targets[smoke.index] == "crop-hand" {
                        egui::Key::H
                    } else {
                        egui::Key::C
                    };
                    smoke.direct_events.push_back(
                        [true, false]
                            .map(|pressed| egui::Event::Key {
                                key,
                                physical_key: None,
                                pressed,
                                repeat: false,
                                modifiers: Default::default(),
                            })
                            .into(),
                    );
                }
                "crop-drag" | "crop-applied" => {
                    let target = smoke.targets[smoke.index];
                    let point = self
                        .viewport
                        .position_at_uv(if target == "crop-drag" {
                            [0.002, 0.002]
                        } else {
                            [0.18, 0.18]
                        })
                        .unwrap();
                    smoke.direct_events.push_back(vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed: target == "crop-drag",
                            modifiers: Default::default(),
                        },
                    ]);
                    if target == "crop-drag" {
                        let end = self.viewport.position_at_uv([0.18, 0.18]).unwrap();
                        smoke
                            .direct_events
                            .push_back(vec![egui::Event::PointerMoved(end)]);
                    }
                }
                "dehaze-auto" => {
                    self.viewport.reset();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Title(
                        "Foveonic — Dehaze check".into(),
                    ));
                    self.preferences.collapsed =
                        [Section::Navigator, Section::Scopes, Section::Metadata].into();
                    self.preferences
                        .editing_sections
                        .retain(|s| *s != crate::preferences::EditingSection::Corrections);
                    self.preferences
                        .editing_sections
                        .insert(0, crate::preferences::EditingSection::Corrections);
                    self.settings.dehaze = foveonic::render::DehazeControls {
                        amount: 35.,
                        shadow: None,
                    };
                    self.submit(None);
                }
                "dehaze-freeze" => {
                    assert!(crate::editor::dehaze::set_auto(
                        ctx,
                        &mut self.settings,
                        false
                    ));
                    self.submit(None);
                }
                "dehaze-reset" => {
                    let family = self.metadata.as_ref().unwrap().processing_family().unwrap();
                    let scope = crate::editor::Scope::Tool(crate::editor::Section::Corrections);
                    // Reset uses the configured manual color without enabling background estimation.
                    self.preferences
                        .editor
                        .defaults
                        .set(family, scope, &self.settings);
                    self.settings.dehaze.shadow = None;
                    self.editor_ui
                        .actions
                        .push(crate::editor::Action::Reset(scope, false));
                    assert!(self.apply_editor_actions(ctx));
                    assert!(self.settings.dehaze.shadow.is_some());
                    self.submit(None);
                }
                "dehaze-picked" => {
                    crate::editor::dehaze::toggle(ctx);
                    // The neutral bolt in the parking-sign fixture avoids a clipped blue patch.
                    let position = self.viewport.position_at_uv([0.82, 0.04]).unwrap();
                    let button = |pressed| egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    };
                    smoke.direct_events.extend([
                        vec![egui::Event::PointerMoved(position), button(true)],
                        vec![button(false)],
                    ]);
                }
                "dehaze-manual" | "dehaze-add" | "dehaze-monochrome" | "dehaze-off" => {
                    crate::editor::dehaze::exit(ctx);
                    assert!(self.settings.dehaze.shadow.is_some());
                    let target = smoke.targets[smoke.index];
                    self.settings.dehaze.amount = match target {
                        "dehaze-manual" => 65.,
                        "dehaze-add" => -35.,
                        "dehaze-off" => 0.,
                        _ => 35.,
                    };
                    if target == "dehaze-monochrome" {
                        self.settings.rendering = foveonic::render::Rendering::Monochrome;
                    }
                    self.submit(None);
                }
                "advanced-ready" => {
                    self.preferences.collapsed =
                        [Section::Navigator, Section::Scopes, Section::Metadata].into();
                    self.preferences
                        .editing_sections
                        .retain(|s| *s != crate::preferences::EditingSection::ColorEditor);
                    self.preferences
                        .editing_sections
                        .insert(0, crate::preferences::EditingSection::ColorEditor);
                    self.settings.photographic.color_editor = Default::default();
                    let mut state = crate::editor::advanced_color::state(ctx);
                    state.advanced = true;
                    state.selected = 0;
                    crate::editor::direct_color::toggle_advanced(ctx);
                    crate::editor::advanced_color::store(ctx, state);
                    self.submit(None);
                }
                "advanced-picked" => {
                    let (response, _) = self.viewport.pointer(None).unwrap();
                    let position = response.rect.center();
                    let end = position + egui::vec2(60., -90.);
                    let button = |pos, pressed| egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    };
                    smoke.direct_events.extend([
                        vec![egui::Event::PointerMoved(position), button(position, true)],
                        vec![egui::Event::PointerMoved(end)],
                        vec![button(end, false)],
                    ]);
                }
                "advanced-name" => {
                    let mut state = crate::editor::advanced_color::state(ctx);
                    state.rename = Some(crate::editor::advanced_color::Rename {
                        index: 0,
                        text: "Petals".into(),
                        focus: true,
                    });
                    crate::editor::advanced_color::store(ctx, state);
                    smoke.name_revision = Some(self.revision);
                    smoke.direct_events.extend([
                        vec![egui::Event::PointerMoved(egui::pos2(6., 6.))],
                        vec![
                            egui::Event::Key {
                                key: egui::Key::Enter,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: egui::Modifiers::NONE,
                            },
                            egui::Event::Key {
                                key: egui::Key::Enter,
                                physical_key: None,
                                pressed: false,
                                repeat: false,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                    ]);
                }
                "advanced-ranges" | "advanced-selection" | "advanced-monochrome" => {
                    let target = smoke.targets[smoke.index];
                    assert_eq!(self.settings.photographic.color_editor.advanced.count, 1);
                    self.settings.photographic.color_editor.advanced.ranges[0]
                        .adjustment
                        .lightness = -0.3;
                    self.settings.color_range_preview =
                        (target == "advanced-selection").then_some(0);
                    self.settings.rendering = if target == "advanced-monochrome" {
                        foveonic::render::Rendering::Monochrome
                    } else {
                        foveonic::render::Rendering::Color
                    };
                    self.submit(None);
                }
                "base-camera" | "base-natural" | "base-strong" | "base-limits" | "base-hdr"
                | "base-monochrome" => {
                    let target = smoke.targets[smoke.index];
                    self.preferences.collapsed =
                        [Section::Navigator, Section::Scopes, Section::Metadata].into();
                    self.preferences
                        .editing_sections
                        .retain(|s| *s != crate::preferences::EditingSection::Tone);
                    self.preferences
                        .editing_sections
                        .insert(0, crate::preferences::EditingSection::Tone);
                    self.settings.rendering = if target == "base-monochrome" {
                        foveonic::render::Rendering::Monochrome
                    } else {
                        foveonic::render::Rendering::Color
                    };
                    self.settings.photographic.tone_method =
                        foveonic::render::PhotographicMethod::Universal;
                    self.settings_open = false;
                    crate::application_settings::set_adaptive_open(ctx, target == "base-limits");
                    self.preferences.adaptive_tone_tuning = Default::default();
                    if target == "base-limits" {
                        self.preferences.adaptive_tone_tuning.natural.shadows = 0.75;
                    }
                    self.settings.photographic.base_response = if target == "base-camera" {
                        foveonic::render::BaseResponse::Camera
                    } else if target == "base-strong" {
                        foveonic::render::BaseResponse::Strong
                    } else {
                        foveonic::render::BaseResponse::Natural
                    };
                    self.settings.tonal_ranges = if target == "base-hdr" {
                        foveonic::render::TonalRanges {
                            shadows: 0.2,
                            whites: -0.2,
                            ..Default::default()
                        }
                    } else {
                        Default::default()
                    };
                    self.submit(None);
                }
                "direct-color-ready" => {
                    if std::env::var_os("FOVEONIC_GUI_SMOKE_DIRECT_COLOR_COMPARISON").is_some() {
                        self.browser.set_actions(vec![(
                            crate::shortcuts::BrowserAction::SelectAll,
                            egui::Modifiers::COMMAND,
                        )]);
                    }
                    self.preferences.collapsed =
                        [Section::Navigator, Section::Scopes, Section::Metadata].into();
                    self.preferences
                        .editing_sections
                        .retain(|s| *s != crate::preferences::EditingSection::ColorEditor);
                    self.preferences
                        .editing_sections
                        .insert(0, crate::preferences::EditingSection::ColorEditor);
                    self.settings.photographic.color_editor = Default::default();
                    smoke.direct_events.push_back(vec![
                        egui::Event::Key {
                            key: egui::Key::D,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                        egui::Event::Key {
                            key: egui::Key::D,
                            physical_key: None,
                            pressed: false,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]);
                    self.submit(None);
                }
                "direct-color-edited" => {
                    let (response, _) = self.viewport.pointer(None).unwrap();
                    let anchor = response.rect.center();
                    let end = anchor + egui::vec2(60., -90.);
                    let button = |pos, pressed| egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    };
                    smoke.direct_events.extend([
                        vec![egui::Event::PointerMoved(anchor), button(anchor, true)],
                        vec![egui::Event::PointerMoved(end)],
                    ]);
                }
                "direct-color-settings" => {
                    let (response, _) = self.viewport.pointer(None).unwrap();
                    let position = response.rect.center() + egui::vec2(60., -90.);
                    let button = |button, pressed| egui::Event::PointerButton {
                        pos: position,
                        button,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    };
                    smoke.direct_events.extend([
                        vec![button(egui::PointerButton::Primary, false)],
                        vec![
                            egui::Event::Key {
                                key: egui::Key::D,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: egui::Modifiers::NONE,
                            },
                            egui::Event::Key {
                                key: egui::Key::D,
                                physical_key: None,
                                pressed: false,
                                repeat: false,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                        vec![button(egui::PointerButton::Secondary, true)],
                        vec![button(egui::PointerButton::Secondary, false)],
                    ]);
                }
                "direct-color-exit" => {
                    let escape = vec![
                        egui::Event::Key {
                            key: egui::Key::Escape,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                        egui::Event::Key {
                            key: egui::Key::Escape,
                            physical_key: None,
                            pressed: false,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ];
                    smoke.direct_events.push_back(escape.clone());
                    smoke.direct_events.push_back(escape);
                }
                "exposure-auto-reference"
                | "exposure-auto-universal"
                | "exposure-auto-dial"
                | "exposure-auto-bracket"
                | "exposure-auto-monochrome"
                | "exposure-auto-manual" => {
                    use foveonic::render::{
                        AutoExposureCompensation, BaseResponse, PhotographicMethod, Rendering,
                    };
                    let target = smoke.targets[smoke.index];
                    self.preferences.collapsed =
                        [Section::Navigator, Section::Scopes, Section::Metadata].into();
                    self.preferences
                        .editing_sections
                        .retain(|s| *s != crate::preferences::EditingSection::Tone);
                    self.preferences
                        .editing_sections
                        .insert(0, crate::preferences::EditingSection::Tone);
                    self.settings.rendering = if target == "exposure-auto-monochrome" {
                        Rendering::Monochrome
                    } else {
                        Rendering::Color
                    };
                    self.settings.photographic.tone_method = if target == "exposure-auto-reference"
                    {
                        PhotographicMethod::Reference
                    } else {
                        PhotographicMethod::Universal
                    };
                    self.settings.photographic.base_response = BaseResponse::Natural;
                    self.settings.exposure_auto = target != "exposure-auto-manual";
                    self.settings.exposure = 0.;
                    self.settings.exposure_auto_compensation = match target {
                        "exposure-auto-dial" => AutoExposureCompensation::Dial,
                        "exposure-auto-bracket" => AutoExposureCompensation::Bracket,
                        _ => AutoExposureCompensation::Both,
                    };
                    self.submit(None);
                }
                "perceptual-neutral" => {
                    self.preferences.collapsed =
                        [Section::Navigator, Section::Scopes, Section::Metadata].into();
                    self.settings.photographic.tone_method =
                        foveonic::render::PhotographicMethod::Universal;
                    self.settings.photographic.saturation_method =
                        foveonic::render::PhotographicMethod::Universal;
                    self.submit(None);
                }
                "perceptual-grade" => {
                    self.settings.photographic.brightness = 0.15;
                    self.settings.tonal_ranges.shadows = 0.25;
                    self.settings.tonal_ranges.highlights = -0.6;
                    let c = &mut self.settings.photographic.perceptual;
                    c.saturation = 0.123456;
                    c.contrast = -0.4;
                    c.bands[1].saturation = 0.1;
                    c.bands[2].saturation = -0.2;
                    self.submit(None);
                }
                "perceptual-extended" | "perceptual-unlocked" => {
                    self.settings.photographic.perceptual = Default::default();
                    self.settings.photographic.perceptual.saturation = 2.;
                    self.settings.prevent_color_clipping =
                        smoke.targets[smoke.index] != "perceptual-unlocked";
                    self.preferences
                        .editing_sections
                        .retain(|s| *s != crate::preferences::EditingSection::Saturation);
                    self.preferences
                        .editing_sections
                        .insert(0, crate::preferences::EditingSection::Saturation);
                    self.submit(None);
                }
                "perceptual-mask" => {
                    self.output = "perceptual.midtones".into();
                    self.submit(None);
                }
                "perceptual-editor" => {
                    self.output = "final".into();
                    self.settings.photographic.color_editor.bands[0].saturation = -0.2;
                    self.submit(None);
                }
                "perceptual-mono" => {
                    self.settings.rendering = foveonic::render::Rendering::Monochrome;
                    self.submit(None);
                }
                "comparison-switch" => {
                    self.comparison_selection = self
                        .browser
                        .processing_selection(self.selected.as_deref())
                        .get(1)
                        .cloned();
                    ctx.request_repaint();
                }
                "comparison" => self.browser.set_actions(vec![(
                    crate::shortcuts::BrowserAction::SelectAll,
                    egui::Modifiers::COMMAND,
                )]),
                "workflow-current" => {
                    self.settings.exposure = 0.7;
                    self.settings.photographic.tone_method =
                        foveonic::render::PhotographicMethod::Universal;
                    self.preferences.collapsed =
                        [Section::Navigator, Section::Scopes, Section::Metadata].into();
                    self.viewport.one_to_one = true;
                    self.viewport.pan_to(egui::vec2(0.55, 0.4));
                    self.submit(None);
                }
                "workflow-camera" | "workflow-restored" => {
                    self.toggle_before(crate::editor::Rendition::Camera)
                }
                "workflow-defaults" => {
                    let mut defaults = self.settings.clone();
                    defaults.exposure = 0.2;
                    self.preferences.editor.defaults.set(
                        foveonic::source::ProcessingFamily::Merrill,
                        crate::editor::Scope::Tool(crate::editor::Section::Tone),
                        &defaults,
                    );
                    self.toggle_before(crate::editor::Rendition::Defaults);
                }
                "workflow-restored-again" => self.toggle_before(crate::editor::Rendition::Defaults),
                "workflow-wb" => {
                    self.settings.white_balance_bias.warmth = 0.35;
                    self.settings.white_balance_bias.tint = -0.2;
                    self.submit(None);
                }
                "workflow-default-reset" | "workflow-camera-reset" => {
                    let camera = smoke.targets[smoke.index] == "workflow-camera-reset";
                    self.editor_ui.actions.push(crate::editor::Action::Reset(
                        crate::editor::Scope::Tool(crate::editor::Section::Tone),
                        camera,
                    ));
                    assert!(self.apply_editor_actions(ctx));
                    assert_eq!(self.settings.exposure, if camera { 0. } else { 0.2 });
                    assert_eq!(self.settings.white_balance_bias.warmth, 0.35);
                    self.submit(None);
                }
                "browser-grid" | "browser-list" | "browser-large" => {
                    if smoke.display_revision.is_none() {
                        smoke.display_revision = self.frame.as_ref().map(|frame| frame.revision);
                    }
                    self.preferences.organizer = match smoke.targets[smoke.index] {
                        "browser-grid" => Organizer::Icons,
                        "browser-list" => Organizer::List,
                        _ => Organizer::Large,
                    };
                    self.preferences.browser_sort = crate::browser::Sort {
                        field: crate::browser::Field::Date,
                        reverse: true,
                    };
                    self.browser.set_actions(vec![(
                        crate::shortcuts::BrowserAction::Rate(4),
                        egui::Modifiers::NONE,
                    )]);
                    let id = egui::Id::new("organizer");
                    if let Some(mut state) = egui::containers::panel::PanelState::load(ctx, id) {
                        state.rect.max.x = state.rect.min.x + IMPORT_WIDE_BROWSER_WIDTH;
                        ctx.data_mut(|data| data.insert_persisted(id, state));
                    }
                    self.browser.reveal_selection();
                }
                "gamut-monitor"
                | "gamut-monitor-again"
                | "gamut-adobe"
                | "gamut-srgb"
                | "gamut-romm" => {
                    self.preferences.display_gamut = match smoke.targets[smoke.index] {
                        "gamut-adobe" => crate::preferences::DisplayGamut::AdobeRgb,
                        "gamut-srgb" => crate::preferences::DisplayGamut::Srgb,
                        "gamut-romm" => crate::preferences::DisplayGamut::ProPhotoRgb,
                        _ => crate::preferences::DisplayGamut::Monitor,
                    };
                    crate::viewport::set_gamut(&self.render_state, self.preferences.display_gamut);
                }
                "gamut-settings" => self.settings_open = true,
                "browse-settings" => {
                    let destination = smoke.directory.join("processed");
                    std::fs::create_dir_all(&destination).unwrap();
                    self.preferences.processing_destination = Some(destination.clone());
                    self.import.rescan(&mut self.browser, Some(&destination));
                    self.preferences.browse_processing = Default::default();
                    self.processing_open = true;
                }
                "browse-processed" | "browse-processed-again" => {
                    self.processing_open = false;
                    self.process();
                }
                "import-review" => {
                    let destination = smoke.directory.join("imported");
                    self.open(
                        self.browser.root.clone().unwrap(),
                        crate::preferences::FolderMode::Import,
                    );
                    self.preferences.processing_destination = Some(destination.clone());
                    self.import.smoke_prepare(&destination, &mut self.browser);
                }
                "import-render-options" => {
                    self.preferences.import_processing.copy_only = false;
                    self.processing_open = true;
                    let id = egui::Id::new("organizer");
                    if let Some(mut state) = egui::containers::panel::PanelState::load(ctx, id) {
                        state.rect.max.x = state.rect.min.x + IMPORT_WIDE_BROWSER_WIDTH;
                        ctx.data_mut(|data| data.insert_persisted(id, state));
                    }
                }
                "import-copied" => {
                    self.processing_open = false;
                    self.preferences.import_processing.copy_only = true;
                    self.sidecars.refresh(&self.settings);
                    self.process();
                }
                "readme" => {
                    let recipe = screenshot_recipe();
                    assert_eq!(
                        self.selected
                            .as_ref()
                            .and_then(|path| path.file_name())
                            .and_then(|name| name.to_str()),
                        Some(recipe.source.as_str())
                    );
                    self.settings = recipe.settings;
                    self.preferences.scope = Scope::Waveform;
                    self.preferences.scope_viewport = true;
                    self.browser.reveal_selection();
                    self.viewport.zoom_by(recipe.zoom);
                    self.viewport
                        .pan_to(egui::vec2(recipe.pan[0], recipe.pan[1]));
                    self.submit(None);
                }
                "final" if smoke.targets == SMART_TARGETS => {
                    self.preferences.preview_quality = crate::preferences::PreviewMode::Full;
                    self.submit(None);
                }
                "final" => {}
                "smart-overview" => {
                    self.preferences.preview_quality = crate::preferences::PreviewMode::Smart;
                    self.preferences.scope_viewport = true;
                    self.settings.reconstruction = Some(Default::default());
                    self.submit(None);
                }
                "smart-100" => self.viewport.one_to_one = true,
                "smart-detail" => self.viewport.zoom_by(2.),
                "smart-full" => {}
                "smart-pan" => self.viewport.pan_to(egui::vec2(0.8, 0.7)),
                "smart-edit" => {
                    self.settings.exposure = 0.5;
                    self.submit(None);
                }
                "smart-fit" => self.viewport.reset(),
                "cache-settings" => self.settings_open = true,
                "cache-list" => {
                    self.settings_open = false;
                    self.preferences.organizer = Organizer::List;
                }
                "cache-reopen" => {
                    self.settings_open = false;
                    self.select(self.selected.clone().unwrap(), false);
                }
                "fast" | "fast-again" => {
                    self.preferences.preview_quality = crate::preferences::PreviewMode::Fast;
                    self.preferences.scope_viewport = true;
                    self.submit(None);
                }
                "full-again" => {
                    self.preferences.preview_quality = crate::preferences::PreviewMode::Full;
                    self.submit(None);
                }
                "fast-zoom" => {
                    self.viewport.zoom_by(3.);
                    self.viewport.pan_to(egui::vec2(0.6, 0.4));
                }
                "fast-edit" => {
                    self.settings.exposure = 0.5;
                    self.submit(None);
                }
                "viewport-fit" => self.preferences.scope_viewport = true,
                "viewport-zoom" => self.viewport.zoom_by(4.),
                "viewport-pan" => self.viewport.pan_to(egui::vec2(0.7, 0.3)),
                "viewport-parade" => self.preferences.scope = Scope::Parade,
                "viewport-waveform" => self.preferences.scope = Scope::Waveform,
                "viewport-resize" => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1200., 800.)))
                }
                "viewport-whole" => self.preferences.scope_viewport = false,
                "display-wide" | "display-narrow" => {
                    let id = egui::Id::new("settings");
                    let mut panel = egui::containers::panel::PanelState::load(ctx, id).unwrap();
                    let width = if smoke.targets[smoke.index] == "display-wide" {
                        420.
                    } else {
                        300.
                    };
                    panel.rect.min.x = panel.rect.max.x - width;
                    ctx.data_mut(|data| data.insert_persisted(id, panel));
                }
                "viewport-sensor" => {
                    self.preferences.scope_viewport = true;
                    self.output = "sensor.layers".into();
                    self.submit(None);
                }
                "viewport-fit-again" => self.viewport.reset(),
                "diff-layer" => {
                    self.output = "repair.layers".into();
                    self.channel = 0;
                    self.comparison.difference = true;
                    self.comparison.gain = 8.;
                    self.submit(None);
                }
                "diff-amplified" => self.comparison.gain = 32.,
                "diff-layer-two" => self.channel = 1,
                "diff-output" => self.comparison.difference = false,
                "compare-input" => self.comparison.input = true,
                "compare-output" => self.comparison.input = false,
                "diff-rgb" => {
                    self.settings.saturation = 1.5;
                    self.output = "saturation.rgb".into();
                    self.channel = 2;
                    self.comparison.difference = true;
                    self.comparison.gain = 8.;
                    self.submit(None);
                }
                "compare-rgb-input" => {
                    self.channel = -1;
                    self.comparison.difference = false;
                    self.comparison.input = true;
                }
                "compare-rgb-output" => self.comparison.input = false,
                "diff-final" => {
                    self.output = "final".into();
                    self.channel = -1;
                    self.comparison.difference = false;
                    self.settings.saturation = 1.;
                    self.submit(None);
                }
                name if name.starts_with("slide-detail-") => {
                    self.settings.reconstruction.as_mut().unwrap().texture.fine =
                        name.strip_prefix("slide-detail-").unwrap().parse().unwrap();
                    self.submit(None);
                }
                "leaf-before" | "leaf-after" => {
                    let enabled = smoke.targets[smoke.index] == "leaf-after";
                    self.settings.repair =
                        enabled.then_some(foveonic::render::RepairThreshold::Camera);
                    self.settings.despeckle = enabled;
                    self.settings.reconstruction = None;
                    self.output = "final".into();
                    self.channel = -1;
                    self.viewport.one_to_one = false;
                    if !enabled {
                        self.viewport.zoom_by(48.);
                    }
                    self.viewport
                        .pan_to(egui::vec2(1582. / 3136., 2737. / 4704.));
                    self.submit(None);
                }
                "camera-only" => {
                    self.settings.reconstruction = Some(foveonic::noise::Controls {
                        chroma: 0,
                        luminance: 0,
                        ..Default::default()
                    });
                    self.settings.despeckle = false;
                    self.viewport.one_to_one = true;
                    self.submit(None);
                }
                "bolt-before" | "bolt-after" => {
                    self.output = "final".into();
                    self.channel = -1;
                    self.settings.despeckle = smoke.targets[smoke.index] == "bolt-after";
                    if smoke.targets[smoke.index] == "bolt-before" {
                        self.viewport.zoom_by(4.);
                    }
                    self.viewport.pan_to(egui::vec2(0.79, 0.05));
                    self.submit(None);
                }
                "white-before" | "white-after" => self.viewport.pan_to(egui::vec2(0.29, 0.757)),
                "final-disabled" | "final-default" => {
                    let enabled = smoke.targets[smoke.index] == "final-default";
                    self.settings.despeckle = enabled;
                    self.settings.repair =
                        enabled.then_some(foveonic::render::RepairThreshold::Camera);
                    self.settings.reconstruction = None;
                    self.output = "final".into();
                    self.channel = -1;
                    self.viewport.reset();
                    self.submit(None);
                }
                "reconstructed" => {
                    if editor_review_active() {
                        self.preferences.collapsed = [Section::Navigator, Section::Scopes].into();
                    }
                    self.settings.reconstruction = Some(Default::default());
                    self.submit(None);
                }
                "fine-soft" => {
                    self.settings.reconstruction.as_mut().unwrap().texture.fine = 0.5;
                    self.submit(None);
                }
                "coarse-large" => {
                    let controls = self.settings.reconstruction.as_mut().unwrap();
                    controls.texture.coarse = 0.5;
                    controls.texture.coarse_size = 2.;
                    self.submit(None);
                }
                "neutral-detail" => {
                    self.settings.reconstruction.as_mut().unwrap().texture = Default::default();
                    self.submit(None);
                }
                "rgb-blue" => self.channel = 2,
                "layers-two" => {
                    self.output = "noise.layers".into();
                    self.channel = 1;
                    self.submit(None);
                }
                "repair-auto" | "repair-zero" => {
                    self.settings.reconstruction = Some(Default::default());
                    self.settings.repair = Some(if smoke.targets[smoke.index] == "repair-auto" {
                        foveonic::render::RepairThreshold::Camera
                    } else {
                        foveonic::render::RepairThreshold::Manual(0)
                    });
                    self.output = "final".into();
                    self.channel = -1;
                    self.submit(None);
                }
                "repair-zoom" => self.viewport.one_to_one = true,
                "bolt" => {
                    self.viewport.zoom_by(4.);
                    self.viewport.pan_to(egui::vec2(0.79, 0.05));
                }
                "white-edge" => self.viewport.pan_to(egui::vec2(0.335, 0.75)),
                "detail-zero" | "noise-low" | "noise-high" | "color-dq" | "color-dq-off"
                | "final-neutral" => {
                    let mut controls = foveonic::noise::Controls::default();
                    match smoke.targets[smoke.index] {
                        "detail-zero" => {
                            controls.texture.fine = 0.;
                            controls.texture.coarse = 0.;
                        }
                        "noise-low" => {
                            controls.chroma = 0;
                            controls.luminance = 0;
                        }
                        "noise-high" => {
                            controls.chroma = 4;
                            controls.luminance = 4;
                        }
                        "color-dq" => self.settings.color_dq_gain = Some(1.),
                        "color-dq-off" => self.settings.color_dq_gain = Some(0.),
                        "final-neutral" => {
                            self.settings.repair = None;
                            self.settings.despeckle = false;
                            self.channel = -1;
                        }
                        _ => unreachable!(),
                    }
                    self.settings.reconstruction =
                        (smoke.targets[smoke.index] != "final-neutral").then_some(controls);
                    self.output = "final".into();
                    self.viewport.reset();
                    self.submit(None);
                }
                "list" => {
                    self.preferences.organizer = Organizer::List;
                    self.output = "final".into();
                    self.submit(None);
                }
                "parade" => self.preferences.scope = Scope::Parade,
                "waveform" => self.preferences.scope = Scope::Waveform,
                "zoom" => self.viewport.one_to_one = true,
                "reordered" => {
                    self.viewport.pan_to(egui::vec2(0.65, 0.5));
                    self.preferences
                        .move_section(Section::Metadata, Section::Navigator, false);
                }
                "collapsed" => self.preferences.toggle(Section::Editing),
                name => {
                    self.viewport.reset();
                    let last_folder = self.preferences.last_folder.clone();
                    self.preferences = Default::default();
                    self.preferences.last_folder = last_folder;
                    self.output = name.into();
                    self.submit(None);
                }
            }
        } else if !smoke.awaiting {
            let ready = if smoke.preview_pending() {
                self.selected
                    .as_ref()
                    .is_some_and(|p| self.browser.thumbnail(p).is_some())
            } else if smoke.targets == CROP_TARGETS
                && smoke.targets[smoke.index] == "crop-tile"
                && regional_smart
            {
                self.detail.is_some()
            } else if smoke.targets == DEHAZE_TARGETS {
                // These controls require the current photo, independently of ancillary panels.
                !self.busy
                    && self.frame.as_ref().is_some_and(|frame| {
                        self.selected.as_ref() == Some(&frame.path)
                            && frame.output == self.output
                            && frame.revision == self.revision
                    })
            } else if smoke.targets == SMART_TARGETS
                && smoke.targets[smoke.index] == "smart-detail"
                && regional_smart
            {
                self.detail.is_some()
            } else {
                !self.busy
                    && self.frame.as_ref().is_some_and(|frame| {
                        self.scope_snapshot
                            .as_ref()
                            .is_some_and(|snapshot| Some(snapshot.key) == self.scope_key)
                            && self.navigator.ready(frame, mapping)
                            && (frame.rendition != crate::editor::Rendition::Current
                                || frame.output != "final"
                                || frame.artifact.settings.color_range_preview.is_some()
                                || frame.crop_preview
                                || self.browser.rendered_revision(&frame.path)
                                    == Some(frame.revision))
                    })
            };
            let ready =
                ready && (smoke.targets != SCREENSHOT_TARGETS || self.browser.thumbnails_ready());
            let ready = ready
                && (smoke.targets != BROWSER_TARGETS
                    || (self.browser.thumbnails_ready() && self.browser.metadata_ready()));
            let ready = ready
                && (smoke.targets != IMPORT_TARGETS
                    || match smoke.targets[smoke.index] {
                        "import-review" | "import-render-options" => self.import.smoke_ready(false),
                        "import-copied" => self.import.smoke_ready(true),
                        _ => true,
                    });
            let ready = ready
                && (smoke.targets != SMART_TARGETS
                    || match smoke.targets[smoke.index] {
                        "smart-detail" if regional_smart => self.detail.is_some(),
                        "smart-detail" => self.frame.as_ref().is_some_and(|f| {
                            f.artifact.settings.preview_quality
                                == foveonic::render::PreviewQuality::Full
                        }),
                        "smart-full" | "smart-edit" => self.frame.as_ref().is_some_and(|f| {
                            f.artifact.settings.preview_quality
                                == foveonic::render::PreviewQuality::Full
                        }),
                        "cache-settings" => self
                            .worker
                            .cache
                            .status()
                            .ready
                            .iter()
                            .any(|p| Some(p) != self.selected.as_ref()),
                        _ => true,
                    });
            let ready = ready
                && (smoke.targets != DISPLAY_COLOR_TARGETS
                    || smoke.preview_pending()
                    || self.frame.as_ref().is_some_and(|frame| {
                        self.browser.rendered_revision(&frame.path) == Some(frame.revision)
                    }));
            let ready = ready
                && (smoke.targets != ADVANCED_COLOR_TARGETS
                    || smoke.targets[smoke.index] != "advanced-picked"
                    || (self.settings.photographic.color_editor.advanced.count == 1
                        && self.settings.photographic.color_editor.advanced.ranges[0]
                            .adjustment
                            .hue
                            > 0.
                        && self.settings.photographic.color_editor.advanced.ranges[0]
                            .adjustment
                            .lightness
                            > 0.
                        && !crate::editor::direct_color::active(ctx)
                        && !crate::editor::direct_color::dragging(ctx)));
            let ready = ready
                && (smoke.targets != ADVANCED_COLOR_TARGETS
                    || smoke.targets[smoke.index] != "advanced-name"
                    || (self.settings.photographic.color_editor.advanced.ranges[0]
                        .name
                        .as_str()
                        == "Petals"
                        && crate::editor::advanced_color::state(ctx).rename.is_none()
                        && smoke.direct_events.is_empty()));
            if ready
                && smoke.targets == ADVANCED_COLOR_TARGETS
                && smoke.targets[smoke.index] == "advanced-name"
            {
                assert_eq!(
                    smoke.name_revision,
                    Some(self.revision),
                    "renaming must not submit an image job"
                );
            }
            let ready = ready
                && (smoke.targets != DEHAZE_TARGETS
                    || smoke.targets[smoke.index] != "dehaze-picked"
                    || (self.settings.dehaze.shadow.is_some()
                        && !crate::editor::dehaze::active(ctx)
                        && !crate::editor::dehaze::pending(ctx)
                        && smoke.direct_events.is_empty()));
            let ready = ready
                && (smoke.targets != DIRECT_COLOR_TARGETS
                    || match smoke.targets[smoke.index] {
                        "direct-color-ready" => crate::editor::direct_color::active(ctx),
                        "direct-color-edited" => self
                            .settings
                            .photographic
                            .color_editor
                            .bands
                            .iter()
                            .any(|b| b.lightness > 0. && b.hue > 0.),
                        "direct-color-settings" => crate::editor::direct_color::menu_open(ctx),
                        "direct-color-exit" => !crate::editor::direct_color::active(ctx),
                        _ => true,
                    });
            let ready = ready
                && (smoke.targets != CROP_TARGETS
                    || (smoke.direct_events.is_empty()
                        && match smoke.targets[smoke.index] {
                            "crop-ready" | "crop-drag" => {
                                crate::crop_tool::active(ctx)
                                    && self.frame.as_ref().is_some_and(|f| f.crop_preview)
                            }
                            "crop-applied" | "crop-reopen" => {
                                !self.settings.crop.is_full()
                                    && self.frame.as_ref().is_some_and(|f| f.crop_preview)
                            }
                            "crop-hand" => {
                                !crate::crop_tool::active(ctx)
                                    && self.frame.as_ref().is_some_and(|f| !f.crop_preview)
                            }
                            "crop-fast" => self.frame.as_ref().is_some_and(|f| {
                                !f.crop_preview
                                    && f.revision == self.revision
                                    && (!f
                                        .artifact
                                        .settings
                                        .fast_preview_available(&f.artifact.metadata)
                                        || f.artifact.settings.preview_quality
                                            == foveonic::render::PreviewQuality::Fast)
                            }),
                            "crop-smart" => self
                                .frame
                                .as_ref()
                                .is_some_and(|f| f.revision == self.revision && !f.crop_preview),
                            "crop-tile" => {
                                if regional_smart {
                                    self.detail.is_some()
                                } else {
                                    self.frame.as_ref().is_some_and(|f| {
                                        f.artifact.settings.preview_quality
                                            == foveonic::render::PreviewQuality::Full
                                    })
                                }
                            }
                            "crop-detail" => self.frame.as_ref().is_some_and(|f| {
                                f.revision == self.revision
                                    && f.artifact.settings.preview_quality
                                        == foveonic::render::PreviewQuality::Full
                            }),
                            _ => true,
                        }));
            let ready =
                ready && (smoke.targets != STATUS_TARGETS || smoke.direct_events.is_empty());
            if ready {
                if smoke.targets == DIRECT_COLOR_TARGETS
                    && smoke.targets[smoke.index] == "direct-color-settings"
                {
                    smoke.settled_frames += 1;
                    if smoke.settled_frames < 8 {
                        ctx.request_repaint_after(std::time::Duration::from_millis(20));
                        return;
                    }
                }
                if smoke.targets == SMART_TARGETS && smoke.targets[smoke.index] == "cache-settings"
                {
                    smoke.settled_frames += 1;
                    if smoke.settled_frames < 8 {
                        ctx.request_repaint_after(std::time::Duration::from_millis(25));
                        return;
                    }
                }
                if smoke.targets == SCREENSHOT_TARGETS && !smoke.preview_pending() {
                    // Let the selected organizer row and its newly visible thumbnails settle.
                    smoke.settled_frames += 1;
                    if smoke.settled_frames < 3 {
                        ctx.request_repaint();
                        return;
                    }
                }
                smoke.awaiting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            } else {
                smoke.settled_frames = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readme_recipe_has_fixed_valid_source_view_and_photographic_settings() {
        let recipe = screenshot_recipe();
        assert_eq!(recipe.source, "yellow-flowers-and-stroller.x3f");
        assert_eq!(recipe.window_size, [1325., 1097.]);
        assert_eq!(recipe.organizer_width, 264.);
        assert_eq!(recipe.tools_width, 330.);
        assert!(recipe.zoom.is_finite() && recipe.zoom >= 1.);
        assert!(recipe.pan.iter().all(|value| (0. ..=1.).contains(value)));
        recipe.settings.validate().unwrap();
        assert_eq!(recipe.settings.white_balance, "Sunlight");
        assert_eq!(recipe.settings.color_mode, "Standard");
        assert!(recipe.settings.reconstruction.is_some());
        assert_eq!(recipe.settings.highlight_threshold_ev, Some(0.71));
        assert_eq!(recipe.settings.tone.highlights, 0.);
        assert_eq!(SCREENSHOT_TARGETS, ["preview", "readme"]);
    }
}
