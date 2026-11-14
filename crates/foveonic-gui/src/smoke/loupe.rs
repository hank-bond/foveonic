//! Exercises held native-detail gestures, cancellation and transient photo ownership in the real app.

use crate::{
    editor::Rendition,
    loupe_tool::{self, Options, Size, Zoom},
    photo_tools,
    preferences::PreviewMode,
    rendering::{matching, worker},
    viewport_selection::{Gesture, Request},
    App,
};
use eframe::egui;
use foveonic::render::{BaseResponse, PhotographicMethod, Scale, Settings};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

type ImageId = (u64, u64);

#[derive(Clone, Copy)]
struct Detail {
    image: ImageId,
    region: [u32; 4],
    focus: [f32; 2],
    required: [u32; 4],
}

#[derive(Clone, Copy, Debug)]
enum Phase {
    Setup,
    Ready,
    First,
    PartialHeld,
    PartialCapture,
    Dragged,
    ReleaseHeld,
    Released,
    ToolHeld,
    ToolCancelled,
    RemovalHeld,
    Removed,
    TransientComposed,
    TransientReady,
    TransientDetail,
    TransientReleased,
    OwnersRestored,
    AdaptiveReady,
    AdaptiveDetail,
    AdaptiveDragged,
    AdaptiveReleased,
    SingleReady,
    SingleDetail,
    SingleMotionCached,
    SingleMotionCachedSettled,
    SingleMotionSweep,
    SingleMotionSweepSettled,
    ReturnHeld,
    ReturnCovered,
    ReturnReleased,
    ReturnRequested,
    SingleReleased,
    ZoomHeld,
    ZoomCapture,
    ZoomReleased,
    RestoreBrowser,
    Restoring,
    Done,
}

struct Saved {
    active: PathBuf,
    editing: Vec<PathBuf>,
    quality: PreviewMode,
    limit: usize,
    options: Options,
    organizer: crate::preferences::Organizer,
    settings: Vec<(PathBuf, Settings)>,
}

pub(super) struct Driver {
    phase: Phase,
    saved: Option<Saved>,
    target: Option<PathBuf>,
    expected_active: Option<PathBuf>,
    expected_editing: Vec<PathBuf>,
    transient_settings: Option<Settings>,
    transient_extent: Option<[u32; 2]>,
    single_whole: bool,
    main: Vec<(PathBuf, Option<ImageId>)>,
    first: Option<Detail>,
    accepted: u64,
    pointer: egui::Pos2,
    hold: Option<worker::DeliveryHold>,
    held_reply_epoch: u64,
    screenshot: Option<egui::UserData>,
    events: Vec<Value>,
}
impl Driver {
    pub(super) fn new() -> Self {
        Self {
            phase: Phase::Setup,
            saved: None,
            target: None,
            expected_active: None,
            expected_editing: Vec::new(),
            transient_settings: None,
            transient_extent: None,
            single_whole: false,
            main: Vec::new(),
            first: None,
            accepted: 0,
            pointer: egui::Pos2::ZERO,
            hold: None,
            held_reply_epoch: 0,
            screenshot: None,
            events: Vec::new(),
        }
    }
    pub(super) fn release(&mut self) {
        self.hold.take();
    }
    pub(super) fn evidence(&self) -> Value {
        json!({"phase":format!("{:?}",self.phase),"single_whole":self.single_whole,"events":self.events})
    }
    fn capture(
        &mut self,
        ctx: &egui::Context,
        smoke: &super::Smoke,
        name: &'static str,
    ) -> Result<bool, String> {
        let Some(tag) = &self.screenshot else {
            let tag = egui::UserData::new(name);
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(tag.clone()));
            self.screenshot = Some(tag);
            return Ok(false);
        };
        let image = ctx.input(|input| {
            input.events.iter().find_map(|event| {
                if let egui::Event::Screenshot {
                    user_data, image, ..
                } = event
                {
                    (user_data == tag).then(|| image.clone())
                } else {
                    None
                }
            })
        });
        let Some(image) = image else { return Ok(false) };
        std::fs::create_dir_all(&smoke.directory).map_err(|error| error.to_string())?;
        let bytes: Vec<_> = image
            .pixels
            .iter()
            .flat_map(|pixel| pixel.to_array())
            .collect();
        image::save_buffer(
            smoke.directory.join(name),
            &bytes,
            image.width() as u32,
            image.height() as u32,
            image::ColorType::Rgba8,
        )
        .map_err(|error| error.to_string())?;
        self.screenshot = None;
        self.events.push(json!({"capture":name,"coverage":"native egui/wgpu frame", "measurement":"screenshot readback and encoding outside edit timing"}));
        Ok(true)
    }
    fn target(&self) -> &Path {
        self.target.as_deref().expect("loupe target is selected")
    }
    fn queue_pointer(
        &mut self,
        app: &App,
        smoke: &mut super::Smoke,
        uv: [f32; 2],
        pressed: Option<bool>,
    ) -> Result<(), String> {
        self.pointer = app
            .viewports
            .view(Some(self.target()))
            .position_at_uv(uv)
            .ok_or("loupe point is outside the drawn photo")?;
        let mut events = vec![egui::Event::PointerMoved(self.pointer)];
        if let Some(pressed) = pressed {
            events.push(egui::Event::PointerButton {
                pos: self.pointer,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        smoke.direct_events.push_back(events);
        // Observation runs before drawing. Leave a complete draw after each input.
        smoke.direct_events.push_back(vec![]);
        self.events.push(json!({"action":"pointer","photo":self.target(),"uv":uv,"pressed":pressed,"coverage":"egui-input"}));
        Ok(())
    }
    fn release_pointer(&self, smoke: &mut super::Smoke) {
        smoke
            .direct_events
            .push_back(vec![egui::Event::PointerButton {
                pos: self.pointer,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }]);
        smoke.direct_events.push_back(vec![]);
    }
    fn start_motion(
        &self,
        app: &App,
        smoke: &mut super::Smoke,
        name: &'static str,
        width: f32,
    ) -> Result<(), String> {
        let view = app.viewports.view(Some(self.target()));
        let center = view
            .position_at_uv([0.5, 0.5])
            .ok_or("motion center is outside the photo")?;
        let edge = view
            .position_at_uv([0.5 + width, 0.5])
            .ok_or("motion edge is outside the photo")?;
        smoke.pointer_motion = Some(super::motion::Motion::new(name, center, edge - center));
        Ok(())
    }
    fn snapshot_main(&mut self, app: &App, paths: &[PathBuf]) {
        self.expected_active = app.selected.clone();
        self.expected_editing = app.edit_selection.paths().to_vec();
        self.main = paths
            .iter()
            .map(|path| (path.clone(), app.current_photo_frame(path).map(image_id)))
            .collect();
    }
    fn check_main(&self, app: &App) -> Result<(), String> {
        if app.selected != self.expected_active
            || app.edit_selection.paths() != self.expected_editing
            || app.preferences.preview_quality != PreviewMode::Thumb
        {
            return Err(
                "held loupe changed active photo, editing selection or explicit Thumb".into(),
            );
        }
        for (path, expected) in &self.main {
            if app.current_photo_frame(path).map(image_id) != *expected {
                return Err(format!(
                    "loupe replaced the main photograph: {}",
                    path.display()
                ));
            }
        }
        Ok(())
    }
    fn detail(
        &self,
        app: &App,
        ctx: &egui::Context,
        whole: bool,
    ) -> Result<Option<Detail>, String> {
        self.check_main(app)?;
        let Some(gesture) = loupe_tool::gesture(ctx) else {
            return Ok(None);
        };
        if gesture.path != self.target() {
            return Err("loupe gesture borrowed another photo's source".into());
        }
        let Some(frame) = app.multi_edit.detail(self.target()) else {
            return Ok(None);
        };
        let settings = app
            .multi_edit
            .settings(self.target())
            .ok_or("loupe settings not resolved")?;
        let effective = app.preferences.render_settings(settings);
        if !matching::same_authored_edits(&frame.requested, &effective) {
            return Ok(None);
        }
        if frame.path != self.target()
            || !frame.is_current_detail()
            || frame.artifact.settings.working_scale != Scale::NATIVE
            || frame.artifact.settings.preview_region.is_none() != whole
        {
            return Err(
                "loupe did not receive its own Full detail with the expected coverage".into(),
            );
        }
        let region = frame.detail_region();
        let (focus, required) = footprint(frame, &gesture, ctx.pixels_per_point());
        if !contains(region, required) {
            if let Some(base) = app.current_photo_frame(self.target()).filter(|base| {
                matching::same_authored_edits(&base.requested, &effective)
                    && contains(base.detail_region(), required)
            }) {
                if app
                    .loupe_image(self.target(), &effective, required)
                    .map(image_id)
                    != Some(image_id(base))
                {
                    return Err(
                        "loupe preferred uncovered detail over the current whole base".into(),
                    );
                }
            }
            return Ok(None);
        }
        if app
            .loupe_image(self.target(), &effective, required)
            .map(image_id)
            != Some(image_id(frame))
        {
            return Err(
                "loupe drawing did not select the covered Full image with current edits".into(),
            );
        }
        Ok(Some(Detail {
            image: image_id(frame),
            region,
            focus,
            required,
        }))
    }
    fn hold_next(&mut self, app: &App) {
        self.held_reply_epoch = app.memory_reply_epoch;
        self.hold = Some(
            app.worker
                .hold_delivery(self.target().to_owned(), app.revision + 1),
        );
    }
    fn held(&self) -> Result<bool, String> {
        match self
            .hold
            .as_ref()
            .expect("pending detail is held")
            .reached
            .try_recv()
        {
            Ok(()) => Ok(true),
            Err(crossbeam_channel::TryRecvError::Empty) => Ok(false),
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                Err("loupe detail hold disconnected".into())
            }
        }
    }
    fn check_retired(&self, app: &App, ctx: &egui::Context) -> Result<bool, String> {
        if loupe_tool::gesture(ctx).is_some() {
            return Ok(false);
        }
        // Retiring desired work can settle busy before the held executor reply
        // reaches the UI. Observe that reply before starting another gesture.
        if app.worker.busy() || app.memory_reply_epoch == self.held_reply_epoch {
            return Ok(false);
        }
        if app
            .multi_edit
            .detail(self.target())
            .is_some_and(|frame| frame.content_revision() > self.accepted)
        {
            return Err("a cancelled loupe detail was delivered after cancellation".into());
        }
        Ok(true)
    }
    pub(super) fn advance(
        &mut self,
        app: &mut App,
        ctx: &egui::Context,
        smoke: &mut super::Smoke,
        paths: &[PathBuf],
    ) -> Result<bool, String> {
        let result = self.advance_inner(app, ctx, smoke, paths);
        if let Err(error) = &result {
            self.events.push(json!({"failure":error}));
            self.release();
            smoke.pointer_motion = None;
            loupe_tool::cancel(ctx);
        }
        result
    }
    fn advance_inner(
        &mut self,
        app: &mut App,
        ctx: &egui::Context,
        smoke: &mut super::Smoke,
        paths: &[PathBuf],
    ) -> Result<bool, String> {
        if !smoke.direct_events.is_empty() {
            return Ok(false);
        }
        match self.phase {
            Phase::Setup => {
                let active = app
                    .selected
                    .clone()
                    .ok_or("loupe journey needs an active photo")?;
                let settings = paths
                    .iter()
                    .map(|path| {
                        app.multi_edit
                            .settings(path)
                            .cloned()
                            .map(|settings| (path.clone(), settings))
                            .ok_or("loupe journey needs resolved photo settings")
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let target = paths
                    .iter()
                    .find(|path| {
                        **path != active
                            && app.current_photo_frame(path).is_some_and(|frame| {
                                app.multi_edit.settings(path).is_some_and(|settings| {
                                    settings.regional_preview_available(&frame.artifact.metadata)
                                })
                            })
                    })
                    .ok_or(
                        "loupe journey needs an inactive photo with supported regional processing",
                    )?;
                self.saved = Some(Saved {
                    active,
                    editing: app.edit_selection.paths().to_vec(),
                    quality: app.preferences.preview_quality,
                    limit: app.preferences.multi_edit_limit,
                    options: app.preferences.loupe,
                    organizer: app.preferences.organizer,
                    settings,
                });
                self.target = Some(target.clone());
                app.preferences.preview_quality = PreviewMode::Thumb;
                app.preferences.loupe = Options {
                    size: Size::Small,
                    zoom: Zoom::Percent200,
                    centered: true,
                };
                for path in paths {
                    app.viewports.view_mut(Some(path)).reset();
                }
                loupe_tool::activate(ctx);
                app.submit_photos();
                self.phase = Phase::Ready;
            }
            Phase::Ready => {
                if app.worker.busy() || paths.iter().any(|path| !thumb_ready(app, path)) {
                    return Ok(false);
                }
                self.snapshot_main(app, paths);
                self.queue_pointer(app, smoke, [0.2, 0.2], Some(true))?;
                self.phase = Phase::First;
            }
            Phase::First => {
                let Some(first) = self.detail(app, ctx, false)? else {
                    return Ok(false);
                };
                if !self.capture(ctx, smoke, "loupe.png")? {
                    return Ok(false);
                }
                self.events.push(json!({"observed":"Full regional detail","photo":self.target(),"task":first.image,"region":first.region}));
                self.first = Some(first);
                let frame = app.multi_edit.detail(self.target()).unwrap();
                let radius = 40. * ctx.pixels_per_point();
                let uv = focus_uv(
                    frame,
                    [first.region[2] as f32 - radius * 0.5, first.focus[1]],
                );
                self.hold_next(app);
                self.queue_pointer(app, smoke, uv, None)?;
                self.phase = Phase::PartialHeld;
            }
            Phase::PartialHeld => {
                if !self.held()? {
                    return Ok(false);
                }
                self.phase = Phase::PartialCapture;
            }
            Phase::PartialCapture => {
                self.check_main(app)?;
                let settings = app
                    .preferences
                    .render_settings(app.multi_edit.settings(self.target()).unwrap());
                let [Some(base), Some(detail)] = app.loupe_images(self.target(), &settings) else {
                    return Err("partial loupe lost its overview or native overlay".into());
                };
                let first = self.first.unwrap();
                let gesture = loupe_tool::gesture(ctx).ok_or("partial loupe lost its gesture")?;
                let (_, required) = footprint(detail, &gesture, ctx.pixels_per_point());
                if image_id(detail) != first.image
                    || contains(first.region, required)
                    || !overlaps(first.region, required)
                    || self.detail(app, ctx, false)?.is_some()
                {
                    return Err(
                        "partial loupe did not retain an incomplete overlapping section".into(),
                    );
                }
                let overview = image_id(base);
                if !self.capture(ctx, smoke, "loupe-partial.png")? {
                    return Ok(false);
                }
                self.events.push(json!({"observed":"partial loupe retained overview and native section while replacement held", "overview":overview,"detail":first.image,"region":first.region,"required":required}));
                self.release();
                self.queue_pointer(app, smoke, [0.8, 0.8], None)?;
                self.phase = Phase::Dragged;
            }
            Phase::Dragged => {
                let Some(detail) = self.detail(app, ctx, false)? else {
                    return Ok(false);
                };
                if covers(self.first.unwrap().region, detail.focus) {
                    return Err("loupe drag did not leave the initial cached region".into());
                }
                if detail.image == self.first.unwrap().image {
                    return Ok(false);
                }
                self.accepted = detail.image.1;
                self.events.push(json!({"observed":"drag replaced regional detail","task":detail.image,"region":detail.region}));
                self.hold_next(app);
                self.queue_pointer(app, smoke, [0.2, 0.8], None)?;
                self.phase = Phase::ReleaseHeld;
            }
            Phase::ReleaseHeld => {
                if !self.held()? {
                    return Ok(false);
                }
                self.check_main(app)?;
                self.release_pointer(smoke);
                self.phase = Phase::Released;
            }
            Phase::Released => {
                if loupe_tool::gesture(ctx).is_some() {
                    return Ok(false);
                }
                self.release();
                if !self.check_retired(app, ctx)? {
                    return Ok(false);
                }
                self.check_main(app)?;
                self.events
                    .push(json!({"observed":"release hid lens and retired held detail"}));
                self.hold_next(app);
                self.queue_pointer(app, smoke, [0.2, 0.2], Some(true))?;
                self.phase = Phase::ToolHeld;
            }
            Phase::ToolHeld => {
                if !self.held()? {
                    return Ok(false);
                }
                photo_tools::pointer(ctx);
                self.release_pointer(smoke);
                self.phase = Phase::ToolCancelled;
            }
            Phase::ToolCancelled => {
                if loupe_tool::gesture(ctx).is_some() {
                    return Ok(false);
                }
                self.release();
                if !self.check_retired(app, ctx)? {
                    return Ok(false);
                }
                self.check_main(app)?;
                self.events
                    .push(json!({"observed":"changing tool retired held detail"}));
                loupe_tool::activate(ctx);
                self.hold_next(app);
                self.queue_pointer(app, smoke, [0.2, 0.8], Some(true))?;
                self.phase = Phase::RemovalHeld;
            }
            Phase::RemovalHeld => {
                if !self.held()? {
                    return Ok(false);
                }
                app.browser.excluded.insert(self.target().to_owned());
                app.viewport_paths_key = None;
                self.events.push(json!({"action":"remove inspected source from browser demand","photo":self.target(),"coverage":"direct-command"}));
                self.phase = Phase::Removed;
            }
            Phase::Removed => {
                if app.displayed_paths.iter().any(|path| path == self.target())
                    || loupe_tool::gesture(ctx).is_some()
                {
                    return Ok(false);
                }
                self.release();
                if !self.check_retired(app, ctx)? {
                    return Ok(false);
                }
                self.release_pointer(smoke);
                app.browser.excluded.remove(self.target());
                app.viewport_paths_key = None;
                app.sync_viewport_paths();
                let mut settings = self
                    .saved
                    .as_ref()
                    .unwrap()
                    .settings
                    .iter()
                    .find(|(path, _)| path == self.target())
                    .unwrap()
                    .1
                    .clone();
                settings.crop.bounds = [0.1, 0.2, 0.9, 0.8];
                settings.rotation.turns = 1;
                settings.rotation.degrees = 0.;
                app.sidecars
                    .apply(self.target().to_owned(), settings.clone());
                app.multi_edit.observe_requested(self.target(), &settings);
                self.transient_settings = Some(settings);
                app.submit_photos();
                self.phase = Phase::TransientComposed;
            }
            Phase::TransientComposed => {
                if app.worker.busy() {
                    return Ok(false);
                }
                let Some(frame) = app.current_photo_frame(self.target()).filter(|frame| {
                    matching::same_authored_edits(
                        &frame.requested,
                        self.transient_settings.as_ref().unwrap(),
                    )
                }) else {
                    return Ok(false);
                };
                self.transient_extent = Some(frame.camera_extent());
                self.events.push(json!({"observed":"composed Thumb prepared before transient eviction", "photo":self.target(),"source_extent":frame.camera_extent(),"display_extent":frame.native_display_extent()}));
                let saved = self.saved.as_ref().unwrap();
                app.preferences.multi_edit_limit = 1;
                app.sync_viewport_paths();
                select_targets(app, std::slice::from_ref(&saved.active));
                app.select(saved.active.clone(), false);
                app.sync_multi_edit();
                self.phase = Phase::TransientReady;
            }
            Phase::TransientReady => {
                if app.worker.busy()
                    || !app.displayed_paths.iter().any(|path| path == self.target())
                {
                    return Ok(false);
                }
                if app
                    .multi_edit
                    .retained_paths()
                    .iter()
                    .any(|path| path == self.target())
                {
                    return Err(
                        "lowering the editing limit did not evict the inactive loupe target".into(),
                    );
                }
                self.snapshot_main(app, paths);
                loupe_tool::activate(ctx);
                self.queue_pointer(app, smoke, [0.2, 0.2], Some(true))?;
                self.phase = Phase::TransientDetail;
            }
            Phase::TransientDetail => {
                let Some(detail) = self.detail(app, ctx, true)? else {
                    return Ok(false);
                };
                let expected = self.transient_settings.as_ref().unwrap();
                let frame = app.multi_edit.detail(self.target()).unwrap();
                if Some(frame.camera_extent()) != self.transient_extent
                    || frame
                        .requested
                        .preview_region
                        .is_some_and(|region| !contains(region, detail.required))
                {
                    return Err(
                        "transient loupe composed the browser preview dimensions a second time"
                            .into(),
                    );
                }
                if !app
                    .multi_edit
                    .settings(self.target())
                    .is_some_and(|settings| matching::same_authored_edits(settings, expected))
                {
                    return Err("transient loupe borrowed the active photo's settings".into());
                }
                self.events.push(json!({"observed":"unowned photo resolved its own Full detail","photo":self.target(),"task":detail.image,"editing":app.edit_selection.paths()}));
                self.release_pointer(smoke);
                self.phase = Phase::TransientReleased;
            }
            Phase::TransientReleased => {
                if loupe_tool::gesture(ctx).is_some() || app.worker.busy() {
                    return Ok(false);
                }
                if app
                    .multi_edit
                    .retained_paths()
                    .iter()
                    .any(|path| path == self.target())
                {
                    return Err("released loupe retained its newly admitted transient photo".into());
                }
                let saved = self.saved.as_ref().unwrap();
                let settings = &saved
                    .settings
                    .iter()
                    .find(|(path, _)| path == self.target())
                    .unwrap()
                    .1;
                app.sidecars
                    .apply(self.target().to_owned(), settings.clone());
                app.preferences.multi_edit_limit = saved.limit;
                select_targets(app, paths);
                app.sync_multi_edit();
                self.phase = Phase::OwnersRestored;
            }
            Phase::OwnersRestored => {
                if app.worker.busy() || !app.multi_edit.editable_ready() {
                    return Ok(false);
                }
                let target = paths.iter().find(|path| {
                    app.selected.as_ref() != Some(path)
                        && app.multi_edit.metadata(path).is_some_and(|metadata| {
                            metadata.processing_family().ok()
                                == Some(foveonic::source::ProcessingFamily::Merrill)
                        })
                });
                if let Some(target) = target {
                    self.target = Some(target.clone());
                    let mut settings = app.multi_edit.settings(target).unwrap().clone();
                    settings.tone_enabled = true;
                    settings.processing_sections.tone = true;
                    settings.photographic.tone_method = PhotographicMethod::Universal;
                    settings.photographic.base_response = BaseResponse::Natural;
                    app.sidecars.apply(target.clone(), settings.clone());
                    app.multi_edit.observe_requested(target, &settings);
                    app.submit_photos();
                    self.phase = Phase::AdaptiveReady;
                } else {
                    self.events.push(json!({"skipped":"Adaptive regional detail","reason":"no inactive Merrill fixture"}));
                    self.begin_single(app);
                }
            }
            Phase::AdaptiveReady => {
                if app.worker.busy() || !thumb_ready(app, self.target()) {
                    return Ok(false);
                }
                self.snapshot_main(app, paths);
                loupe_tool::activate(ctx);
                self.queue_pointer(app, smoke, [0.2, 0.2], Some(true))?;
                self.phase = Phase::AdaptiveDetail;
            }
            Phase::AdaptiveDetail => {
                let Some(detail) = self.detail(app, ctx, false)? else {
                    return Ok(false);
                };
                self.first = Some(detail);
                self.events.push(json!({"observed":"Full Adaptive regional detail preserved main Thumb","photo":self.target(),"task":detail.image}));
                self.queue_pointer(app, smoke, [0.8, 0.8], None)?;
                self.phase = Phase::AdaptiveDragged;
            }
            Phase::AdaptiveDragged => {
                let Some(detail) = self.detail(app, ctx, false)? else {
                    return Ok(false);
                };
                if app.worker.busy() {
                    return Ok(false);
                }
                if detail.image == self.first.unwrap().image {
                    return Err(
                        "moving Adaptive loupe outside coverage did not render its new region"
                            .into(),
                    );
                }
                self.release_pointer(smoke);
                self.phase = Phase::AdaptiveReleased;
            }
            Phase::AdaptiveReleased => {
                if loupe_tool::gesture(ctx).is_some() || app.worker.busy() {
                    return Ok(false);
                }
                self.check_main(app)?;
                self.begin_single(app);
            }
            Phase::SingleReady => {
                if app.multi_photo_overview() || app.worker.busy() || !app.sidecars.ready() {
                    return Ok(false);
                }
                let path = app
                    .selected
                    .clone()
                    .ok_or("single loupe has no active photo")?;
                if !thumb_ready(app, &path) {
                    return Ok(false);
                }
                let frame = app.current_photo_frame(&path).unwrap();
                self.single_whole = !app
                    .preferences
                    .render_settings(&app.settings)
                    .regional_preview_available(&frame.artifact.metadata);
                self.target = Some(path);
                self.snapshot_main(app, paths);
                loupe_tool::activate(ctx);
                self.queue_pointer(app, smoke, [0.5, 0.5], Some(true))?;
                self.phase = Phase::SingleDetail;
            }
            Phase::SingleDetail => {
                let Some(detail) = self.detail(app, ctx, self.single_whole)? else {
                    return Ok(false);
                };
                if !self.capture(ctx, smoke, "loupe-single.png")? {
                    return Ok(false);
                }
                self.events.push(json!({"observed":"active single-photo Full loupe preserved main Thumb","photo":self.target(),"task":detail.image}));
                self.first = Some(detail);
                self.start_motion(app, smoke, "cached", 0.0005)?;
                self.phase = Phase::SingleMotionCached;
            }
            Phase::SingleMotionCached => {
                if !smoke
                    .pointer_motion
                    .as_ref()
                    .is_some_and(|motion| motion.finished())
                {
                    return Ok(false);
                }
                self.events
                    .push(smoke.pointer_motion.take().unwrap().evidence());
                // The final injected point still needs this frame's loupe draw.
                smoke.direct_events.push_back(vec![]);
                self.phase = Phase::SingleMotionCachedSettled;
            }
            Phase::SingleMotionCachedSettled => {
                let Some(detail) = self.detail(app, ctx, self.single_whole)? else {
                    return Err("cached motion lost its native coverage".into());
                };
                if detail.image != self.first.unwrap().image {
                    return Err("cached motion unexpectedly replaced native detail".into());
                }
                self.start_motion(app, smoke, "sweep", 0.22)?;
                self.phase = Phase::SingleMotionSweep;
            }
            Phase::SingleMotionSweep => {
                if !smoke
                    .pointer_motion
                    .as_ref()
                    .is_some_and(|motion| motion.finished())
                {
                    return Ok(false);
                }
                self.events
                    .push(smoke.pointer_motion.take().unwrap().evidence());
                smoke.direct_events.push_back(vec![]);
                self.phase = Phase::SingleMotionSweepSettled;
            }
            Phase::SingleMotionSweepSettled => {
                if app.worker.busy() {
                    return Ok(false);
                }
                let Some(detail) = self.detail(app, ctx, self.single_whole)? else {
                    return Ok(false);
                };
                self.first = Some(detail);
                if self.single_whole {
                    self.release_pointer(smoke);
                    self.phase = Phase::SingleReleased;
                } else {
                    let frame = app.multi_edit.detail(self.target()).unwrap();
                    let radius = 40. * ctx.pixels_per_point();
                    let uv = focus_uv(
                        frame,
                        [detail.region[2] as f32 - radius * 0.5, detail.focus[1]],
                    );
                    self.hold_next(app);
                    self.queue_pointer(app, smoke, uv, None)?;
                    self.phase = Phase::ReturnHeld;
                }
            }
            Phase::ReturnHeld => {
                if !self.held()? {
                    return Ok(false);
                }
                let frame = app.multi_edit.detail(self.target()).unwrap();
                let uv = focus_uv(frame, self.first.unwrap().focus);
                self.queue_pointer(app, smoke, uv, None)?;
                self.phase = Phase::ReturnCovered;
            }
            Phase::ReturnCovered => {
                let Some(detail) = self.detail(app, ctx, false)? else {
                    return Ok(false);
                };
                if detail.image != self.first.unwrap().image {
                    return Err("return to covered loupe lost the retained native image".into());
                }
                if app.worker.busy() {
                    return Ok(false);
                }
                self.events.push(json!({"observed":"covered loupe withdrew held replacement", "detail":detail.image,"region":detail.region,"required":detail.required}));
                self.held_reply_epoch = app.memory_reply_epoch;
                self.release();
                self.phase = Phase::ReturnReleased;
            }
            Phase::ReturnReleased => {
                if app.worker.busy() || app.memory_reply_epoch == self.held_reply_epoch {
                    return Ok(false);
                }
                let Some(detail) = self.detail(app, ctx, false)? else {
                    return Ok(false);
                };
                if detail.image != self.first.unwrap().image {
                    return Err(
                        "withdrawn replacement was admitted over retained loupe coverage".into(),
                    );
                }
                let frame = app.multi_edit.detail(self.target()).unwrap();
                let radius = 40. * ctx.pixels_per_point();
                let uv = focus_uv(
                    frame,
                    [detail.region[2] as f32 - radius * 0.5, detail.focus[1]],
                );
                self.queue_pointer(app, smoke, uv, None)?;
                self.phase = Phase::ReturnRequested;
            }
            Phase::ReturnRequested => {
                let Some(detail) = self.detail(app, ctx, false)? else {
                    return Ok(false);
                };
                let first = self.first.unwrap();
                if detail.image == first.image {
                    return Err(
                        "leaving retained loupe coverage did not request fresh detail".into(),
                    );
                }
                self.events.push(json!({"observed":"leaving covered loupe requested fresh detail", "previous":first.image,"detail":detail.image,"region":detail.region,"required":detail.required}));
                self.first = Some(detail);
                self.release_pointer(smoke);
                self.phase = Phase::SingleReleased;
            }
            Phase::SingleReleased => {
                if loupe_tool::gesture(ctx).is_some() || app.worker.busy() {
                    return Ok(false);
                }
                self.check_main(app)?;
                if self.single_whole {
                    self.restore_browser(app);
                } else {
                    self.hold_next(app);
                    app.preferences.preview_quality = PreviewMode::Smart;
                    photo_tools::pointer(ctx);
                    let view = app.viewports.view_mut(Some(self.target()));
                    let current = view
                        .zoom_percent(ctx.pixels_per_point())
                        .ok_or("zoom has no geometry")?;
                    view.zoom_by(200. / current);
                    app.submit_photos();
                    self.events.push(json!({"action":"zoom existing native section to 200 percent with replacement held","coverage":"direct-command"}));
                    self.phase = Phase::ZoomHeld;
                }
            }
            Phase::ZoomHeld => {
                if !self.held()? {
                    return Ok(false);
                }
                app.viewports.view_mut(Some(self.target())).pan += egui::vec2(35., 20.);
                app.submit_photos();
                smoke.direct_events.push_back(vec![]);
                self.phase = Phase::ZoomCapture;
            }
            Phase::ZoomCapture => {
                let base = app
                    .current_photo_frame(self.target())
                    .ok_or("zoom lost overview")?;
                let detail = app
                    .photo_detail(
                        self.target(),
                        Some(base),
                        &app.view_settings(),
                        Rendition::Current,
                    )
                    .ok_or("zoom lost compatible native overlay")?;
                let visible = app
                    .viewports
                    .view(Some(self.target()))
                    .visible_pixels()
                    .ok_or("zoom has no visible bounds")?;
                let window = base.display_window(false);
                let required = [
                    visible.top_left[0] + window[0],
                    visible.top_left[1] + window[1],
                    visible.bottom_right[0] + 1 + window[0],
                    visible.bottom_right[1] + 1 + window[1],
                ];
                let first = self.first.unwrap();
                if app.selected.as_deref() != Some(self.target())
                    || image_id(detail) != first.image
                    || contains(first.region, required)
                    || !overlaps(first.region, required)
                {
                    return Err("zoom movement did not preserve the partial native section".into());
                }
                let overview = image_id(base);
                if !self.capture(ctx, smoke, "zoom-partial.png")? {
                    return Ok(false);
                }
                self.events.push(json!({"observed":"zoom movement retained overview and native section while replacement held", "overview":overview,"detail":first.image,"region":first.region,"required":required}));
                self.release();
                app.preferences.preview_quality = PreviewMode::Thumb;
                app.viewports.view_mut(Some(self.target())).reset();
                app.submit_photos();
                self.phase = Phase::ZoomReleased;
            }
            Phase::ZoomReleased => {
                if app.worker.busy() {
                    return Ok(false);
                }
                self.restore_browser(app);
            }
            Phase::RestoreBrowser => {
                if app.viewport_paths.len() != paths.len() || app.worker.busy() {
                    return Ok(false);
                }
                self.restore(app, ctx);
            }
            Phase::Restoring => {
                let saved = self.saved.as_ref().unwrap();
                if app.worker.busy()
                    || !app.multi_edit.editable_ready()
                    || app.selected.as_ref() != Some(&saved.active)
                    || app.edit_selection.paths() != saved.editing
                    || saved.settings.iter().any(|(path, expected)| {
                        !app.multi_edit.settings(path).is_some_and(|settings| {
                            matching::same_authored_edits(settings, expected)
                        })
                    })
                {
                    return Ok(false);
                }
                self.events.push(json!({"observed":"loupe journey restored authored settings and editing selection"}));
                self.phase = Phase::Done;
            }
            Phase::Done => return Ok(true),
        }
        ctx.request_repaint();
        Ok(false)
    }
    fn restore_browser(&mut self, app: &mut App) {
        app.preferences.organizer = self.saved.as_ref().unwrap().organizer;
        app.browser.set_actions(vec![(
            crate::shortcuts::BrowserAction::SelectAll,
            egui::Modifiers::COMMAND,
        )]);
        self.phase = Phase::RestoreBrowser;
    }

    fn begin_single(&mut self, app: &mut App) {
        let saved = self.saved.as_ref().unwrap();
        for (path, settings) in &saved.settings {
            app.sidecars.apply(path.clone(), settings.clone());
            app.multi_edit.observe_requested(path, settings);
        }
        let index = app
            .browser
            .files
            .iter()
            .position(|path| app.selected.as_ref() == Some(path))
            .unwrap();
        let key = if index + 1 < app.browser.files.len() {
            egui::Key::ArrowDown
        } else {
            egui::Key::ArrowUp
        };
        app.preferences.organizer = crate::preferences::Organizer::List;
        app.browser.set_actions(vec![(
            crate::shortcuts::BrowserAction::Arrow(key),
            egui::Modifiers::NONE,
        )]);
        self.phase = Phase::SingleReady;
    }
    fn restore(&mut self, app: &mut App, ctx: &egui::Context) {
        let saved = self.saved.as_ref().unwrap();
        for (path, settings) in &saved.settings {
            app.sidecars.apply(path.clone(), settings.clone());
            app.multi_edit.observe_requested(path, settings);
        }
        app.preferences.preview_quality = saved.quality;
        app.preferences.multi_edit_limit = saved.limit;
        app.preferences.loupe = saved.options;
        app.preferences.organizer = saved.organizer;
        select_targets(app, &saved.editing);
        app.select(saved.active.clone(), false);
        photo_tools::pointer(ctx);
        app.submit_photos();
        self.phase = Phase::Restoring;
    }
}

fn focus_uv(frame: &worker::Frame, focus: [f32; 2]) -> [f32; 2] {
    let window = frame.requested.crop.pixels(frame.uncropped_extent());
    [
        (focus[0] - window[0] as f32) / (window[2] - window[0]) as f32,
        (focus[1] - window[1] as f32) / (window[3] - window[1]) as f32,
    ]
}

fn thumb_ready(app: &App, path: &Path) -> bool {
    app.current_photo_frame(path).is_some_and(|frame| {
        app.multi_edit.settings(path).is_some_and(|settings| {
            matching::same_authored_edits(
                &frame.requested,
                &app.preferences.render_settings(settings),
            )
        }) && foveonic::render::ControlCapabilities::from_metadata(&frame.artifact.metadata)
            .is_ok_and(|capabilities| {
                frame.artifact.settings.working_scale == capabilities.thumb_scale()
            })
    })
}
fn image_id(frame: &worker::Frame) -> ImageId {
    (frame.diagnostic_task.id, frame.content_revision())
}
fn covers(region: [u32; 4], point: [f32; 2]) -> bool {
    point[0] >= region[0] as f32
        && point[0] < region[2] as f32
        && point[1] >= region[1] as f32
        && point[1] < region[3] as f32
}
// The maintained gesture fixes a 160-point lens at 200%. Its native footprint
// is independent of the production placement and overscan helpers.
fn footprint(
    frame: &worker::Frame,
    gesture: &loupe_tool::Gesture,
    pixels_per_point: f32,
) -> ([f32; 2], [u32; 4]) {
    let window = frame.requested.crop.pixels(frame.uncropped_extent());
    let focus = [
        window[0] as f32 + gesture.focus_uv[0] * (window[2] - window[0]) as f32,
        window[1] as f32 + gesture.focus_uv[1] * (window[3] - window[1]) as f32,
    ];
    let radius = 40. * pixels_per_point;
    (
        focus,
        [
            (focus[0] - radius).floor().max(window[0] as f32) as u32,
            (focus[1] - radius).floor().max(window[1] as f32) as u32,
            (focus[0] + radius).ceil().min(window[2] as f32) as u32,
            (focus[1] + radius).ceil().min(window[3] as f32) as u32,
        ],
    )
}

fn overlaps(a: [u32; 4], b: [u32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

fn contains(outer: [u32; 4], inner: [u32; 4]) -> bool {
    outer[0] <= inner[0] && outer[1] <= inner[1] && outer[2] >= inner[2] && outer[3] >= inner[3]
}
fn select_targets(app: &mut App, paths: &[PathBuf]) {
    for (index, path) in paths.iter().enumerate() {
        app.edit_selection.apply(
            Request {
                path: path.clone(),
                gesture: if index == 0 {
                    Gesture::Replace
                } else {
                    Gesture::Toggle
                },
            },
            app.preferences.multi_edit_limit,
        );
    }
}
