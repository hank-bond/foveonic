//! Captures opt-in native workspace checks and the recipe-driven README image using isolated real GPU frames.

use crate::rendering::request::RequestId;
use crate::{
    preferences::{Organizer, Scope, Section},
    App,
};
use eframe::egui;
use foveonic::render::{ProcessingQuality, Settings};
use std::path::PathBuf;

mod background;
mod browser_journey;
mod cache_workflow;
mod capture;
mod choices;
mod collection;
mod damaged;
mod journey;
mod lifecycle;
mod loupe;
mod motion;
mod multi_edit;
mod rotate;
mod scenarios;
mod screenshot;
mod selection;
mod source_loading;
pub use screenshot::{screenshot_active, screenshot_recipe};
mod slider;
mod transitions;

/// Check display demand from observed geometry, independently of the application's resolver.
fn automatic_scale_bound(
    frame: &crate::rendering::worker::RenderResult,
    view: &crate::viewport::Viewport,
    policy: ProcessingQuality,
    pixels_per_point: f32,
) -> Result<foveonic::render::Scale, String> {
    let (_, image) = view
        .image_rectangles()
        .ok_or("missing image display bounds")?;
    let extent = frame.native_display_extent();
    let density = (image.width() / extent[0] as f32).max(image.height() / extent[1] as f32)
        * pixels_per_point;
    let limit = match policy {
        ProcessingQuality::Quality => 1.1,
        ProcessingQuality::Speed => 1.2,
    };
    let divisor = [8, 4, 2, 1]
        .into_iter()
        .find(|divisor| density * *divisor as f32 <= limit)
        .unwrap_or(1);
    if divisor == 8 {
        foveonic::render::ControlCapabilities::from_metadata(&frame.artifact.metadata)
            .map(|capabilities| capabilities.thumb_scale())
            .map_err(|error| error.to_string())
    } else {
        Ok(foveonic::render::Scale::new(divisor).unwrap())
    }
}

/// Scope coordinates belong to the complete visible source, not its retained overview.
fn covers_visible(region: [u32; 4], visible: Option<[u32; 4]>) -> bool {
    visible.is_some_and(|visible| {
        region[0] <= visible[0]
            && region[1] <= visible[1]
            && region[2] >= visible[2]
            && region[3] >= visible[3]
    })
}

fn scope_frame<'a>(
    detail: Option<&'a crate::rendering::worker::RenderResult>,
    navigator: Option<&'a crate::rendering::worker::RenderResult>,
    visible: Option<[u32; 4]>,
    viewport: bool,
) -> Option<&'a crate::rendering::worker::RenderResult> {
    detail
        .filter(|frame| viewport && covers_visible(frame.detail_region(), visible))
        .or(navigator)
}

fn scope_region(
    frame: &crate::rendering::worker::RenderResult,
    view: &crate::viewport::Viewport,
    visible: Option<[u32; 4]>,
    viewport: bool,
) -> Option<[u32; 4]> {
    if !viewport {
        return None;
    }
    if let Some(region) = frame.artifact.settings.preview_region {
        let visible = visible.expect("visible native bounds for regional scopes");
        assert!(covers_visible(region, Some(visible)));
        return Some([
            visible[0] - region[0],
            visible[1] - region[1],
            visible[2] - region[0],
            visible[3] - region[1],
        ]);
    }
    // Keep this rounding oracle independent of the production pixel-region helper.
    let uv = view.visible_uv();
    let [width, height] = frame.texture.description().extent;
    Some([
        (uv.min.x.clamp(0., 1.) * width as f32).floor() as u32,
        (uv.min.y.clamp(0., 1.) * height as f32).floor() as u32,
        (uv.max.x.clamp(0., 1.) * width as f32).ceil() as u32,
        (uv.max.y.clamp(0., 1.) * height as f32).ceil() as u32,
    ])
}

const CROP_TARGETS: &[&str] = &[
    "preview",
    "final",
    "crop-ready",
    "crop-drag",
    "crop-applied",
    "crop-pointer",
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
    "smart-edit-detail",
    "smart-edit",
    "smart-zoom-pending",
    "smart-crop-zoom",
    "smart-crop-enter",
    "smart-crop-active-zoom",
    "smart-crop-leave",
    "smart-transition-full",
    "smart-adaptive-zoom",
    "smart-adaptive-edit",
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

const WORKFLOW_TARGETS: &[&str] = &[
    "preview",
    "final",
    "workflow-current",
    "workflow-camera",
    "workflow-restored",
    "workflow-navigator-open",
    "workflow-camera-open",
    "workflow-restored-open",
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
const WHITE_BALANCE_TARGETS: &[&str] = &[
    "preview",
    "final",
    "white-balance-ready",
    "white-balance-picked",
    "white-balance-reset",
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
    "white-background",
    "browse-settings",
    "browse-processed",
    "browse-processed-again",
    "import-open",
    "import-review",
    "import-render-options",
    "import-copied",
];

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

struct SmartReopen {
    texture: std::sync::Weak<foveonic::image::ImageTexture>,
    task: crate::timing::DeliveredTask,
    scale: foveonic::render::Scale,
    thumbnail_revision: Option<u64>,
}

struct ZoomRetention {
    texture: std::sync::Weak<foveonic::image::ImageTexture>,
    task: crate::timing::DeliveredTask,
}
impl ZoomRetention {
    fn capture(frame: &crate::rendering::worker::RenderResult) -> Self {
        Self {
            texture: std::sync::Arc::downgrade(&frame.texture),
            task: frame.diagnostic_task,
        }
    }
    fn check(&self, frame: &crate::rendering::worker::RenderResult) {
        let texture = self
            .texture
            .upgrade()
            .expect("zoom discarded compatible Full pixels");
        assert!(
            std::sync::Arc::ptr_eq(&texture, &frame.texture),
            "pure zoom replaced compatible Full pixels"
        );
        assert_eq!(
            frame.diagnostic_task, self.task,
            "pure zoom replaced the compatible Full task"
        );
    }
}

pub struct Smoke {
    scenario: Option<scenarios::Scenario>,
    capture: capture::Capture,
    directory: PathBuf,
    targets: &'static [&'static str],
    index: usize,
    step_started: Option<(usize, std::time::Instant)>,
    awaiting: bool,
    scope_capture: Option<(crate::rendering::worker::ScopeKey, [usize; 2])>,
    direct_events: std::collections::VecDeque<Vec<egui::Event>>,
    pointer_motion: Option<motion::Motion>,
    close_at: Option<std::time::Instant>,
    settled_frames: u8,
    fast_region: Option<crate::viewport::PixelRegion>,
    smart_full_revision: Option<u64>,
    zoom_retained: Option<ZoomRetention>,
    smart_scrub_checked: bool,
    smart_reopen: Option<SmartReopen>,
    display_revision: Option<u64>,
    authored: Option<foveonic::render::Settings>,
    name_revision: Option<u64>,
    current_texture: Option<std::sync::Arc<foveonic::image::ImageTexture>>,
    multi_paths: Vec<PathBuf>,
    multi_expected: Vec<foveonic::render::Settings>,
    /// The multi-photo arrangement before a crop made with the tool armed.
    multi_tiles: Vec<egui::Rect>,
    multi_cached_textures: Vec<std::sync::Weak<foveonic::image::ImageTexture>>,
    multi_cached_scales: Vec<foveonic::render::Scale>,
    multi_sweep_groups: usize,
    multi_sweep_started: Option<std::time::Instant>,
    multi_sweep_ms: Vec<f64>,
    /// A pick step's press and release, sent once the hover frame shows the pick cursor.
    multi_pick_events: Option<Vec<Vec<egui::Event>>>,
    /// The current pick step and its evidence.
    multi_pick: Option<multi_edit::Pick>,
    /// Where the Exposure row sat at the first compared step.
    multi_exposure_top: Option<f32>,
}
impl Smoke {
    pub(crate) fn observe_admitted(
        &mut self,
        frame: &crate::rendering::worker::RenderResult,
        current: bool,
        visible: Option<[u32; 4]>,
    ) {
        if let Some(scenario) = &mut self.scenario {
            scenario.observe_admitted(frame, current, visible);
        }
    }
    pub(crate) fn expects_failure(&self, path: &std::path::Path) -> bool {
        self.scenario
            .as_ref()
            .is_some_and(|scenario| scenario.expects_failure(path))
    }
    pub fn inject_direct_color(&mut self, input: &mut egui::RawInput, ctx: &egui::Context) {
        if self.targets == WHITE_BALANCE_TARGETS
            || self.targets == DIRECT_COLOR_TARGETS
            || self.targets == DEHAZE_TARGETS
            || self.targets == ADVANCED_COLOR_TARGETS
            || self.targets == CROP_TARGETS
            || self.targets == rotate::TARGETS
            || self.targets == STATUS_TARGETS
            || self.targets == multi_edit::TARGETS
            || self.scenario.is_some()
        {
            // Inject focused input without moving the owner's system pointer or focus.
            input.focused = true;
        }
        if let Some(events) = self.direct_events.pop_front() {
            if self.targets == multi_edit::TARGETS || self.scenario.is_some() {
                input.modifiers = events
                    .iter()
                    .find_map(|event| match event {
                        egui::Event::PointerButton { modifiers, .. }
                        | egui::Event::MouseWheel { modifiers, .. } => Some(*modifiers),
                        _ => None,
                    })
                    .unwrap_or_default();
            }
            input.events.push(egui::Event::WindowFocused(true));
            input.events.extend(events);
            ctx.request_repaint();
        }
        if let Some(motion) = &mut self.pointer_motion {
            motion.inject(input, ctx);
        }
    }
    pub fn from_env() -> Option<Self> {
        std::env::var_os("FOVEONIC_GUI_SMOKE_DIR").map(|directory| Self {
            scenario: scenarios::Scenario::from_env(),
            capture: Default::default(),
            directory: directory.into(),
            targets: if std::env::var_os("FOVEONIC_GUI_SMOKE_WHITE_BALANCE").is_some() {
                WHITE_BALANCE_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_STATUS").is_some() {
                STATUS_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_ROTATE").is_some() {
                rotate::TARGETS
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
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_MULTI").is_some() {
                multi_edit::TARGETS
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
                screenshot::TARGETS
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
            step_started: None,
            awaiting: false,
            scope_capture: None,
            direct_events: Default::default(),
            pointer_motion: None,
            close_at: None,
            settled_frames: 0,
            fast_region: None,
            smart_full_revision: None,
            zoom_retained: None,
            smart_scrub_checked: false,
            smart_reopen: None,
            display_revision: None,
            authored: None,
            name_revision: None,
            current_texture: None,
            multi_paths: Vec::new(),
            multi_expected: Vec::new(),
            multi_tiles: Vec::new(),
            multi_cached_textures: Vec::new(),
            multi_cached_scales: Vec::new(),
            multi_sweep_groups: std::env::var("FOVEONIC_GUI_SMOKE_MULTI_SWEEP")
                .ok()
                .map(|value| {
                    value
                        .parse()
                        .expect("multi-edit sweep must be a nonnegative integer")
                })
                .unwrap_or(0),
            multi_sweep_started: None,
            multi_sweep_ms: Vec::new(),
            multi_pick_events: None,
            multi_pick: None,
            multi_exposure_top: None,
        })
    }
    pub(super) fn configure_cache(&self, settings: &mut foveonic::decode_cache::Settings) {
        if let Some(scenario) = &self.scenario {
            scenario.configure_cache(settings);
        } else if let Some(bytes) = std::env::var_os("FOVEONIC_GUI_SMOKE_DISK_CACHE_BYTES") {
            let bytes = bytes
                .to_str()
                .and_then(|value| value.parse::<u64>().ok())
                .expect("smoke disk cache budget must be an unsigned byte count");
            settings.enabled = bytes > 0;
            settings.max_bytes = bytes;
        }
    }
    /// Hold processing replies until the embedded preview has been captured.
    pub fn preview_pending(&self) -> bool {
        self.scenario.is_none() && self.index == 0
    }
}

impl App {
    /// The image answers its photo's current view. Ids are issued per image in
    /// use across photos, so the newest id need not be this photo's, and a stored
    /// image the planner finds current keeps no coordinator task to admit it.
    fn shows_current_view(&self, frame: &crate::rendering::worker::RenderResult) -> bool {
        self.view_images
            .view(&frame.request.path)
            .is_some_and(|view| {
                if frame.request.detail {
                    crate::rendering::view_plan::Rendered::matches(frame, &view.request)
                } else {
                    crate::rendering::view_plan::whole_current(frame, &view.request)
                }
            })
    }
    fn smoke_check_deadline(&mut self) {
        let Some(smoke) = &mut self.smoke else {
            return;
        };
        if smoke.scenario.is_some()
            || smoke.close_at.is_some()
            || smoke.index >= smoke.targets.len()
        {
            return;
        }
        let started = smoke
            .step_started
            .get_or_insert((smoke.index, std::time::Instant::now()));
        if started.0 != smoke.index {
            *started = (smoke.index, std::time::Instant::now());
        }
        if started.1.elapsed() < std::time::Duration::from_secs(60) {
            return;
        }
        let target = smoke.targets[smoke.index];
        let mut state = self.diagnostic_state();
        state["smoke_readiness"] = serde_json::json!({
            "target":target,"app_busy":self.busy,"opening":self.opening.is_some(),
            "selection_loaded":self.selection_loaded(),
            "scope_key":self.scope_key.map(|key|(key.request_id,key.region)),
            "scope_snapshot":self.scope_snapshot.as_ref().map(|snapshot|(snapshot.key.request_id,snapshot.key.region)),
            "navigator":self.navigator.smoke_state(self.navigator_source(),self.displayed_mapping()),
            "frame":self.active_image().map(|frame|serde_json::json!({
                "matches_settings":self.matches_selected(&frame.request.settings),
                "current_view":self.shows_current_view(frame),
                "navigator_ready":self.navigator_source().is_some_and(|source|self.navigator.ready(source,self.displayed_mapping())),
                "thumbnail_revision":self.browser.rendered_revision(&frame.request.path),
            })),
        });
        panic!("smoke step {target} did not finish within 60 seconds: {state}");
    }

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

    /// The selected photo's settings. Harness steps run once they have loaded.
    pub(crate) fn smoke_recipe(&self) -> &Settings {
        self.selected_settings()
            .expect("smoke steps wait for loaded settings")
    }
    /// Whether a request rendered the selected photo's loaded edits.
    fn matches_selected(&self, requested: &Settings) -> bool {
        self.selected_settings().is_some_and(|settings| {
            crate::rendering::matching::same_authored_edits(requested, settings)
        })
    }
    /// Author a scenario's change to the selected photo's recipe through the
    /// editing store, as a person editing would.
    pub(crate) fn smoke_edit(&mut self, change: impl FnOnce(&mut Settings)) {
        let mut recipe = self.smoke_recipe().clone();
        change(&mut recipe);
        self.edit_selected(crate::multi_edit_runtime::Change::Replace(Box::new(recipe)));
    }
    pub(super) fn smoke_tick(&mut self, ctx: &egui::Context) {
        self.smoke_check_deadline();
        if self
            .smoke
            .as_ref()
            .is_some_and(|smoke| smoke.scenario.is_none())
        {
            // Direct scripted actions also need another tick when no worker
            // or physical pointer event will wake an otherwise idle window.
            ctx.request_repaint_after(std::time::Duration::from_millis(20));
        }
        if self.smoke.is_none() || self.smoke_capture_tick(ctx) {
            return;
        }
        let _smoke_scope = self
            .smoke
            .as_ref()
            .and_then(|smoke| smoke.scenario.is_none().then(|| smoke.diagnostic_scope()));
        if self
            .smoke
            .as_ref()
            .is_some_and(|smoke| smoke.scenario.is_some())
        {
            let mut smoke = self.smoke.take().unwrap();
            let mut scenario = smoke.scenario.take().unwrap();
            scenario.tick(self, ctx, &mut smoke);
            smoke.scenario = Some(scenario);
            self.smoke = Some(smoke);
            return;
        }
        if self
            .smoke
            .as_ref()
            .is_some_and(|smoke| smoke.targets == multi_edit::TARGETS)
        {
            self.smoke_multi_tick(ctx);
            return;
        }
        let mapping = self.displayed_mapping();
        let quattro_smart = self.active_image().is_some_and(|frame| {
            frame.artifact.metadata.processing_family().ok()
                == Some(foveonic::source::ProcessingFamily::Quattro)
        });
        let regional_smart = self.active_image().is_some_and(|frame| {
            frame
                .artifact
                .settings
                .regional_preview_available(&frame.artifact.metadata)
        });
        let shown_detail = self.active_patch().map(|detail| {
            (
                detail.request.framing_preview,
                self.matches_selected(&detail.request.settings),
            )
        });
        if self.smoke.as_ref().is_some_and(|smoke| {
            smoke.targets == SMART_TARGETS
                && smoke.index < smoke.targets.len()
                && smoke.targets[smoke.index] == "smart-edit-detail"
                && !smoke.smart_scrub_checked
        }) && self
            .active_patch()
            .or(self.active_image())
            .is_some_and(|frame| {
                frame.request.settings.exposure == 0.5
                    && frame.request.rendition == crate::editor::Rendition::Current
                    && frame.request.output == "final"
                    && self.selected.as_ref() == Some(&frame.request.path)
                    && frame.artifact.settings.working_scale == foveonic::render::Scale::NATIVE
                    && self.matches_selected(&frame.request.settings)
            })
        {
            let feedback = self.active_patch().or(self.active_image()).unwrap();
            let revision = feedback.request.id;
            let task = feedback.diagnostic_task;
            let content_revision = feedback.request.id;
            let texture = feedback.texture.clone();
            let requested = feedback.request.settings.clone();
            let regional = feedback.artifact.settings.preview_region.is_some();
            // Either the regional result or its whole-image continuation may already be visible.
            // Changing settings and submitting must preserve the exact completed feedback.
            for exposure in [0.625, 0.75] {
                self.smoke_edit(|recipe| recipe.exposure = exposure);
                for submitted in [false, true] {
                    if submitted {
                        self.submit_photos();
                    }
                    let retained = self
                        .active_patch()
                        .or(self.active_image())
                        .expect("editing hid the latest completed feedback");
                    assert_eq!(retained.request.id, revision);
                    assert_eq!(retained.diagnostic_task, task);
                    assert_eq!(retained.request.id, content_revision);
                    assert_eq!(retained.request.settings, requested);
                    assert!(std::sync::Arc::ptr_eq(&retained.texture, &texture));
                }
            }
            let smoke = self.smoke.as_mut().unwrap();
            smoke.smart_scrub_checked = true;
            std::fs::write(smoke.directory.join("smart-scrub-retained.json"), serde_json::to_vec_pretty(&serde_json::json!({"retained_revision":revision,"retained_content_revision":content_revision,"retained_exposure":0.5,"requested_exposure":0.75,"requested_revision":self.view_images.last_request_id(),"retained_kind":if regional {"region"} else {"whole"}})).unwrap()).unwrap();
        }
        let current_native_feedback =
            self.active_patch()
                .or(self.active_image())
                .is_some_and(|frame| {
                    self.selected.as_ref() == Some(&frame.request.path)
                        && frame.request.output == "final"
                        && frame.request.rendition == crate::editor::Rendition::Current
                        && !frame.request.framing_preview
                        && frame.comparison.is_none()
                        && frame.artifact.settings.working_scale == foveonic::render::Scale::NATIVE
                        && self.matches_selected(&frame.request.settings)
                });
        let current_native_whole = self.active_image().is_some_and(|frame| {
            self.selected.as_ref() == Some(&frame.request.path)
                && frame.request.output == "final"
                && frame.request.rendition == crate::editor::Rendition::Current
                && frame.processing_quality() == self.preferences.processing_quality
                && frame.artifact.settings.working_scale == foveonic::render::Scale::NATIVE
                && frame.artifact.settings.preview_region.is_none()
                && !frame.request.detail
                && !frame.request.framing_preview
                && frame.comparison.is_none()
                && self.matches_selected(&frame.request.settings)
                && self.shows_current_view(frame)
                && self
                    .worker
                    .cache
                    .source_origin(&frame.request.path)
                    .is_none_or(|origin| origin == frame.origin)
        });
        let requested_preview_scale = self
            .selected
            .as_deref()
            .and_then(|path| self.preview_scale_for(path, self.preferences.processing_quality));
        let navigator_ready = self
            .navigator_source()
            .is_some_and(|source| self.navigator.ready(source, mapping));
        let visible_native = self.viewport().visible_pixels().map(|pixels| {
            let window = self
                .active_image()
                .map_or([0; 4], |frame| frame.display_window(false));
            pixels.native_bounds(window)
        });
        let fast_feedback_ready = scope_frame(
            self.active_patch(),
            self.navigator_source(),
            visible_native,
            self.preferences.scope_viewport,
        )
        .is_some_and(|frame| {
            frame.processing_quality() == self.preferences.processing_quality
                && self.matches_selected(&frame.request.settings)
                && self.shows_current_view(frame)
                && automatic_scale_bound(
                    frame,
                    self.viewport(),
                    self.preferences.processing_quality,
                    ctx.pixels_per_point(),
                )
                .is_ok_and(|demand| {
                    frame.artifact.settings.working_scale.divisor() <= demand.divisor()
                })
        });
        let comparison_stage = self
            .smoke
            .as_ref()
            .is_some_and(|smoke| smoke.targets == COMPARISON_TARGETS && smoke.index >= 2);
        let comparison_tiles = comparison_stage.then(|| {
            let rows = self.viewport_selection.iter().map(|path| {
                let frame = self.active_image().filter(|frame| &frame.request.path == path)
                    .or_else(|| self.photo_layers(path)[0]);
                let expected = self.multi_edit.settings(path);
                let demand = frame.and_then(|frame| automatic_scale_bound(
                    frame, self.viewports.view(Some(path)), self.preferences.processing_quality,
                    ctx.pixels_per_point(),
                ).ok());
                let ready = frame.zip(expected).zip(demand).is_some_and(|((frame, expected), demand)| {
                    frame.request.rendition == crate::editor::Rendition::Current
                        && frame.request.output == "final"
                        && frame.is_photographic()
                        && frame.processing_quality() == self.preferences.processing_quality
                        && crate::rendering::matching::same_authored_edits(&frame.request.settings, expected)
                        && frame.artifact.settings.working_scale.divisor() <= demand.divisor()
                        && self.shows_current_view(frame)
                        && self.worker.cache.source_origin(path)
                            .is_none_or(|origin| origin == frame.origin)
                        && self.browser.rendered_revision(path) == Some(frame.request.id
                            .max(self.photo_layers(path)[0].map_or(RequestId(0), |preview| preview.request.id))
                            .0)
                });
                (ready, serde_json::json!({"source":path,"ready":ready,
                    "expected_scale_divisor":demand.map(|scale|scale.divisor()),
                    "frame":frame.map(|frame|serde_json::json!({"revision":frame.request.id,
                        "content_revision":frame.request.id,"scale_divisor":frame.artifact.settings.working_scale.divisor(),
                        "quality":frame.quality_label(),"exposure":frame.request.settings.exposure})),
                }))
            }).collect::<Vec<_>>();
            let complete = rows.len() > 1
                && self.edit_selection.paths().iter().all(|path|self.viewport_selection.contains(path))
                && rows.iter().all(|(ready,_)|*ready);
            (complete, rows.into_iter().map(|(_,row)|row).collect::<Vec<_>>())
        });
        let comparison_auxiliary_disabled =
            comparison_stage && self.multi_edit.multiple() && self.scope_key.is_none() && {
                let state = self.navigator.smoke_state(None, mapping);
                ["displayed", "wanted", "sent", "queued", "active"]
                    .iter()
                    .all(|name| state[*name].is_null())
                    && state["ready"]
                        .as_array()
                        .is_some_and(|ready| ready.is_empty())
            };
        let Some(mut held) = self.smoke.take() else {
            return;
        };
        'tick: {
            let smoke = &mut held;
            if let Some(close_at) = smoke.close_at {
                if std::time::Instant::now() >= close_at {
                    smoke.capture.request_close(ctx);
                } else {
                    ctx.request_repaint_after(std::time::Duration::from_millis(200));
                }
                break 'tick;
            }
            // A background repaint may arrive before the window handles Close.
            if smoke.index >= smoke.targets.len() {
                smoke.capture.request_close(ctx);
                break 'tick;
            }
            if smoke.targets == SMART_TARGETS
                && matches!(
                    smoke.targets[smoke.index],
                    "smart-zoom-pending" | "smart-crop-enter" | "smart-crop-leave"
                )
            {
                if let Some((framing_preview, current)) = shown_detail {
                    assert!(current, "obsolete regional detail was presented");
                    assert_eq!(
                        framing_preview,
                        crate::crop_tool::active(ctx),
                        "detail used the wrong crop view"
                    );
                }
                if self
                    .viewports
                    .view(self.selected.as_deref())
                    .zoom_percent(ctx.pixels_per_point())
                    .is_some_and(|z| z <= 100.)
                {
                    assert!(
                        shown_detail.is_none(),
                        "regional detail remained visible at Fit"
                    );
                }
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
            if let Some(image) = screenshot
                .as_ref()
                .filter(|_| smoke.targets == VIEWPORT_SCOPE_TARGETS && !smoke.preview_pending())
            {
                let requested_capture = smoke.scope_capture.is_some_and(|(key, pixels)| {
                    self.scope_key == Some(key)
                        && self
                            .scope_snapshot
                            .as_ref()
                            .is_some_and(|snapshot| snapshot.key == key)
                        && image.size == pixels
                });
                if !requested_capture
                    || (smoke.targets[smoke.index] == "viewport-resize"
                        && ctx.screen_rect().size() != egui::vec2(1200., 800.))
                {
                    // The scope request and native size can change while capture is pending.
                    // Do not pair an old screenshot with a newer scope report.
                    smoke.awaiting = false;
                    smoke.scope_capture = None;
                    ctx.request_repaint();
                    break 'tick;
                }
            }
            if let Some(image) = screenshot {
                if smoke.targets == STATUS_TARGETS {
                    match smoke.targets[smoke.index] {
                        "status-error" | "status-new-error" => {
                            assert!(self.activity.unread_errors())
                        }
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
                if smoke.targets == rotate::TARGETS
                    && smoke.targets[smoke.index].starts_with("rotate-")
                {
                    rotate::check(
                        ctx,
                        smoke,
                        self.active_image().unwrap(),
                        self.smoke_recipe(),
                        self.viewports.view(self.selected.as_deref()),
                    );
                }
                if smoke.targets == CROP_TARGETS && !smoke.preview_pending() {
                    let frame = self.active_image().unwrap();
                    let target = smoke.targets[smoke.index];
                    if target.starts_with("crop-") {
                        assert_eq!(
                            frame.request.framing_preview,
                            matches!(
                                target,
                                "crop-ready" | "crop-drag" | "crop-applied" | "crop-reopen"
                            )
                        );
                        assert_eq!(crate::crop_tool::active(ctx), frame.request.framing_preview);
                        assert_eq!(
                            self.smoke_recipe().crop.is_full(),
                            matches!(target, "crop-ready" | "crop-drag")
                        );
                        assert_eq!(frame.artifact.settings.crop, self.smoke_recipe().crop);
                        if target == "crop-smart" {
                            let feedback = self.active_patch().unwrap_or(frame);
                            assert_eq!(
                                feedback.artifact.settings.working_scale,
                                foveonic::render::Scale::NATIVE
                            );
                            smoke.zoom_retained = Some(ZoomRetention::capture(feedback));
                        }
                        if target == "crop-tile" {
                            assert!(current_native_feedback);
                            let feedback = if shown_detail.is_some() {
                                self.active_patch().unwrap()
                            } else {
                                frame
                            };
                            let region = feedback.artifact.settings.preview_region;
                            if let Some(region) = region {
                                let visible = self
                                    .viewports
                                    .view(self.selected.as_deref())
                                    .visible_pixels()
                                    .unwrap();
                                let window = frame.display_window(false);
                                assert!(crate::rendering::smart::contains(
                                    region,
                                    visible.native_bounds(window)
                                ));
                                assert_eq!(
                                    feedback.texture.description().extent,
                                    [region[2] - region[0], region[3] - region[1]]
                                );
                                assert_eq!(
                                    feedback.artifact.settings.working_scale,
                                    foveonic::render::Scale::NATIVE
                                );
                            } else {
                                smoke.zoom_retained.as_ref().unwrap().check(feedback);
                            }
                            std::fs::write(smoke.directory.join("crop-tile-feedback.json"),
                            serde_json::to_vec_pretty(&serde_json::json!({"feedback":if region.is_some() {"region"} else {"whole"},"regional_supported":regional_smart})).unwrap()).unwrap();
                        }
                        if frame.request.framing_preview {
                            assert_eq!(frame.display_extent(false), frame.editing_extent());
                        }
                        std::fs::write(
                            smoke.directory.join(format!("{target}.json")),
                            serde_json::to_vec_pretty(&self.smoke_recipe().crop).unwrap(),
                        )
                        .unwrap();
                    }
                }
                if let Some((ready, tiles)) = &comparison_tiles {
                    assert!(*ready, "comparison images changed before capture");
                    assert!(
                        !self.multi_edit.multiple() || comparison_auxiliary_disabled,
                        "multi-edit auxiliaries were not disabled"
                    );
                    std::fs::write(
                        smoke
                            .directory
                            .join(format!("{}.json", smoke.targets[smoke.index])),
                        serde_json::to_vec_pretty(&serde_json::json!({"tiles":tiles,
                        "multiple_editing_targets":self.multi_edit.multiple(),
                        "auxiliary_disabled":comparison_auxiliary_disabled}))
                        .unwrap(),
                    )
                    .unwrap();
                }
                if smoke.targets == COMPARISON_TARGETS
                    && smoke.targets[smoke.index] == "comparison-switch"
                {
                    let selected = self.browser.processing_selection(self.selected.as_deref());
                    assert!(selected.len() > 1);
                    assert_eq!(selected.get(1), self.selected.as_ref());
                }
                if smoke.targets == WORKFLOW_TARGETS {
                    let frame = self.active_image();
                    let target = smoke.targets[smoke.index];
                    let evidence = frame.map(|frame| serde_json::json!({
                    "target":target,"path":frame.request.path,"rendition":format!("{:?}",frame.request.rendition),
                    "presentation_revision":frame.request.id,"content_revision":frame.request.id,
                    "scale":frame.artifact.settings.working_scale.divisor(),"region":frame.artifact.settings.preview_region,
                    "exposure":frame.request.settings.exposure,"authored_exposure":self.smoke_recipe().exposure,
                    "matches_authored":self.matches_selected(&frame.request.settings),
                    "current_view":self.shows_current_view(frame),
                }));
                    std::fs::write(
                        smoke.directory.join(format!("{target}.json")),
                        serde_json::to_vec_pretty(&evidence).unwrap(),
                    )
                    .unwrap();
                    if target == "workflow-current" {
                        smoke.authored = Some(self.smoke_recipe().clone());
                        smoke.fast_region = self
                            .viewports
                            .view(self.selected.as_deref())
                            .visible_pixels();
                        smoke.display_revision = frame.map(|f| f.request.id.0);
                        smoke.current_texture = frame.map(|f| f.texture.clone());
                    }
                    if matches!(
                        target,
                        "workflow-camera"
                            | "workflow-camera-open"
                            | "workflow-restored-open"
                            | "workflow-defaults"
                            | "workflow-restored"
                            | "workflow-restored-again"
                    ) {
                        assert_eq!(
                            Some(self.smoke_recipe()),
                            smoke.authored.as_ref(),
                            "Before must preserve authored edits"
                        );
                        assert_eq!(
                            self.viewports
                                .view(self.selected.as_deref())
                                .visible_pixels(),
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
                            assert_ne!(
                                frame.unwrap().request.rendition,
                                crate::editor::Rendition::Current
                            );
                            assert_ne!(
                                self.browser.rendered_revision(&frame.unwrap().request.path),
                                frame.map(|f| f.request.id.0)
                            );
                        }
                    }
                }
                if smoke.targets == BROWSER_TARGETS
                    && smoke.targets[smoke.index].starts_with("browser-")
                {
                    assert_eq!(
                        self.active_image().map(|frame| frame.request.id.0),
                        smoke.display_revision,
                        "browser changes must not rerender the image"
                    );
                }
                {
                    let _artifact = foveonic::diagnostics::scope(
                        foveonic::diagnostics::Category::Scenario,
                        "smoke.artifact.png",
                    );
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
                }
                if smoke.targets == SMART_TARGETS && smoke.preview_pending() {
                    assert_eq!(
                        self.preferences.processing_quality,
                        foveonic::render::ProcessingQuality::Quality
                    );
                    assert_eq!(
                        requested_preview_scale.unwrap(),
                        foveonic::render::Scale::new(2).unwrap()
                    );
                    let preview = self
                        .browser
                        .thumbnail(self.selected.as_ref().unwrap())
                        .unwrap();
                    let report = serde_json::json!({"processing_quality":"Quality", "render_received":self.active_image().is_some(), "requested_scale_divisor":requested_preview_scale.unwrap().divisor(),"native_extent":preview.native_extent,"thumbnail_extent":preview.texture.size()});
                    std::fs::write(
                        smoke.directory.join("preview.json"),
                        serde_json::to_vec_pretty(&report).unwrap(),
                    )
                    .unwrap();
                }
                if smoke.targets == VIEWPORT_SCOPE_TARGETS && !smoke.preview_pending() {
                    let frame = scope_frame(
                        shown_detail.and_then(|_| self.active_patch()),
                        self.active_image(),
                        visible_native,
                        self.preferences.scope_viewport,
                    )
                    .expect("current scope source");
                    let snapshot = self.scope_snapshot.as_ref().unwrap();
                    let key = snapshot.key;
                    assert_eq!(self.scope_key, Some(key));
                    let data = &snapshot.data;
                    assert_eq!(key.request_id, frame.request.id);
                    assert!(!frame.request.framing_preview);
                    let expected_region = scope_region(
                        frame,
                        self.viewports.view(self.selected.as_deref()),
                        visible_native,
                        self.preferences.scope_viewport,
                    );
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
                    let report = serde_json::json!({"revision":key.request_id,"region":key.region,"samples":data.samples,"histogram":data.histogram(0),"density_weight":density_weight,"density_scale":foveonic::scopes::DENSITY_SCALE,
                    "capture_request":smoke.scope_capture.map(|(key,pixels)|serde_json::json!({
                        "revision":key.request_id,"region":key.region,"image_pixels":pixels})),
                    "captured_image_pixels":image.size});
                    std::fs::write(
                        smoke
                            .directory
                            .join(format!("{}.json", smoke.targets[smoke.index])),
                        serde_json::to_vec_pretty(&report).unwrap(),
                    )
                    .unwrap();
                }
                if smoke.targets == FAST_PREVIEW_TARGETS && !smoke.preview_pending() {
                    let frame = scope_frame(
                        shown_detail.and_then(|_| self.active_patch()),
                        self.active_image(),
                        visible_native,
                        self.preferences.scope_viewport,
                    )
                    .expect("current scope source");
                    let target = smoke.targets[smoke.index];
                    assert_eq!(
                        frame.processing_quality(),
                        self.preferences.processing_quality
                    );
                    let native = frame.display_extent(false);
                    let divisor = frame.artifact.settings.working_scale.divisor();
                    assert_eq!(
                        frame.texture.description().extent,
                        native.map(|value| value / divisor)
                    );
                    let visible = self
                        .viewports
                        .view(self.selected.as_deref())
                        .visible_pixels()
                        .unwrap();
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
                    if target != "final" {
                        assert_eq!(self.scope_key.unwrap().request_id, frame.request.id);
                        assert_eq!(
                            self.scope_key.unwrap().region,
                            scope_region(
                                frame,
                                self.viewports.view(self.selected.as_deref()),
                                visible_native,
                                self.preferences.scope_viewport,
                            )
                        );
                    }
                    let progress = self.worker.progress.snapshot().unwrap();
                    assert_eq!(Some(&frame.request.path), self.selected.as_ref());
                    assert_eq!(frame.request.rendition, crate::editor::Rendition::Current);
                    assert_eq!(frame.request.output, "final");
                    assert_eq!(progress.fraction, 1.);
                    assert!(!progress.running());
                    assert!(!progress.failed);
                    assert!(progress.request_id <= self.view_images.last_request_id().0);
                    // Auxiliary previews and idle Whole completion do not own foreground progress.
                    if progress.request_id == frame.request.id.0 {
                        assert!(progress.milliseconds() >= frame.milliseconds);
                    }
                    let report = serde_json::json!({"foreground_progress_revision":progress.request_id,"source_revision":frame.request.id,"source_preview_region":frame.artifact.settings.preview_region,"total_ms":progress.milliseconds(),"progress":progress.fraction,"target":target,"quality":frame.quality_label(),"rendered_extent":frame.texture.description().extent,"native_extent":native,"visible_native":[visible.top_left,visible.bottom_right],"scope_region":self.scope_key.unwrap().region,"thumbnail_revision":self.browser.rendered_revision(&frame.request.path),"processing_ms":frame.milliseconds});
                    std::fs::write(
                        smoke.directory.join(format!("{target}.json")),
                        serde_json::to_vec_pretty(&report).unwrap(),
                    )
                    .unwrap();
                }
                if smoke.targets == SMART_TARGETS && !smoke.preview_pending() {
                    let target = smoke.targets[smoke.index];
                    let frame = if shown_detail.is_some() {
                        self.active_patch().unwrap()
                    } else {
                        self.active_image().unwrap()
                    };
                    let visible = self
                        .viewports
                        .view(self.selected.as_deref())
                        .visible_pixels()
                        .unwrap();
                    assert_eq!(
                        frame.processing_quality(),
                        self.preferences.processing_quality
                    );
                    let demanded_scale = automatic_scale_bound(
                        frame,
                        self.viewports.view(self.selected.as_deref()),
                        self.preferences.processing_quality,
                        ctx.pixels_per_point(),
                    )
                    .unwrap();
                    assert!(
                        frame.artifact.settings.working_scale.divisor() <= demanded_scale.divisor(),
                        "automatic preview is coarser than physical display demand"
                    );
                    let region = frame.artifact.settings.preview_region;
                    let regional = region.is_some();
                    if !quattro_smart || !matches!(target, "final" | "cache-reopen") {
                        assert!(self.smoke_recipe().reconstruction.is_some());
                        assert!(frame.artifact.settings.reconstruction.is_some());
                    }
                    if matches!(target, "smart-detail" | "smart-edit-detail") {
                        assert!(current_native_feedback);
                        if target == "smart-edit-detail" {
                            assert!(smoke.smart_scrub_checked);
                            assert_eq!(frame.request.settings.exposure, 0.75);
                        }
                    }
                    if target == "smart-100" {
                        smoke.zoom_retained = Some(ZoomRetention::capture(frame));
                    }
                    if target == "smart-detail" && !regional {
                        smoke.zoom_retained.as_ref().unwrap().check(frame);
                    }
                    if regional {
                        let rect = region.unwrap();
                        assert!(crate::rendering::smart::contains(rect, visible.bounds()));
                        assert_eq!(
                            frame.texture.description().extent,
                            [rect[2] - rect[0], rect[3] - rect[1]]
                        );
                        assert_eq!(
                            frame.artifact.settings.working_scale,
                            foveonic::render::Scale::NATIVE
                        );
                    } else {
                        let full = matches!(
                            target,
                            "smart-100"
                                | "smart-detail"
                                | "smart-full"
                                | "smart-pan"
                                | "smart-edit-detail"
                                | "smart-edit"
                                | "smart-crop-zoom"
                                | "smart-crop-enter"
                                | "smart-crop-active-zoom"
                                | "smart-crop-leave"
                                | "smart-adaptive-zoom"
                                | "smart-transition-full"
                                | "smart-adaptive-edit"
                        );
                        if target == "cache-reopen" {
                            let retained = smoke.smart_reopen.as_ref().unwrap();
                            let texture = retained
                                .texture
                                .upgrade()
                                .expect("reopening discarded retained pixels");
                            assert!(
                                std::sync::Arc::ptr_eq(&texture, &frame.texture),
                                "same-photo selection replaced retained pixels"
                            );
                            assert_eq!(
                                frame.diagnostic_task, retained.task,
                                "same-photo selection replaced the delivered task"
                            );
                            assert_eq!(frame.artifact.settings.working_scale, retained.scale);
                            assert_eq!(
                                self.browser.rendered_revision(&frame.request.path),
                                retained.thumbnail_revision
                            );
                        } else if full {
                            assert_eq!(
                                frame.artifact.settings.working_scale,
                                foveonic::render::Scale::NATIVE
                            );
                        }
                        assert_eq!(frame.artifact.settings.preview_region, None);
                        if target == "smart-pan" {
                            assert_eq!(
                                smoke.smart_full_revision,
                                Some(frame.request.id.0),
                                "panning submitted more render work"
                            );
                        }
                    }
                    if matches!(
                        target,
                        "smart-zoom-pending"
                            | "smart-crop-enter"
                            | "smart-crop-leave"
                            | "smart-transition-full"
                    ) {
                        if target == "smart-transition-full" {
                            // An identical native Whole request may coalesce with the
                            // admitted frame instead of producing a new presentation.
                            assert!(current_native_whole);
                        } else {
                            assert!(self.shows_current_view(frame));
                        }
                        assert!(self.matches_selected(&frame.request.settings));
                        assert_eq!(frame.request.framing_preview, target == "smart-crop-enter");
                        assert_eq!(
                            self.scope_key.map(|key| key.request_id),
                            Some(frame.request.id)
                        );
                        if !frame.request.framing_preview {
                            assert_eq!(
                                self.browser.rendered_revision(&frame.request.path),
                                Some(frame.request.id.0)
                            );
                        }
                    }
                    if target == "smart-full" {
                        smoke.smart_full_revision = Some(frame.request.id.0);
                    }
                    if target == "cache-reopen" {
                        assert!(
                            self.shows_current_view(frame),
                            "retained image does not answer the reopened photo's view"
                        );
                    }
                    let report = serde_json::json!({"target":target,"quality":frame.quality_label(),"region":region,"feedback":if regional {"region"} else {"whole"},"regional_supported":regional_smart,"native_extent":self.active_image().unwrap().display_extent(false),"rendered_extent":frame.texture.description().extent,
                    "zoom_percent":self.viewports.view(self.selected.as_deref()).zoom_percent(ctx.pixels_per_point()),"thumbnail_revision":self.browser.rendered_revision(&frame.request.path),"processing_ms":frame.milliseconds,"scope_revision":self.scope_key.map(|k|k.request_id),"image_revision":frame.request.id,"cache_hits":self.worker.cache.status().hits,"cache_ready":self.worker.cache.status().ready.len(),"continuation_running":self.busy});
                    std::fs::write(
                        smoke.directory.join(format!("{target}.json")),
                        serde_json::to_vec_pretty(&report).unwrap(),
                    )
                    .unwrap();
                }
                if smoke.targets == DISPLAY_COLOR_TARGETS && !smoke.preview_pending() {
                    let frame = self.active_image().unwrap();
                    let revision = *smoke.display_revision.get_or_insert(frame.request.id.0);
                    assert_eq!(
                        frame.request.id,
                        RequestId(revision),
                        "display choice submitted photographic work"
                    );
                    assert!(!self.busy);
                    let report = serde_json::json!({"gamut":self.preferences.display_gamut,
                    "image_revision":revision,"thumbnail_revision":self.browser.rendered_revision(&frame.request.path),
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
                    let frame = self.active_image().unwrap();
                    let target = smoke.targets[smoke.index];
                    if target.starts_with("dehaze-") && target != "dehaze-off" {
                        assert_eq!(
                            frame.processing_quality(),
                            self.preferences.processing_quality
                        );
                        let bound = automatic_scale_bound(
                            frame,
                            self.viewports.view(self.selected.as_deref()),
                            self.preferences.processing_quality,
                            ctx.pixels_per_point(),
                        )
                        .unwrap();
                        assert!(
                            frame.artifact.settings.working_scale.divisor() <= bound.divisor(),
                            "Dehaze preview is coarser than physical display demand"
                        );
                        assert_eq!(frame.artifact.settings.preview_region, None);
                        assert_eq!(frame.artifact.settings.dehaze, self.smoke_recipe().dehaze);
                    }
                    let estimated = crate::editor::dehaze::auto_color(ctx, self.smoke_recipe());
                    if target == "final" {
                        assert!(!self.smoke_recipe().dehaze_auto());
                        assert!(frame.artifact.dehaze_estimate.is_none());
                    }
                    if target == "dehaze-auto" {
                        assert_eq!(estimated, frame.artifact.dehaze_estimate);
                        assert!(estimated.is_some(), "fixture needs an automatic haze color");
                    }
                    if target == "dehaze-freeze" {
                        assert_eq!(self.smoke_recipe().dehaze.shadow, estimated);
                    }
                    if target == "dehaze-reset" {
                        assert!(self.smoke_recipe().dehaze.shadow.is_some());
                        assert!(!self.smoke_recipe().dehaze_auto());
                        assert_eq!(self.smoke_recipe().dehaze.amount, 35.);
                    }
                    let report = serde_json::json!({"target":target,"dehaze":self.smoke_recipe().dehaze,"automatic_color":estimated,"shown_color":self.smoke_recipe().dehaze.shadow.or(estimated),"rendering":self.smoke_recipe().rendering,"quality":frame.quality_label(),"processing_ms":frame.milliseconds});
                    std::fs::write(
                        smoke.directory.join(format!("{target}.json")),
                        serde_json::to_vec_pretty(&report).unwrap(),
                    )
                    .unwrap();
                }
                smoke.diagnostic_checked(self.active_image(), self.worker.busy());
                smoke.index += 1;
                smoke.awaiting = false;
                smoke.scope_capture = None;
                if smoke.index == smoke.targets.len() {
                    if smoke.targets == screenshot::TARGETS {
                        smoke.capture.request_close(ctx);
                        break 'tick;
                    }
                    // Exercise native preference restoration on a subsequent isolated launch.
                    self.preferences.organizer = Organizer::List;
                    self.preferences.scope = Scope::Waveform;
                    self.preferences
                        .move_section(Section::Metadata, Section::Navigator, false);
                    self.preferences.collapsed = [Section::Editing].into();
                    self.smoke_edit(|recipe| recipe.exposure = 1.25); // deliberately session-only
                    if let Some(seconds) = std::env::var_os("FOVEONIC_GUI_SMOKE_HOLD_SECONDS")
                        .and_then(|s| s.to_str().and_then(|s| s.parse::<u64>().ok()))
                    {
                        smoke.close_at = Some(
                            std::time::Instant::now() + std::time::Duration::from_secs(seconds),
                        );
                        ctx.request_repaint();
                    } else {
                        smoke.capture.request_close(ctx);
                    }
                    break 'tick;
                }
                smoke.diagnostic_begin();
                if smoke.targets == WORKFLOW_TARGETS {
                    let target = smoke.targets[smoke.index];
                    let source = self.active_image();
                    std::fs::write(
                        smoke
                            .directory
                            .join(format!("{target}-navigator-before.json")),
                        serde_json::to_vec_pretty(&self.navigator.smoke_state(source, mapping))
                            .unwrap(),
                    )
                    .unwrap();
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
                    target if target.starts_with("rotate-") => self.smoke_rotate_enter(smoke),
                    "crop-fast" => {
                        crate::crop_tool::exit(ctx);
                        self.preferences.processing_quality = ProcessingQuality::Speed;
                    }
                    "crop-smart" => {
                        self.preferences.processing_quality = ProcessingQuality::Speed;
                        self.viewports.view_mut(self.selected.as_deref()).one_to_one = true;
                        self.submit_photos();
                    }
                    "crop-tile" => self
                        .viewports
                        .view_mut(self.selected.as_deref())
                        .zoom_by(2.),
                    "crop-detail" => {}
                    "crop-ready" | "crop-reopen" | "crop-pointer" => {
                        let key = if smoke.targets[smoke.index] == "crop-pointer" {
                            egui::Key::V
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
                            .viewports
                            .view(self.selected.as_deref())
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
                            let end = self
                                .viewports
                                .view(self.selected.as_deref())
                                .position_at_uv([0.18, 0.18])
                                .unwrap();
                            smoke
                                .direct_events
                                .push_back(vec![egui::Event::PointerMoved(end)]);
                        }
                    }
                    "dehaze-auto" => {
                        self.viewports.view_mut(self.selected.as_deref()).reset();
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
                        self.smoke_edit(|recipe| {
                            recipe.dehaze = foveonic::render::DehazeControls {
                                amount: 35.,
                                shadow: None,
                            }
                        });
                        self.submit_photos();
                    }
                    "dehaze-freeze" => {
                        // The edit the panel's Auto button pushes when turned off.
                        let manual = crate::multi_edit_controls::Edit::Optional {
                            pointer: "/dehaze/shadow".into(),
                            enabled: true,
                        };
                        assert!(
                            self.edit_selected(crate::multi_edit_runtime::Change::Fields(vec![
                                manual
                            ]))
                        );
                        self.submit_photos();
                    }
                    "dehaze-reset" => {
                        let family = self
                            .selected_metadata()
                            .unwrap()
                            .processing_family()
                            .unwrap();
                        let scope = crate::editor::Scope::Tool(crate::editor::Section::Corrections);
                        // Restore a distinct saved amount, so this reset must render an actual change.
                        let mut defaults = self.smoke_recipe().clone();
                        defaults.dehaze.amount = 35.;
                        self.preferences
                            .editor
                            .defaults
                            .set(family, scope, &defaults);
                        self.smoke_edit(|recipe| recipe.dehaze.shadow = None);
                        self.editor_ui
                            .actions
                            .push(crate::editor::Action::Reset(scope, false));
                        assert!(self.apply_editor_actions(ctx));
                        assert!(self.smoke_recipe().dehaze.shadow.is_some());
                        self.submit_photos();
                    }
                    "white-balance-ready" => {
                        self.smoke_edit(|recipe| recipe.color_editor_enabled = false);
                        self.submit_photos();
                    }
                    "white-balance-picked" => {
                        crate::editor::white_balance::toggle(ctx);
                        let position = self
                            .viewports
                            .view(self.selected.as_deref())
                            .position_at_uv([0.82, 0.04])
                            .unwrap();
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
                    "white-balance-reset" => {
                        assert!(self.smoke_recipe().white_balance_bias.neutral.is_some());
                        let scope =
                            crate::editor::Scope::Tool(crate::editor::Section::WhiteBalance);
                        self.editor_ui
                            .actions
                            .push(crate::editor::Action::Reset(scope, false));
                        assert!(self.apply_editor_actions(ctx));
                        assert!(self.smoke_recipe().white_balance_bias.is_neutral());
                        self.submit_photos();
                    }
                    "dehaze-picked" => {
                        crate::editor::dehaze::toggle(ctx);
                        // The neutral bolt in the parking-sign fixture avoids a clipped blue patch.
                        let position = self
                            .viewports
                            .view(self.selected.as_deref())
                            .position_at_uv([0.82, 0.04])
                            .unwrap();
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
                        assert!(self.smoke_recipe().dehaze.shadow.is_some());
                        let target = smoke.targets[smoke.index];
                        self.smoke_edit(|recipe| {
                            recipe.dehaze.amount = match target {
                                "dehaze-manual" => 65.,
                                "dehaze-add" => -35.,
                                "dehaze-off" => 0.,
                                _ => 35.,
                            }
                        });
                        if target == "dehaze-monochrome" {
                            self.smoke_edit(|recipe| {
                                recipe.rendering = foveonic::render::Rendering::Monochrome
                            });
                        }
                        self.submit_photos();
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
                        self.smoke_edit(|recipe| {
                            recipe.photographic.color_editor = Default::default()
                        });
                        let mut state = crate::editor::advanced_color::state(ctx);
                        state.advanced = true;
                        state.selected = 0;
                        crate::editor::direct_color::toggle_advanced(ctx);
                        crate::editor::advanced_color::store(ctx, state);
                        self.submit_photos();
                    }
                    "advanced-picked" => {
                        let (response, _) = self
                            .viewports
                            .view(self.selected.as_deref())
                            .pointer(None)
                            .unwrap();
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
                        smoke.name_revision = Some(self.view_images.last_request_id().0);
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
                        assert_eq!(
                            self.smoke_recipe().photographic.color_editor.advanced.count,
                            1
                        );
                        self.smoke_edit(|recipe| {
                            recipe.photographic.color_editor.advanced.ranges[0]
                                .adjustment
                                .lightness = -0.3;
                        });
                        if target == "advanced-selection" {
                            let buttons = crate::automation::snapshot()
                                .expect("direct smoke runs observe controls")
                                .buttons;
                            let eye = buttons
                                .iter()
                                .find(|button| button.label == "View color range")
                                .unwrap_or_else(|| {
                                    let labels: Vec<_> =
                                        buttons.iter().map(|button| &button.label).collect();
                                    panic!("the panel shows no range eye among {labels:?}")
                                });
                            let center = egui::pos2(
                                (eye.rect[0] + eye.rect[2]) / 2.,
                                (eye.rect[1] + eye.rect[3]) / 2.,
                            );
                            smoke.direct_events.extend([
                                vec![egui::Event::PointerMoved(center)],
                                vec![egui::Event::PointerButton {
                                    pos: center,
                                    button: egui::PointerButton::Primary,
                                    pressed: true,
                                    modifiers: egui::Modifiers::NONE,
                                }],
                                vec![egui::Event::PointerButton {
                                    pos: center,
                                    button: egui::PointerButton::Primary,
                                    pressed: false,
                                    modifiers: egui::Modifiers::NONE,
                                }],
                            ]);
                        } else {
                            self.range_preview = None;
                        }
                        self.smoke_edit(|recipe| {
                            recipe.rendering = if target == "advanced-monochrome" {
                                foveonic::render::Rendering::Monochrome
                            } else {
                                foveonic::render::Rendering::Color
                            }
                        });
                        self.submit_photos();
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
                        self.smoke_edit(|recipe| {
                            recipe.rendering = if target == "base-monochrome" {
                                foveonic::render::Rendering::Monochrome
                            } else {
                                foveonic::render::Rendering::Color
                            };
                            recipe.photographic.tone_method =
                                foveonic::render::PhotographicMethod::Universal
                        });
                        self.settings_open = false;
                        crate::application_settings::set_adaptive_open(
                            ctx,
                            target == "base-limits",
                        );
                        self.preferences.adaptive_tone_tuning = Default::default();
                        if target == "base-limits" {
                            self.preferences.adaptive_tone_tuning.natural.shadows = 0.75;
                        }
                        self.smoke_edit(|recipe| {
                            recipe.photographic.base_response = if target == "base-camera" {
                                foveonic::render::BaseResponse::Camera
                            } else if target == "base-strong" {
                                foveonic::render::BaseResponse::Strong
                            } else {
                                foveonic::render::BaseResponse::Natural
                            };
                            recipe.tonal_ranges = if target == "base-hdr" {
                                foveonic::render::TonalRanges {
                                    shadows: 0.2,
                                    whites: -0.2,
                                    ..Default::default()
                                }
                            } else {
                                Default::default()
                            }
                        });
                        self.submit_photos();
                    }
                    "direct-color-ready" => {
                        if std::env::var_os("FOVEONIC_GUI_SMOKE_DIRECT_COLOR_COMPARISON").is_some()
                        {
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
                        self.smoke_edit(|recipe| {
                            recipe.photographic.color_editor = Default::default()
                        });
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
                        self.submit_photos();
                    }
                    "direct-color-edited" => {
                        let (response, _) = self
                            .viewports
                            .view(self.selected.as_deref())
                            .pointer(None)
                            .unwrap();
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
                        let (response, _) = self
                            .viewports
                            .view(self.selected.as_deref())
                            .pointer(None)
                            .unwrap();
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
                        self.smoke_edit(|recipe| {
                            recipe.rendering = if target == "exposure-auto-monochrome" {
                                Rendering::Monochrome
                            } else {
                                Rendering::Color
                            };
                            recipe.photographic.tone_method = if target == "exposure-auto-reference"
                            {
                                PhotographicMethod::Reference
                            } else {
                                PhotographicMethod::Universal
                            };
                            recipe.photographic.base_response = BaseResponse::Natural;
                            recipe.exposure_auto = target != "exposure-auto-manual";
                            recipe.exposure = 0.;
                            recipe.exposure_auto_compensation = match target {
                                "exposure-auto-dial" => AutoExposureCompensation::Dial,
                                "exposure-auto-bracket" => AutoExposureCompensation::Bracket,
                                _ => AutoExposureCompensation::Both,
                            }
                        });
                        self.submit_photos();
                    }
                    "perceptual-neutral" => {
                        self.preferences.collapsed =
                            [Section::Navigator, Section::Scopes, Section::Metadata].into();
                        self.smoke_edit(|recipe| {
                            recipe.photographic.tone_method =
                                foveonic::render::PhotographicMethod::Universal;
                            recipe.photographic.saturation_method =
                                foveonic::render::PhotographicMethod::Universal
                        });
                        self.submit_photos();
                    }
                    "perceptual-grade" => {
                        self.smoke_edit(|recipe| {
                            recipe.photographic.brightness = 0.15;
                            recipe.tonal_ranges.shadows = 0.25;
                            recipe.tonal_ranges.highlights = -0.6;
                            let c = &mut recipe.photographic.perceptual;
                            c.saturation = 0.123456;
                            c.contrast = -0.4;
                            c.bands[1].saturation = 0.1;
                            c.bands[2].saturation = -0.2;
                        });
                        self.submit_photos();
                    }
                    "perceptual-extended" | "perceptual-unlocked" => {
                        self.smoke_edit(|recipe| {
                            recipe.photographic.perceptual = Default::default();
                            recipe.photographic.perceptual.saturation = 2.;
                            recipe.prevent_color_clipping =
                                smoke.targets[smoke.index] != "perceptual-unlocked"
                        });
                        self.preferences
                            .editing_sections
                            .retain(|s| *s != crate::preferences::EditingSection::Saturation);
                        self.preferences
                            .editing_sections
                            .insert(0, crate::preferences::EditingSection::Saturation);
                        self.submit_photos();
                    }
                    "perceptual-mask" => {
                        self.output = "perceptual.midtones".into();
                        self.submit_photos();
                    }
                    "perceptual-editor" => {
                        self.output = "final".into();
                        self.smoke_edit(|recipe| {
                            recipe.photographic.color_editor.bands[0].saturation = -0.2
                        });
                        self.submit_photos();
                    }
                    "perceptual-mono" => {
                        self.smoke_edit(|recipe| {
                            recipe.rendering = foveonic::render::Rendering::Monochrome
                        });
                        self.submit_photos();
                    }
                    "comparison-switch" => {
                        self.comparison_selection = self
                            .browser
                            .processing_selection(self.selected.as_deref())
                            .get(1)
                            .cloned()
                            .map(|path| crate::viewport_selection::Request {
                                path,
                                gesture: crate::viewport_selection::Gesture::Replace,
                            });
                        ctx.request_repaint();
                    }
                    "comparison" => self.browser.set_actions(vec![(
                        crate::shortcuts::BrowserAction::SelectAll,
                        egui::Modifiers::COMMAND,
                    )]),
                    "workflow-current" => {
                        self.smoke_edit(|recipe| {
                            recipe.exposure = 0.7;
                            recipe.photographic.tone_method =
                                foveonic::render::PhotographicMethod::Universal
                        });
                        self.preferences.collapsed =
                            [Section::Navigator, Section::Scopes, Section::Metadata].into();
                        self.viewports.view_mut(self.selected.as_deref()).one_to_one = true;
                        self.viewports
                            .view_mut(self.selected.as_deref())
                            .pan_to(egui::vec2(0.55, 0.4));
                        self.submit_photos();
                    }
                    "workflow-navigator-open" => {
                        self.preferences.collapsed.remove(&Section::Navigator);
                    }
                    "workflow-camera"
                    | "workflow-restored"
                    | "workflow-camera-open"
                    | "workflow-restored-open" => {
                        self.toggle_before(crate::editor::Rendition::Camera)
                    }
                    "workflow-defaults" => {
                        let mut defaults = self.smoke_recipe().clone();
                        defaults.exposure = 0.2;
                        self.preferences.editor.defaults.set(
                            foveonic::source::ProcessingFamily::Merrill,
                            crate::editor::Scope::Tool(crate::editor::Section::Tone),
                            &defaults,
                        );
                        self.toggle_before(crate::editor::Rendition::Defaults);
                    }
                    "workflow-restored-again" => {
                        self.toggle_before(crate::editor::Rendition::Defaults)
                    }
                    "workflow-wb" => {
                        self.smoke_edit(|recipe| {
                            recipe.white_balance_bias.warmth = 0.35;
                            recipe.white_balance_bias.tint = -0.2;
                        });
                        self.submit_photos();
                    }
                    "workflow-default-reset" | "workflow-camera-reset" => {
                        let camera = smoke.targets[smoke.index] == "workflow-camera-reset";
                        self.editor_ui.actions.push(crate::editor::Action::Reset(
                            crate::editor::Scope::Tool(crate::editor::Section::Tone),
                            camera,
                        ));
                        assert!(self.apply_editor_actions(ctx));
                        assert_eq!(self.smoke_recipe().exposure, if camera { 0. } else { 0.2 });
                        assert_eq!(self.smoke_recipe().white_balance_bias.warmth, 0.35);
                        self.submit_photos();
                    }
                    "browser-grid" | "browser-list" | "browser-large" => {
                        if smoke.display_revision.is_none() {
                            smoke.display_revision =
                                self.active_image().map(|frame| frame.request.id.0);
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
                        if let Some(mut state) = egui::containers::panel::PanelState::load(ctx, id)
                        {
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
                        crate::viewport::set_gamut(
                            &self.render_state,
                            self.preferences.display_gamut,
                        );
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
                    "white-background" => self.preferences.white_viewport_background = true,
                    "browse-processed" | "browse-processed-again" => {
                        self.processing_open = false;
                        self.process();
                    }
                    "import-open" => {
                        let destination = smoke.directory.join("imported");
                        self.open(
                            self.browser.root.clone().unwrap(),
                            crate::preferences::FolderMode::Import,
                        );
                        self.preferences.processing_destination = Some(destination);
                    }
                    "import-review" => {
                        let destination = smoke.directory.join("imported");
                        self.import.smoke_prepare(&destination, &mut self.browser);
                    }
                    "import-render-options" => {
                        self.preferences.import_processing.copy_only = false;
                        self.processing_open = true;
                        let id = egui::Id::new("organizer");
                        if let Some(mut state) = egui::containers::panel::PanelState::load(ctx, id)
                        {
                            state.rect.max.x = state.rect.min.x + IMPORT_WIDE_BROWSER_WIDTH;
                            ctx.data_mut(|data| data.insert_persisted(id, state));
                        }
                    }
                    "import-copied" => {
                        self.processing_open = false;
                        self.preferences.import_processing.copy_only = true;
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
                        self.smoke_edit(|current| *current = recipe.settings);
                        self.preferences.scope = Scope::Waveform;
                        self.preferences.scope_viewport = true;
                        self.browser.reveal_selection();
                        self.viewports
                            .view_mut(self.selected.as_deref())
                            .zoom_by(recipe.zoom);
                        self.viewports
                            .view_mut(self.selected.as_deref())
                            .pan_to(egui::vec2(recipe.pan[0], recipe.pan[1]));
                        self.submit_photos();
                    }
                    "final" if smoke.targets == SMART_TARGETS => {
                        self.preferences.processing_quality = ProcessingQuality::Quality;
                        self.submit_photos();
                    }
                    "final" => {}
                    "smart-overview" => {
                        self.preferences.processing_quality = ProcessingQuality::Speed;
                        self.preferences.scope_viewport = true;
                        self.smoke_edit(|recipe| recipe.reconstruction = Some(Default::default()));
                        self.submit_photos();
                    }
                    "smart-100" => {
                        self.viewports.view_mut(self.selected.as_deref()).one_to_one = true
                    }
                    "smart-detail" => self
                        .viewports
                        .view_mut(self.selected.as_deref())
                        .zoom_by(2.),
                    "smart-full" => {}
                    "smart-pan" => self
                        .viewports
                        .view_mut(self.selected.as_deref())
                        .pan_to(egui::vec2(0.8, 0.7)),
                    "smart-edit-detail" => {
                        self.smoke_edit(|recipe| recipe.exposure = 0.25);
                        self.submit_photos();
                        self.smoke_edit(|recipe| recipe.exposure = 0.5);
                        self.submit_photos();
                        let regional_revision = self.view_images.last_request_id();
                        self.submit_photos();
                        assert_eq!(
                            self.view_images.last_request_id(),
                            regional_revision,
                            "ordinary demand started Full before regional delivery"
                        );
                    }
                    "smart-edit" => {}
                    "smart-zoom-pending" => {
                        self.smoke_edit(|recipe| recipe.exposure = 0.6);
                        assert!(
                            self.active_patch().is_none(),
                            "a newer Full frame must supersede old detail during the next edit"
                        );
                        self.submit_photos();
                        assert!(self.busy);
                        self.viewports.view_mut(self.selected.as_deref()).reset();
                    }
                    "smart-crop-zoom" | "smart-crop-active-zoom" | "smart-adaptive-zoom" => {
                        self.viewports
                            .view_mut(self.selected.as_deref())
                            .zoom_by(6.);
                    }
                    "smart-crop-enter" => {
                        self.smoke_edit(|recipe| recipe.exposure = 0.7);
                        self.submit_photos();
                        assert!(self.busy);
                        crate::crop_tool::toggle(ctx);
                    }
                    "smart-crop-leave" => {
                        self.smoke_edit(|recipe| recipe.exposure = 0.8);
                        self.submit_photos();
                        assert!(self.busy);
                        crate::crop_tool::exit(ctx);
                    }
                    // The planner requests the native whole image when memory allows.
                    "smart-transition-full" => self.submit_photos(),
                    "smart-adaptive-edit" => {
                        self.smoke_edit(|recipe| {
                            recipe.photographic.tone_method =
                                foveonic::render::PhotographicMethod::Universal;
                            recipe.photographic.base_response =
                                foveonic::render::BaseResponse::Natural
                        });
                        assert!(self.smoke_recipe().adaptive_tone_active());
                        self.submit_photos();
                    }
                    "smart-fit" => self.viewports.view_mut(self.selected.as_deref()).reset(),
                    "cache-settings" => self.settings_open = true,
                    "cache-list" => {
                        self.settings_open = false;
                        self.preferences.organizer = Organizer::List;
                    }
                    "cache-reopen" => {
                        self.settings_open = false;
                        // Compatible same-photo selection retains the exact current image.
                        let frame = self.active_image().unwrap();
                        let thumbnail_revision =
                            self.browser.rendered_revision(&frame.request.path);
                        assert!(
                            thumbnail_revision.is_some(),
                            "reopen needs a retained browser preview"
                        );
                        smoke.smart_reopen = Some(SmartReopen {
                            texture: std::sync::Arc::downgrade(&frame.texture),
                            task: frame.diagnostic_task,
                            scale: frame.artifact.settings.working_scale,
                            thumbnail_revision,
                        });
                        let path = self.selected.clone().unwrap();
                        self.select(path, false);
                        let selection_revision = self.view_images.last_request_id();
                        self.submit_photos();
                        assert_eq!(
                            self.view_images.last_request_id(),
                            selection_revision,
                            "reselecting the same photo submitted new work"
                        );
                    }
                    "fast" | "fast-again" => {
                        self.preferences.processing_quality = ProcessingQuality::Speed;
                        self.preferences.scope_viewport = true;
                        self.submit_photos();
                    }
                    "full-again" => {
                        self.preferences.processing_quality = ProcessingQuality::Quality;
                        self.submit_photos();
                    }
                    "fast-zoom" => {
                        self.viewports
                            .view_mut(self.selected.as_deref())
                            .zoom_by(3.);
                        self.viewports
                            .view_mut(self.selected.as_deref())
                            .pan_to(egui::vec2(0.6, 0.4));
                    }
                    "fast-edit" => {
                        self.smoke_edit(|recipe| recipe.exposure = 0.5);
                        self.submit_photos();
                    }
                    "viewport-fit" => self.preferences.scope_viewport = true,
                    "viewport-zoom" => self
                        .viewports
                        .view_mut(self.selected.as_deref())
                        .zoom_by(4.),
                    "viewport-pan" => self
                        .viewports
                        .view_mut(self.selected.as_deref())
                        .pan_to(egui::vec2(0.7, 0.3)),
                    "viewport-parade" => self.preferences.scope = Scope::Parade,
                    "viewport-waveform" => self.preferences.scope = Scope::Waveform,
                    "viewport-resize" => ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                        egui::vec2(1200., 800.),
                    )),
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
                        self.submit_photos();
                    }
                    "viewport-fit-again" => {
                        self.viewports.view_mut(self.selected.as_deref()).reset()
                    }
                    "diff-layer" => {
                        self.output = "repair.layers".into();
                        self.channel = 0;
                        self.comparison.difference = true;
                        self.comparison.gain = 8.;
                        self.submit_photos();
                    }
                    "diff-amplified" => self.comparison.gain = 32.,
                    "diff-layer-two" => self.channel = 1,
                    "diff-output" => self.comparison.difference = false,
                    "compare-input" => self.comparison.input = true,
                    "compare-output" => self.comparison.input = false,
                    "diff-rgb" => {
                        self.smoke_edit(|recipe| recipe.saturation = 1.5);
                        self.output = "saturation.rgb".into();
                        self.channel = 2;
                        self.comparison.difference = true;
                        self.comparison.gain = 8.;
                        self.submit_photos();
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
                        self.smoke_edit(|recipe| recipe.saturation = 1.);
                        self.submit_photos();
                    }
                    name if name.starts_with("slide-detail-") => {
                        let fine = name.strip_prefix("slide-detail-").unwrap().parse().unwrap();
                        self.smoke_edit(|recipe| {
                            recipe.reconstruction.as_mut().unwrap().texture.fine = fine;
                        });
                        self.submit_photos();
                    }
                    "leaf-before" | "leaf-after" => {
                        let enabled = smoke.targets[smoke.index] == "leaf-after";
                        self.smoke_edit(|recipe| {
                            recipe.repair =
                                enabled.then_some(foveonic::render::RepairThreshold::Camera);
                            recipe.despeckle = enabled;
                            recipe.reconstruction = None;
                        });
                        self.output = "final".into();
                        self.channel = -1;
                        self.viewports.view_mut(self.selected.as_deref()).one_to_one = false;
                        if !enabled {
                            self.viewports
                                .view_mut(self.selected.as_deref())
                                .zoom_by(48.);
                        }
                        self.viewports
                            .view_mut(self.selected.as_deref())
                            .pan_to(egui::vec2(1582. / 3136., 2737. / 4704.));
                        self.submit_photos();
                    }
                    "camera-only" => {
                        self.smoke_edit(|recipe| {
                            recipe.reconstruction = Some(foveonic::noise::Controls {
                                chroma: 0,
                                luminance: 0,
                                ..Default::default()
                            });
                            recipe.despeckle = false;
                        });
                        self.viewports.view_mut(self.selected.as_deref()).one_to_one = true;
                        self.submit_photos();
                    }
                    "bolt-before" | "bolt-after" => {
                        self.output = "final".into();
                        self.channel = -1;
                        self.smoke_edit(|recipe| {
                            recipe.despeckle = smoke.targets[smoke.index] == "bolt-after"
                        });
                        if smoke.targets[smoke.index] == "bolt-before" {
                            self.viewports
                                .view_mut(self.selected.as_deref())
                                .zoom_by(4.);
                        }
                        self.viewports
                            .view_mut(self.selected.as_deref())
                            .pan_to(egui::vec2(0.79, 0.05));
                        self.submit_photos();
                    }
                    "white-before" | "white-after" => self
                        .viewports
                        .view_mut(self.selected.as_deref())
                        .pan_to(egui::vec2(0.29, 0.757)),
                    "final-disabled" | "final-default" => {
                        let enabled = smoke.targets[smoke.index] == "final-default";
                        self.smoke_edit(|recipe| {
                            recipe.despeckle = enabled;
                            recipe.repair =
                                enabled.then_some(foveonic::render::RepairThreshold::Camera);
                            recipe.reconstruction = None;
                        });
                        self.output = "final".into();
                        self.channel = -1;
                        self.viewports.view_mut(self.selected.as_deref()).reset();
                        self.submit_photos();
                    }
                    "reconstructed" => {
                        if editor_review_active() {
                            self.preferences.collapsed =
                                [Section::Navigator, Section::Scopes].into();
                        }
                        self.smoke_edit(|recipe| recipe.reconstruction = Some(Default::default()));
                        self.submit_photos();
                    }
                    "fine-soft" => {
                        self.smoke_edit(|recipe| {
                            recipe.reconstruction.as_mut().unwrap().texture.fine = 0.5;
                        });
                        self.submit_photos();
                    }
                    "coarse-large" => {
                        self.smoke_edit(|recipe| {
                            let controls = recipe.reconstruction.as_mut().unwrap();
                            controls.texture.coarse = 0.5;
                            controls.texture.coarse_size = 2.;
                        });
                        self.submit_photos();
                    }
                    "neutral-detail" => {
                        self.smoke_edit(|recipe| {
                            recipe.reconstruction.as_mut().unwrap().texture = Default::default();
                        });
                        self.submit_photos();
                    }
                    "rgb-blue" => self.channel = 2,
                    "layers-two" => {
                        self.output = "noise.layers".into();
                        self.channel = 1;
                        self.submit_photos();
                    }
                    "repair-auto" | "repair-zero" => {
                        self.smoke_edit(|recipe| {
                            recipe.reconstruction = Some(Default::default());
                            recipe.repair = Some(if smoke.targets[smoke.index] == "repair-auto" {
                                foveonic::render::RepairThreshold::Camera
                            } else {
                                foveonic::render::RepairThreshold::Manual(0)
                            })
                        });
                        self.output = "final".into();
                        self.channel = -1;
                        self.submit_photos();
                    }
                    "repair-zoom" => {
                        self.viewports.view_mut(self.selected.as_deref()).one_to_one = true
                    }
                    "bolt" => {
                        self.viewports
                            .view_mut(self.selected.as_deref())
                            .zoom_by(4.);
                        self.viewports
                            .view_mut(self.selected.as_deref())
                            .pan_to(egui::vec2(0.79, 0.05));
                    }
                    "white-edge" => self
                        .viewports
                        .view_mut(self.selected.as_deref())
                        .pan_to(egui::vec2(0.335, 0.75)),
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
                            "color-dq" => self.smoke_edit(|recipe| recipe.color_dq_gain = Some(1.)),
                            "color-dq-off" => {
                                self.smoke_edit(|recipe| recipe.color_dq_gain = Some(0.))
                            }
                            "final-neutral" => {
                                self.smoke_edit(|recipe| {
                                    recipe.repair = None;
                                    recipe.despeckle = false;
                                });
                                self.channel = -1;
                            }
                            _ => unreachable!(),
                        }
                        self.smoke_edit(|recipe| {
                            recipe.reconstruction =
                                (smoke.targets[smoke.index] != "final-neutral").then_some(controls)
                        });
                        self.output = "final".into();
                        self.viewports.view_mut(self.selected.as_deref()).reset();
                        self.submit_photos();
                    }
                    "list" => {
                        self.preferences.organizer = Organizer::List;
                        self.output = "final".into();
                        self.submit_photos();
                    }
                    "parade" => self.preferences.scope = Scope::Parade,
                    "waveform" => self.preferences.scope = Scope::Waveform,
                    "zoom" => self.viewports.view_mut(self.selected.as_deref()).one_to_one = true,
                    "reordered" => {
                        self.viewports
                            .view_mut(self.selected.as_deref())
                            .pan_to(egui::vec2(0.65, 0.5));
                        self.preferences
                            .move_section(Section::Metadata, Section::Navigator, false);
                    }
                    "collapsed" => self.preferences.toggle(Section::Editing),
                    name => {
                        self.viewports.view_mut(self.selected.as_deref()).reset();
                        let last_folder = self.preferences.last_folder.clone();
                        self.preferences = Default::default();
                        self.preferences.last_folder = last_folder;
                        self.output = name.into();
                        self.submit_photos();
                    }
                }
            } else if !smoke.awaiting {
                let ready = if smoke.preview_pending() {
                    self.selected
                        .as_ref()
                        .is_some_and(|p| self.browser.thumbnail(p).is_some())
                } else if smoke.targets == CROP_TARGETS && smoke.targets[smoke.index] == "crop-tile"
                {
                    current_native_feedback
                } else if smoke.targets == COMPARISON_TARGETS && self.multi_edit.multiple() {
                    !self.busy
                        && !self.worker.busy()
                        && self.selection_loaded()
                        && self.multi_edit.editable_ready()
                        && comparison_auxiliary_disabled
                        && comparison_tiles.as_ref().is_some_and(|(ready, _)| *ready)
                } else if smoke.targets == DEHAZE_TARGETS {
                    // These controls require the current photo, independently of ancillary panels.
                    !self.busy
                        && self.active_image().is_some_and(|frame| {
                            self.selected.as_ref() == Some(&frame.request.path)
                                && frame.request.output == self.output
                                && self.shows_current_view(frame)
                        })
                } else if smoke.targets == SMART_TARGETS
                    && matches!(
                        smoke.targets[smoke.index],
                        "smart-detail" | "smart-edit-detail"
                    )
                {
                    current_native_feedback
                } else {
                    !self.busy
                    && self.active_image().is_some_and(|frame| {
                        self.scope_snapshot
                            .as_ref()
                            .is_some_and(|snapshot| Some(snapshot.key) == self.scope_key)
                            && navigator_ready
                            && (frame.request.rendition != crate::editor::Rendition::Current
                                || frame.request.output != "final"
                                || frame.artifact.settings.color_range_preview.is_some()
                                || frame.request.framing_preview
                                || self.browser.rendered_revision(&frame.request.path)
                                    // Thumbnails identify the delivered image, even when
                                    // selection or Before/After starts a new presentation.
                                    == Some(frame.request.id.max(self.photo_layers(&frame.request.path)[0].map_or(RequestId(0), |preview| preview.request.id)).0))
                    })
                };
                let ready = ready
                    && comparison_tiles.as_ref().is_none_or(|(ready, _)| *ready)
                    && (smoke.targets != COMPARISON_TARGETS
                        || smoke.targets[smoke.index] != "comparison-switch"
                        || self
                            .browser
                            .processing_selection(self.selected.as_deref())
                            .get(1)
                            == self.selected.as_ref());
                let ready = ready
                    && (smoke.targets != WORKFLOW_TARGETS
                        || smoke.targets[smoke.index] != "workflow-current"
                        || current_native_whole)
                    && (smoke.targets != VIEWPORT_SCOPE_TARGETS
                        || smoke.targets[smoke.index] != "viewport-resize"
                        || ctx.screen_rect().size() == egui::vec2(1200., 800.));
                let ready = ready
                    && (smoke.targets != FAST_PREVIEW_TARGETS
                        || smoke.preview_pending()
                        || fast_feedback_ready);
                let ready = ready
                    && (smoke.targets != screenshot::TARGETS || self.browser.thumbnails_ready());
                let ready = ready
                    && (smoke.targets != BROWSER_TARGETS
                        || (self.browser.thumbnails_ready() && self.browser.metadata_ready()));
                let ready = ready
                    && (smoke.targets != IMPORT_TARGETS
                        || match smoke.targets[smoke.index] {
                            "import-open" => {
                                !self.folder_loader.loading() && self.browser.import_mode
                            }
                            "import-review" | "import-render-options" => {
                                self.import.smoke_ready(false)
                            }
                            "import-copied" => self.import.smoke_ready(true),
                            _ => true,
                        });
                let ready = ready
                    && (smoke.targets != SMART_TARGETS
                        || match smoke.targets[smoke.index] {
                            "smart-detail" => current_native_feedback,
                            "smart-edit-detail" => {
                                current_native_feedback
                                    && smoke.smart_scrub_checked
                                    && self.smoke_recipe().exposure == 0.75
                            }
                            "smart-transition-full" => current_native_whole,
                            "smart-zoom-pending" | "smart-crop-enter" | "smart-crop-leave" => {
                                self.active_image().is_some_and(|frame| {
                                    self.shows_current_view(frame)
                                        && self.matches_selected(&frame.request.settings)
                                        && frame.request.framing_preview
                                            == (smoke.targets[smoke.index] == "smart-crop-enter")
                                })
                            }
                            "smart-full" | "smart-edit" => current_native_whole,
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
                        || self.active_image().is_some_and(|frame| {
                            self.browser.rendered_revision(&frame.request.path)
                                == Some(frame.request.id.0)
                        }));
                let ready = ready
                    && (smoke.targets != ADVANCED_COLOR_TARGETS
                        || smoke.targets[smoke.index] != "advanced-picked"
                        || (self.smoke_recipe().photographic.color_editor.advanced.count == 1
                            && self
                                .smoke_recipe()
                                .photographic
                                .color_editor
                                .advanced
                                .ranges[0]
                                .adjustment
                                .hue
                                > 0.
                            && self
                                .smoke_recipe()
                                .photographic
                                .color_editor
                                .advanced
                                .ranges[0]
                                .adjustment
                                .lightness
                                > 0.
                            && !crate::editor::direct_color::active(ctx)
                            && !crate::editor::direct_color::dragging(ctx)));
                // The eye previews a range on the selected photo's render
                // without entering its recipe.
                let ready = ready
                    && (smoke.targets != ADVANCED_COLOR_TARGETS
                        || smoke.targets[smoke.index] != "advanced-selection"
                        || (smoke.direct_events.is_empty()
                            && self.range_preview == Some(0)
                            && self.smoke_recipe().color_range_preview.is_none()
                            && self.active_image().is_some_and(|frame| {
                                frame.request.settings.color_range_preview == Some(0)
                            })));
                let ready = ready
                    && (smoke.targets != ADVANCED_COLOR_TARGETS
                        || smoke.targets[smoke.index] != "advanced-name"
                        || (self
                            .smoke_recipe()
                            .photographic
                            .color_editor
                            .advanced
                            .ranges[0]
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
                        Some(self.view_images.last_request_id().0),
                        "renaming must not submit an image job"
                    );
                }
                let ready = ready
                    && (smoke.targets != WHITE_BALANCE_TARGETS
                        || smoke.targets[smoke.index] != "white-balance-picked"
                        || (self.smoke_recipe().white_balance_bias.neutral.is_some()
                            && !crate::editor::white_balance::active(ctx)
                            && !crate::editor::white_balance::pending(ctx)
                            && smoke.direct_events.is_empty()));
                let ready = ready
                    && (smoke.targets != DEHAZE_TARGETS
                        || smoke.targets[smoke.index] != "dehaze-picked"
                        || (self.smoke_recipe().dehaze.shadow.is_some()
                            && !crate::editor::dehaze::active(ctx)
                            && !crate::editor::dehaze::pending(ctx)
                            && smoke.direct_events.is_empty()));
                let ready = ready
                    && (smoke.targets != DIRECT_COLOR_TARGETS
                        || match smoke.targets[smoke.index] {
                            "direct-color-ready" => crate::editor::direct_color::active(ctx),
                            "direct-color-edited" => self
                                .smoke_recipe()
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
                                        && self
                                            .active_image()
                                            .is_some_and(|f| f.request.framing_preview)
                                }
                                "crop-applied" | "crop-reopen" => {
                                    !self.smoke_recipe().crop.is_full()
                                        && self
                                            .active_image()
                                            .is_some_and(|f| f.request.framing_preview)
                                }
                                "crop-pointer" => {
                                    !crate::crop_tool::active(ctx)
                                        && self
                                            .active_image()
                                            .is_some_and(|f| !f.request.framing_preview)
                                }
                                "crop-fast" => self.active_image().is_some_and(|f| {
                                    !f.request.framing_preview
                                        && self.shows_current_view(f)
                                        && f.processing_quality()
                                            == foveonic::render::ProcessingQuality::Speed
                                }),
                                // At 1:1 a native patch covers the view; the full-size
                                // whole image is optional background work.
                                "crop-smart" | "crop-tile" => current_native_feedback,
                                "crop-detail" => self.active_image().is_some_and(|f| {
                                    self.shows_current_view(f)
                                        && f.artifact.settings.working_scale
                                            == foveonic::render::Scale::NATIVE
                                }),
                                _ => true,
                            }));
                let ready = ready
                    && (smoke.targets != rotate::TARGETS
                        || rotate::ready(
                            ctx,
                            smoke,
                            self.smoke_recipe(),
                            self.active_image().filter(|f| self.shows_current_view(f)),
                        ));
                let ready =
                    ready && (smoke.targets != STATUS_TARGETS || smoke.direct_events.is_empty());
                if ready {
                    if smoke.targets == DIRECT_COLOR_TARGETS
                        && smoke.targets[smoke.index] == "direct-color-settings"
                    {
                        smoke.settled_frames += 1;
                        if smoke.settled_frames < 8 {
                            ctx.request_repaint_after(std::time::Duration::from_millis(20));
                            break 'tick;
                        }
                    }
                    if smoke.targets == SMART_TARGETS
                        && smoke.targets[smoke.index] == "cache-settings"
                    {
                        smoke.settled_frames += 1;
                        if smoke.settled_frames < 8 {
                            ctx.request_repaint_after(std::time::Duration::from_millis(25));
                            break 'tick;
                        }
                    }
                    if smoke.targets == screenshot::TARGETS && !smoke.preview_pending() {
                        // Let the selected organizer row and its newly visible thumbnails settle.
                        smoke.settled_frames += 1;
                        if smoke.settled_frames < 3 {
                            ctx.request_repaint();
                            break 'tick;
                        }
                    }
                    smoke.scope_capture =
                        if smoke.targets == VIEWPORT_SCOPE_TARGETS && !smoke.preview_pending() {
                            let key = self.scope_key.expect("ready scope capture key");
                            let pixels = ctx.screen_rect().size() * ctx.pixels_per_point();
                            Some((key, [pixels.x.round() as usize, pixels.y.round() as usize]))
                        } else {
                            None
                        };
                    smoke.awaiting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                } else {
                    smoke.settled_frames = 0;
                }
            }
        }
        self.smoke = Some(held);
    }
}
