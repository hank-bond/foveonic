//! Exercises bounded multi-photo selection, edits and retained image delivery.

use super::{App, Smoke};
use crate::preferences::Section;
use eframe::egui;

pub(super) const TARGETS: &[&str] = &[
    "preview",
    "final",
    "multi-open",
    "multi-eight",
    "multi-replace",
    "multi-option",
    "multi-switch-a",
    "multi-switch-b",
    "multi-option-a",
    "multi-relative",
    "multi-remove",
    "multi-before",
    "multi-current",
    "multi-option-return",
    "multi-shift",
    "multi-space",
    "multi-choice",
];

impl App {
    /// The existing screenshot driver also exercises bounded multi-photo delivery.
    pub(super) fn smoke_multi_tick(&mut self, ctx: &egui::Context) {
        let mut smoke = self.smoke.take().unwrap();
        if let Some(close_at) = smoke.close_at {
            if std::time::Instant::now() >= close_at {
                smoke.capture.request_close(ctx);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
            self.smoke = Some(smoke);
            return;
        }
        if smoke.index >= smoke.targets.len() {
            smoke.capture.request_close(ctx);
            self.smoke = Some(smoke);
            return;
        }
        let target = smoke.targets[smoke.index];
        let unchanged_selection_count = match target {
            "multi-replace" | "multi-switch-a" | "multi-switch-b" => Some(1),
            "multi-option" | "multi-option-a" => Some(2),
            _ => None,
        };
        let unchanged_active = match target {
            "multi-replace" | "multi-switch-a" | "multi-option-a" => smoke.multi_paths.first(),
            "multi-option" | "multi-switch-b" => smoke.multi_paths.get(1),
            _ => None,
        };
        if unchanged_selection_count == Some(self.edit_selection.paths().len())
            && self.selected.as_ref() == unchanged_active
            && smoke.direct_events.is_empty()
        {
            // Check while work may still be pending, as well as at the settled screenshot.
            self.smoke_multi_assert(&smoke, target);
        }
        let screenshot = ctx.input(|input| {
            input.events.iter().find_map(|event| {
                if let egui::Event::Screenshot { image, .. } = event {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = screenshot {
            let progress = self.worker.progress.snapshot();
            if target != "preview" {
                assert!(
                    !self.worker.busy(),
                    "settled multi-edit capture still has pending renders"
                );
                assert!(
                    progress.is_none_or(|progress| !progress.running()),
                    "multi-edit progress is still running after all renders settled"
                );
            }
            if target == "multi-eight" {
                (smoke.multi_cached_textures, smoke.multi_cached_scales) = smoke
                    .multi_paths
                    .iter()
                    .take(2)
                    .map(|path| {
                        let frame = self
                            .active_image()
                            .filter(|frame| frame.request.path == *path)
                            .or_else(|| self.photo_layers(path)[0])
                            .unwrap();
                        (
                            std::sync::Arc::downgrade(&frame.texture),
                            frame.artifact.settings.working_scale,
                        )
                    })
                    .unzip();
            }
            self.smoke_multi_assert(&smoke, target);
            {
                let _artifact = foveonic::diagnostics::scope(
                    foveonic::diagnostics::Category::Scenario,
                    "smoke.artifact.png",
                );
                std::fs::create_dir_all(&smoke.directory).unwrap();
                let bytes: Vec<_> = image
                    .pixels
                    .iter()
                    .flat_map(|pixel| pixel.to_array())
                    .collect();
                image::save_buffer(
                    smoke.directory.join(format!("{target}.png")),
                    &bytes,
                    image.width() as u32,
                    image.height() as u32,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
            let photos: Vec<_> = self
                .edit_selection
                .paths()
                .iter()
                .map(|path| {
                    let frame = self
                        .active_image()
                        .filter(|frame| frame.request.path == *path)
                        .or_else(|| self.photo_layers(path)[0]);
                    let cached = smoke.multi_paths.iter().position(|candidate| candidate == path)
                        .and_then(|index| smoke.multi_cached_textures.get(index))
                        .and_then(std::sync::Weak::upgrade);
                    serde_json::json!({"path":path,"requested":self.multi_edit.settings(path),
                    "delivered":frame.map(|frame| &frame.request.settings),
                    "revision":frame.map(|frame| frame.request.id),
                    "scale":frame.map(|frame| frame.artifact.settings.working_scale.divisor()),
                    "extent":frame.map(|frame| frame.texture.description().extent),
                    "same_cached_texture":frame.zip(cached.as_ref()).map(|(frame,cached)| std::sync::Arc::ptr_eq(&frame.texture,cached))})
                })
                .collect();
            let memory = self.worker.memory_status();
            std::fs::write(smoke.directory.join(format!("{target}.json")),
                serde_json::to_vec_pretty(&serde_json::json!({"target":target,
                    "process_id":std::process::id(),"browser_count":self.browser.processing_selection(self.selected.as_deref()).len(),
                    "active":self.selected,"request_revision":self.last_request_id,"worker_busy":self.worker.busy(),
                    "progress_running":progress.map(|progress| progress.running()),"progress_fraction":progress.map(|progress| progress.fraction),
                    "photos":photos,"retained":memory.resources,
                    "over_target_bytes":memory.over_target_bytes,"sweep_group_ms":smoke.multi_sweep_ms})).unwrap()).unwrap();
            smoke.diagnostic_checked(self.active_image(), self.worker.busy());
            smoke.index += 1;
            smoke.awaiting = false;
            smoke.settled_frames = 0;
            if smoke.index < smoke.targets.len() {
                smoke.diagnostic_begin();
                self.smoke_multi_enter(ctx, &mut smoke);
            } else if let Some(seconds) = std::env::var_os("FOVEONIC_GUI_SMOKE_HOLD_SECONDS")
                .and_then(|value| value.to_str().and_then(|value| value.parse::<u64>().ok()))
            {
                smoke.close_at =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(seconds));
            } else {
                smoke.capture.request_close(ctx);
            }
        } else if !smoke.awaiting && self.smoke_multi_ready(&smoke, target) {
            if target == "multi-choice" && self.smoke_multi_sweep(&mut smoke) {
                smoke.settled_frames = 0;
            } else if target == "multi-eight" && smoke.multi_expected.is_empty() {
                smoke.multi_paths = self.edit_selection.paths().to_vec();
                assert_eq!(
                    smoke.multi_paths.len(),
                    8,
                    "multi-edit smoke needs at least eight photographs"
                );
                for (index, path) in smoke.multi_paths.iter().enumerate() {
                    let mut settings = self.multi_edit.settings(path).unwrap().clone();
                    settings.exposure = index as f32 * 0.1;
                    settings.exposure_auto = false;
                    settings.tone_enabled = true;
                    settings.photographic.tone_method =
                        foveonic::render::PhotographicMethod::Universal;
                    settings.photographic.base_response = if index % 2 == 0 {
                        foveonic::render::BaseResponse::Camera
                    } else {
                        foveonic::render::BaseResponse::Natural
                    };
                    self.sidecars.apply(path.clone(), settings.clone());
                    self.multi_edit.observe_requested(path, &settings);
                    smoke.multi_expected.push(settings);
                }
                self.settings = self
                    .multi_edit
                    .settings(self.selected.as_ref().unwrap())
                    .unwrap()
                    .clone();
                self.submit_photos();
            } else {
                // Let injected input, layout and the last accepted image reach the screen.
                smoke.settled_frames += 1;
                if smoke.settled_frames >= 3 {
                    smoke.awaiting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                }
            }
        } else {
            smoke.settled_frames = 0;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(25));
        self.smoke = Some(smoke);
    }

    fn smoke_multi_ready(&mut self, smoke: &Smoke, target: &str) -> bool {
        if target == "preview" {
            return self
                .selected
                .as_ref()
                .is_some_and(|path| self.browser.thumbnail(path).is_some());
        }
        if target == "multi-open" {
            return !self.folder_loader.loading()
                && self.browser.import_mode
                && self.selection_loaded()
                && !self.worker.busy();
        }
        if !smoke.direct_events.is_empty() || self.worker.busy() {
            return false;
        }
        if target == "final" {
            return self
                .active_image()
                .is_some_and(|frame| self.selected.as_ref() == Some(&frame.request.path));
        }
        let count = match target {
            "multi-replace" | "multi-switch-a" | "multi-switch-b" | "multi-remove"
            | "multi-before" | "multi-current" => 1,
            "multi-option" | "multi-option-a" | "multi-relative" | "multi-option-return" => 2,
            _ => 8,
        };
        if self.edit_selection.paths().len() != count {
            return false;
        }
        if target == "multi-before" {
            return self
                .active_image()
                .is_some_and(|frame| frame.request.rendition == crate::editor::Rendition::Camera);
        }
        if !self.multi_edit.editable_ready() {
            return false;
        }
        self.edit_selection.paths().iter().all(|path| {
            let frame = self
                .active_image()
                .filter(|frame| {
                    frame.request.path == *path
                        && frame.request.rendition == crate::editor::Rendition::Current
                })
                .or_else(|| self.photo_layers(path)[0]);
            let expected = smoke
                .multi_paths
                .iter()
                .position(|candidate| candidate == path)
                .map(|index| &smoke.multi_expected[index]);
            frame.is_some_and(|frame| {
                expected.is_none_or(|expected| {
                    crate::rendering::matching::same_authored_edits(
                        &frame.request.settings,
                        expected,
                    )
                }) && frame.processing_quality() == self.preferences.processing_quality
                    && frame.artifact.settings.working_scale.divisor()
                        <= self
                            .preview_scale_for(path, self.preferences.processing_quality)
                            .map_or(0, |scale| scale.divisor())
            })
        })
    }

    fn smoke_multi_assert(&self, smoke: &Smoke, target: &str) {
        if target == "multi-eight" {
            for path in &smoke.multi_paths {
                let frame = self
                    .active_image()
                    .filter(|frame| frame.request.path == *path)
                    .or_else(|| self.photo_layers(path)[0])
                    .unwrap();
                assert_eq!(
                    frame.processing_quality(),
                    self.preferences.processing_quality
                );
                let bound = super::automatic_scale_bound(
                    frame,
                    self.viewports.view(Some(path)),
                    self.preferences.processing_quality,
                    self.context.pixels_per_point(),
                )
                .unwrap();
                assert!(
                    frame.artifact.settings.working_scale.divisor() <= bound.divisor(),
                    "multi-photo preview is coarser than physical display demand"
                );
            }
        }
        if target.starts_with("multi-") && target != "multi-open" {
            assert!(
                self.browser.import_mode,
                "multi-edit smoke must keep Import's memory-only policy"
            );
        }
        if matches!(
            target,
            "multi-replace" | "multi-switch-a" | "multi-remove" | "multi-before" | "multi-current"
        ) {
            assert_eq!(self.edit_selection.paths(), &smoke.multi_paths[..1]);
            assert_eq!(self.selected.as_ref(), smoke.multi_paths.first());
        }
        if target == "multi-switch-b" {
            assert_eq!(self.edit_selection.paths(), &smoke.multi_paths[1..2]);
            assert_eq!(self.selected.as_ref(), smoke.multi_paths.get(1));
        }
        if matches!(
            target,
            "multi-option" | "multi-option-a" | "multi-relative" | "multi-option-return"
        ) {
            assert_eq!(self.edit_selection.paths(), &smoke.multi_paths[..2]);
        }
        if matches!(
            target,
            "multi-replace"
                | "multi-option"
                | "multi-switch-a"
                | "multi-switch-b"
                | "multi-option-a"
        ) {
            for path in self.edit_selection.paths() {
                let index = smoke
                    .multi_paths
                    .iter()
                    .position(|candidate| candidate == path)
                    .unwrap();
                let cached = &smoke.multi_cached_textures[index];
                let cached = cached
                    .upgrade()
                    .expect("unchanged overview texture was discarded");
                let frame = self
                    .active_image()
                    .filter(|frame| frame.request.path == *path)
                    .or_else(|| self.photo_layers(path)[0])
                    .unwrap();
                assert_eq!(
                    frame.artifact.settings.working_scale, smoke.multi_cached_scales[index],
                    "selecting an unchanged overview photo changed its working scale: target={target}, path={}, scale={:?}, zoom={:?}, explicit_zoom={}, rectangles={:?}, task={}, task_revision={}, purpose={}, app_revision={}, requested_scale={:?}",
                    path.display(),
                    self.preview_scale_for(path, self.preferences.processing_quality),
                    self.viewports.view(Some(path)).zoom_percent(self.context.pixels_per_point()),
                    self.viewports.view(Some(path)).explicit_zoom(),
                    self.viewports.view(Some(path)).image_rectangles(),
                    frame.diagnostic_task.id,
                    frame.diagnostic_task.revision,
                    frame.diagnostic_task.purpose,
                    self.last_request_id,
                    frame.request.settings.working_scale,
                );
                assert!(
                    std::sync::Arc::ptr_eq(&cached, &frame.texture),
                    "selecting an unchanged overview photo replaced its cached image"
                );
            }
        }
        if matches!(target, "multi-shift" | "multi-space" | "multi-choice") {
            assert_eq!(self.edit_selection.paths(), smoke.multi_paths);
        }
        if matches!(target, "multi-remove" | "multi-before" | "multi-current") {
            assert_eq!(
                self.sidecars.requested_settings(&smoke.multi_paths[1]),
                Some(&smoke.multi_expected[1]),
                "removing another target or showing Before lost its latest edits"
            );
        }
        if target == "multi-space" {
            assert_eq!(
                crate::photo_tools::selected(&self.context),
                crate::photo_tools::Tool::Pointer
            );
        }
    }

    fn smoke_multi_enter(&mut self, ctx: &egui::Context, smoke: &mut Smoke) {
        use crate::multi_edit_controls::{Edit, Mapping, NumberChange};
        let target = smoke.targets[smoke.index];
        match target {
            "final" => {}
            "multi-open" => {
                let folder = self.selected.as_ref().unwrap().parent().unwrap().to_owned();
                self.open(folder, crate::preferences::FolderMode::Import);
            }
            "multi-eight" => {
                self.preferences.multi_edit_limit = 8;
                self.preferences.collapsed =
                    [Section::Navigator, Section::Scopes, Section::Metadata].into();
                self.preferences
                    .editing_sections
                    .retain(|section| *section != crate::preferences::EditingSection::Tone);
                self.preferences
                    .editing_sections
                    .insert(0, crate::preferences::EditingSection::Tone);
                crate::photo_tools::pointer(ctx);
                self.browser.set_actions(vec![(
                    crate::shortcuts::BrowserAction::SelectAll,
                    egui::Modifiers::COMMAND,
                )]);
            }
            "multi-replace" | "multi-switch-a" => {
                self.smoke_multi_click(ctx, smoke, 0, egui::Modifiers::NONE)
            }
            "multi-switch-b" => self.smoke_multi_click(ctx, smoke, 1, egui::Modifiers::NONE),
            "multi-option-a" => self.smoke_multi_click(ctx, smoke, 0, egui::Modifiers::ALT),
            "multi-option" | "multi-option-return" => {
                self.smoke_multi_click(ctx, smoke, 1, egui::Modifiers::ALT)
            }
            "multi-relative" | "multi-remove" => {
                let delta = if target == "multi-relative" {
                    0.25
                } else {
                    -0.125
                };
                let edit = Edit::Number {
                    pointer: "/exposure".into(),
                    change: NumberChange::Relative(delta),
                    range: [-4., 4.],
                    mapping: Mapping::Identity,
                };
                assert!(self.multi_edit.apply_edits(&[edit], &mut self.sidecars));
                for path in self.edit_selection.paths() {
                    let index = smoke
                        .multi_paths
                        .iter()
                        .position(|candidate| candidate == path)
                        .unwrap();
                    smoke.multi_expected[index] = self.multi_edit.settings(path).unwrap().clone();
                }
                self.settings = self
                    .multi_edit
                    .settings(self.selected.as_ref().unwrap())
                    .unwrap()
                    .clone();
                self.submit_photos();
                if target == "multi-relative" {
                    // Viewing A preserves the two-target subset before removing B.
                    if self.selected.as_ref() != smoke.multi_paths.first() {
                        self.select(smoke.multi_paths[0].clone(), true);
                    }
                } else {
                    self.smoke_multi_click(ctx, smoke, 1, egui::Modifiers::ALT);
                }
            }
            "multi-before" => self.toggle_before(crate::editor::Rendition::Camera),
            "multi-current" => self.toggle_before(crate::editor::Rendition::Camera),
            "multi-shift" => {
                // Shift follows the drawn tile order, which can differ from
                // browser order when photographs have different aspect ratios.
                assert_eq!(smoke.multi_paths.len(), 8);
                let (first, last) = self.smoke_visual_range(&smoke.multi_paths);
                self.smoke_multi_click(ctx, smoke, first, egui::Modifiers::NONE);
                self.smoke_multi_click(ctx, smoke, last, egui::Modifiers::SHIFT);
            }
            "multi-space" => {
                crate::photo_tools::pointer(ctx);
                let pos = self
                    .viewports
                    .view(Some(&smoke.multi_paths[1]))
                    .response()
                    .expect("Space-click tile is drawn")
                    .rect
                    .center();
                smoke
                    .direct_events
                    .push_back(vec![egui::Event::PointerMoved(pos), space_key(true)]);
                self.smoke_multi_click(ctx, smoke, 1, egui::Modifiers::NONE);
                smoke.direct_events.push_back(vec![space_key(false)]);
            }
            "multi-choice" => {
                crate::photo_tools::pointer(ctx);
                let edit = Edit::Assign {
                    pointer: "/photographic/base_response".into(),
                    value: serde_json::to_value(foveonic::render::BaseResponse::Natural).unwrap(),
                };
                assert!(self.multi_edit.apply_edits(&[edit], &mut self.sidecars));
                for (index, path) in smoke.multi_paths.iter().enumerate() {
                    smoke.multi_expected[index] = self.multi_edit.settings(path).unwrap().clone();
                    assert_eq!(
                        smoke.multi_expected[index].photographic.base_response,
                        foveonic::render::BaseResponse::Natural
                    );
                }
                self.settings = self
                    .multi_edit
                    .settings(self.selected.as_ref().unwrap())
                    .unwrap()
                    .clone();
                self.submit_photos();
            }
            _ => unreachable!("unknown multi-edit smoke step"),
        }
    }

    pub(super) fn smoke_visual_range(&self, paths: &[std::path::PathBuf]) -> (usize, usize) {
        let mut tiles: Vec<_> = paths
            .iter()
            .enumerate()
            .map(|(index, path)| {
                let rect = self
                    .viewports
                    .view(Some(path))
                    .response()
                    .expect("range-selection tile is drawn")
                    .rect;
                (index, rect)
            })
            .collect();
        tiles.sort_by(|(_, a), (_, b)| {
            a.top()
                .total_cmp(&b.top())
                .then_with(|| a.left().total_cmp(&b.left()))
        });
        (tiles.first().unwrap().0, tiles.last().unwrap().0)
    }

    pub(super) fn smoke_multi_click(
        &self,
        ctx: &egui::Context,
        smoke: &mut Smoke,
        index: usize,
        modifiers: egui::Modifiers,
    ) {
        let path = &smoke.multi_paths[index];
        let response = ctx
            .read_response(self.viewports.view(Some(path)).id())
            .unwrap_or_else(|| panic!("comparison tile is not visible: {}", path.display()));
        let pos = response.rect.center();
        foveonic::diagnostics::event(
            foveonic::diagnostics::Category::Scenario,
            "scenario.pointer_click",
            |fields| {
                fields
                    .str("source", &path.to_string_lossy())
                    .u64("photo", foveonic::diagnostics::photo_id(path))
                    .f64("x", pos.x as f64)
                    .f64("y", pos.y as f64)
                    .bool("alt", modifiers.alt)
                    .bool("shift", modifiers.shift);
            },
        );
        smoke.direct_events.extend([true, false].map(|pressed| {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers,
                },
            ]
        }));
    }

    fn smoke_multi_sweep(&mut self, smoke: &mut Smoke) -> bool {
        if let Some(started) = smoke.multi_sweep_started.take() {
            smoke
                .multi_sweep_ms
                .push(started.elapsed().as_secs_f64() * 1000.);
        }
        if smoke.multi_sweep_ms.len() >= smoke.multi_sweep_groups {
            return false;
        }
        use crate::multi_edit_controls::{Edit, Mapping, NumberChange};
        let started = std::time::Instant::now();
        let edit = Edit::Number {
            pointer: "/exposure".into(),
            change: NumberChange::Relative(0.01),
            range: [-4., 4.],
            mapping: Mapping::Identity,
        };
        assert!(
            self.multi_edit.apply_edits(&[edit], &mut self.sidecars),
            "multi-edit sweep reached the exposure limit"
        );
        for (index, path) in smoke.multi_paths.iter().enumerate() {
            smoke.multi_expected[index] = self.multi_edit.settings(path).unwrap().clone();
        }
        self.settings = self
            .multi_edit
            .settings(self.selected.as_ref().unwrap())
            .unwrap()
            .clone();
        self.submit_photos();
        smoke.multi_sweep_started = Some(started);
        true
    }
}

fn space_key(pressed: bool) -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Space,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Default::default(),
    }
}
