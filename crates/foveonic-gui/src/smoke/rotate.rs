//! Keeps rotation smoke gestures, readiness and assertions with their capture steps.

use super::{App, Smoke};
use crate::{rendering::worker::Frame, viewport::Viewport};
use eframe::egui;
use foveonic::render::Settings;

pub(super) const TARGETS: &[&str] = &[
    "preview",
    "final",
    "rotate-ready",
    "rotate-draft",
    "rotate-straight",
    "rotate-first-click",
    "rotate-click-line",
    "rotate-click-applied",
    "rotate-quarter",
    "rotate-freehand-draft",
    "rotate-freehand",
    "rotate-pointer",
    "rotate-reopen",
    "rotate-fast",
    "rotate-smart",
    "rotate-shared-zoom",
    "rotate-crop",
    "rotate-return",
    "rotate-done",
];

pub(super) fn check(
    ctx: &egui::Context,
    smoke: &mut Smoke,
    frame: &Frame,
    settings: &Settings,
    viewport: &Viewport,
) {
    let target = smoke.targets[smoke.index];
    assert_eq!(frame.requested.rotation, settings.rotation);
    match target {
        "rotate-draft" => {
            assert!(
                settings.rotation.is_neutral(),
                "draft must not render or save"
            );
            assert_eq!(
                crate::rotate_tool::displayed(ctx, settings.rotation),
                settings.rotation
            );
        }
        "rotate-straight" => {
            assert!(settings.rotation.degrees < -1.);
            smoke.authored = Some(settings.clone());
        }
        "rotate-first-click" | "rotate-click-line" => {
            assert_eq!(settings.rotation, smoke.authored.as_ref().unwrap().rotation);
            assert_eq!(
                crate::rotate_tool::displayed(ctx, settings.rotation),
                settings.rotation
            );
        }
        "rotate-click-applied" => {
            assert!(
                settings.rotation.degrees < smoke.authored.as_ref().unwrap().rotation.degrees - 1.
            );
            smoke.authored = Some(settings.clone());
        }
        "rotate-quarter" | "rotate-freehand-draft" => {
            assert_eq!(settings.rotation.turns, 1);
            assert_eq!(
                settings.rotation.degrees,
                smoke.authored.as_ref().unwrap().rotation.degrees
            );
        }
        "rotate-freehand" => {
            assert_eq!(settings.rotation.turns, 1);
            assert!(settings.rotation.degrees > smoke.authored.as_ref().unwrap().rotation.degrees);
            smoke.authored = Some(settings.clone());
        }
        "rotate-pointer" | "rotate-reopen" | "rotate-fast" | "rotate-smart" | "rotate-crop"
        | "rotate-done" => {
            assert_eq!(settings.rotation, smoke.authored.as_ref().unwrap().rotation);
        }
        _ => {}
    }
    if target == "rotate-fast" {
        assert!(frame.artifact.settings.working_scale.is_reduced());
    }
    if target == "rotate-smart" {
        assert_eq!(
            frame.artifact.settings.working_scale,
            foveonic::render::Scale::new(2).unwrap()
        );
        assert!(frame.artifact.settings.preview_region.is_none());
    }
    if target == "rotate-shared-zoom" {
        smoke.name_revision = Some(frame.revision);
        smoke.current_texture = Some(frame.texture.clone());
        smoke.fast_region = viewport.visible_pixels();
    }
    if matches!(target, "rotate-crop" | "rotate-return") {
        assert_eq!(
            Some(frame.revision),
            smoke.name_revision,
            "switching tools submitted a render"
        );
        assert!(std::sync::Arc::ptr_eq(
            smoke.current_texture.as_ref().unwrap(),
            &frame.texture
        ));
        assert_eq!(
            viewport.visible_pixels(),
            smoke.fast_region,
            "switching tools reset pan or zoom"
        );
    }
    std::fs::write(
        smoke.directory.join(format!("{target}.json")),
        serde_json::to_vec_pretty(&settings.rotation).unwrap(),
    )
    .unwrap();
}

impl App {
    pub(super) fn smoke_rotate_enter(&mut self) {
        let smoke = self.smoke.as_mut().unwrap();
        match smoke.targets[smoke.index] {
            "rotate-ready" | "rotate-reopen" | "rotate-pointer" | "rotate-crop"
            | "rotate-return" | "rotate-done" => {
                let key = match smoke.targets[smoke.index] {
                    "rotate-pointer" | "rotate-done" => egui::Key::V,
                    "rotate-crop" => egui::Key::C,
                    _ => egui::Key::R,
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
            "rotate-shared-zoom" => {
                self.viewports
                    .view_mut(self.selected.as_deref())
                    .zoom_by(1.3);
                self.viewports
                    .view_mut(self.selected.as_deref())
                    .pan_to(egui::vec2(0.57, 0.45));
            }
            "rotate-fast" | "rotate-smart" => {
                self.preferences.preview_quality = if smoke.targets[smoke.index] == "rotate-fast" {
                    crate::preferences::PreviewMode::Fast
                } else {
                    crate::preferences::PreviewMode::Smart
                };
                self.submit(None);
            }
            "rotate-first-click" | "rotate-click-applied" => {
                let point = self
                    .viewports
                    .view(self.selected.as_deref())
                    .position_at_uv(if smoke.targets[smoke.index] == "rotate-first-click" {
                        [0.25, 0.4]
                    } else {
                        [0.75, 0.5]
                    })
                    .unwrap();
                for pressed in [true, false] {
                    smoke.direct_events.push_back(vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ]);
                }
            }
            "rotate-click-line" => {
                let point = self
                    .viewports
                    .view(self.selected.as_deref())
                    .position_at_uv([0.75, 0.5])
                    .unwrap();
                smoke
                    .direct_events
                    .push_back(vec![egui::Event::PointerMoved(point)]);
            }
            "rotate-quarter" => {
                self.settings.rotation.turn(true, &mut self.settings.crop);
                self.submit(None);
            }
            "rotate-draft" | "rotate-freehand-draft" => {
                let freehand = smoke.targets[smoke.index] == "rotate-freehand-draft";
                if freehand {
                    smoke.direct_events.push_back(
                        [true, false]
                            .map(|pressed| egui::Event::Key {
                                key: egui::Key::R,
                                physical_key: None,
                                pressed,
                                repeat: false,
                                modifiers: egui::Modifiers::SHIFT,
                            })
                            .into(),
                    );
                }
                let start = self
                    .viewports
                    .view(self.selected.as_deref())
                    .position_at_uv(if freehand { [0.75, 0.5] } else { [0.25, 0.4] })
                    .unwrap();
                let end = self
                    .viewports
                    .view(self.selected.as_deref())
                    .position_at_uv(if freehand { [0.74, 0.6] } else { [0.75, 0.5] })
                    .unwrap();
                smoke.direct_events.push_back(vec![
                    egui::Event::PointerMoved(start),
                    egui::Event::PointerButton {
                        pos: start,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ]);
                smoke
                    .direct_events
                    .push_back(vec![egui::Event::PointerMoved(end)]);
            }
            "rotate-straight" | "rotate-freehand" => {
                let point = self
                    .viewports
                    .view(self.selected.as_deref())
                    .position_at_uv(if smoke.targets[smoke.index] == "rotate-freehand" {
                        [0.74, 0.6]
                    } else {
                        [0.75, 0.5]
                    })
                    .unwrap();
                smoke
                    .direct_events
                    .push_back(vec![egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: Default::default(),
                    }]);
            }
            _ => unreachable!("rotation suite action"),
        }
    }
}

pub(super) fn ready(
    ctx: &egui::Context,
    smoke: &Smoke,
    settings: &Settings,
    frame: Option<&Frame>,
    revision: u64,
) -> bool {
    smoke.preview_pending()
        || (smoke.direct_events.is_empty()
            && match smoke.targets[smoke.index] {
                "rotate-draft" => ctx.input(|i| i.pointer.primary_down()),
                "rotate-freehand-draft" => {
                    crate::rotate_tool::displayed(ctx, settings.rotation) != settings.rotation
                }
                "rotate-straight" => settings.rotation.degrees < -1.,
                "rotate-click-applied" => {
                    settings.rotation.degrees
                        < smoke.authored.as_ref().unwrap().rotation.degrees - 1.
                }
                "rotate-crop" => crate::crop_tool::active(ctx),
                "rotate-return" => crate::rotate_tool::editing(ctx),
                "rotate-freehand" => {
                    settings.rotation.degrees > smoke.authored.as_ref().unwrap().rotation.degrees
                }
                _ => true,
            }
            && frame.is_some_and(|f| {
                f.revision == revision
                    && crate::rendering::matching::same_authored_edits(&f.requested, settings)
                    && f.framing_preview
                        == matches!(
                            smoke.targets[smoke.index],
                            "rotate-ready"
                                | "rotate-draft"
                                | "rotate-straight"
                                | "rotate-first-click"
                                | "rotate-click-line"
                                | "rotate-click-applied"
                                | "rotate-quarter"
                                | "rotate-freehand-draft"
                                | "rotate-freehand"
                                | "rotate-reopen"
                                | "rotate-fast"
                                | "rotate-smart"
                                | "rotate-crop"
                                | "rotate-shared-zoom"
                                | "rotate-return"
                        )
            }))
}
