//! Tracks automatic shadow colors and asynchronous patch picking while rejecting stale replies.

use crate::{
    photo_tools::{self, Tool},
    rendering::worker::{ColorPickReply, ColorPickRequest, ColorPickTool},
};
use eframe::egui;
use foveonic::render::Settings;
use std::path::{Path, PathBuf};

#[derive(Clone)]
struct Pending {
    token: u64,
    path: PathBuf,
    expected: Settings,
    result: Option<Result<[f32; 3], String>>,
}
#[derive(Clone)]
struct Estimate {
    settings: Settings,
    color: Option<[f32; 3]>,
}
#[derive(Clone, Default)]
struct State {
    /// The photo pressed by the gesture that ends when the button is released.
    pressed: Option<PathBuf>,
    generation: u64,
    pending: Option<Pending>,
    source: Option<PathBuf>,
    estimate: Option<Estimate>,
}
fn id() -> egui::Id {
    egui::Id::new("Dehaze shadow picker")
}
fn state(ctx: &egui::Context) -> State {
    ctx.data_mut(|d| d.get_temp(id())).unwrap_or_default()
}
fn store(ctx: &egui::Context, s: State) {
    ctx.data_mut(|d| d.insert_temp(id(), s));
}
pub fn source(ctx: &egui::Context, path: Option<&Path>) {
    let mut s = state(ctx);
    if s.source.as_deref() != path {
        s = State {
            source: path.map(Path::to_owned),
            generation: s.generation + 1,
            ..Default::default()
        };
        store(ctx, s);
        photo_tools::release(ctx, Tool::Dehaze);
    }
}
pub(crate) fn estimate_settings(settings: &Settings) -> Settings {
    let mut basis = settings.clone();
    basis.dehaze = Default::default();
    basis.dehaze_estimate = false;
    basis.working_scale = foveonic::render::Scale::NATIVE;
    basis.preview_region = None;
    basis.color_range_preview = None;
    basis
}
pub fn set_estimate(ctx: &egui::Context, settings: &Settings, color: Option<[f32; 3]>) {
    let mut s = state(ctx);
    s.estimate = Some(Estimate {
        settings: estimate_settings(settings),
        color,
    });
    store(ctx, s);
}
pub fn auto_color(ctx: &egui::Context, settings: &Settings) -> Option<[f32; 3]> {
    state(ctx)
        .estimate
        .filter(|estimate| estimate.settings == estimate_settings(settings))
        .and_then(|estimate| estimate.color)
}
pub fn active(ctx: &egui::Context) -> bool {
    photo_tools::selected(ctx) == Tool::Dehaze
}
pub fn pending(ctx: &egui::Context) -> bool {
    state(ctx).pending.is_some()
}
/// The photo whose press or pick is still in progress.
pub fn held_photo(ctx: &egui::Context) -> Option<PathBuf> {
    let s = state(ctx);
    s.pending.map(|pending| pending.path).or(s.pressed)
}
pub(crate) fn cancel(ctx: &egui::Context) {
    let mut s = state(ctx);
    s.pressed = None;
    s.pending = None;
    s.generation += 1;
    store(ctx, s);
}
pub fn exit(ctx: &egui::Context) {
    cancel(ctx);
    photo_tools::release(ctx, Tool::Dehaze);
}
pub fn toggle(ctx: &egui::Context) {
    photo_tools::select(
        ctx,
        if active(ctx) {
            Tool::Pointer
        } else {
            Tool::Dehaze
        },
    );
}
pub fn receive(ctx: &egui::Context, reply: &ColorPickReply) -> bool {
    if reply.tool != ColorPickTool::Dehaze {
        return false;
    }
    let mut s = state(ctx);
    if let Some(p) = &mut s.pending {
        if p.token == reply.token && p.path == reply.path {
            p.result = Some(
                reply
                    .sample
                    .as_ref()
                    .map(|sample| [sample.color[0], sample.color[1], sample.color[2]])
                    .map_err(Clone::clone),
            );
        }
    }
    store(ctx, s);
    true
}
/// Returns the picked shadow color.
pub fn photo(
    ui: &egui::Ui,
    response: &egui::Response,
    uv: Option<[f32; 2]>,
    path: &Path,
    settings: &Settings,
    allowed: bool,
) -> (Option<[f32; 3]>, Option<ColorPickRequest>) {
    let mut s = state(ui.ctx());
    if !allowed || !settings.corrections_enabled {
        s.pending = None;
        s.pressed = None;
        store(ui.ctx(), s);
        return (None, None);
    }
    if s.pending
        .as_ref()
        .is_some_and(|p| p.path != path || p.expected != *settings)
    {
        s.pending = None;
    }
    let mut picked = None;
    if let Some(result) = s.pending.as_mut().and_then(|p| p.result.take()) {
        s.pending = None;
        match result {
            Ok(rgb) => {
                picked = Some(rgb);
                crate::activity::communicate(
                    ui.ctx(),
                    crate::activity::Event::Operation("Haze color selected.".into()),
                );
            }
            Err(error) => {
                crate::activity::communicate(ui.ctx(), crate::activity::Event::Error(error));
            }
        }
    }
    let pan = ui.input(|i| i.key_down(egui::Key::Space));
    if active(ui.ctx()) && response.hovered() && !pan {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    let mut request = None;
    if s.pressed.is_some() && ui.input(|i| i.pointer.primary_released()) {
        photo_tools::release(ui.ctx(), Tool::Dehaze);
        s.pressed = None;
    }
    if active(ui.ctx())
        && !pan
        && response.is_pointer_button_down_on()
        && ui.input(|i| i.pointer.primary_pressed())
    {
        // Without a current image the press does nothing, so the tool stays armed.
        if let Some(uv) = uv {
            s.pressed = Some(path.to_owned());
            s.generation += 1;
            s.pending = Some(Pending {
                token: s.generation,
                path: path.to_owned(),
                expected: settings.clone(),
                result: None,
            });
            request = Some(ColorPickRequest {
                tool: ColorPickTool::Dehaze,
                token: s.generation,
                path: path.to_owned(),
                settings: settings.clone(),
                uv,
            });
            crate::activity::communicate(
                ui.ctx(),
                crate::activity::Event::Operation("Selecting haze color…".into()),
            );
        }
    }
    store(ui.ctx(), s);
    (picked, request)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pickers_disarm_on_release_and_finish_the_pending_selection() {
        for advanced in [false, true] {
            let ctx = egui::Context::default();
            let settings = Settings::default();
            if advanced {
                crate::editor::direct_color::toggle_advanced(&ctx);
            } else {
                toggle(&ctx);
            }
            let frame = |settings: &Settings, events: Vec<egui::Event>, time| {
                let mut result = (false, None);
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(400., 300.),
                        )),
                        events,
                        time: Some(time),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let (_, response) = ui.allocate_exact_size(
                                ui.available_size(),
                                egui::Sense::click_and_drag(),
                            );
                            result = if advanced {
                                let (edits, request) = crate::editor::direct_color::photo(
                                    ui,
                                    &response,
                                    Some([0.5, 0.5]),
                                    Path::new("photo.x3f"),
                                    settings,
                                    true,
                                    true,
                                );
                                (!edits.is_empty(), request)
                            } else {
                                let (picked, request) = photo(
                                    ui,
                                    &response,
                                    Some([0.5, 0.5]),
                                    Path::new("photo.x3f"),
                                    settings,
                                    true,
                                );
                                (picked.is_some(), request)
                            };
                        });
                    },
                );
                result
            };
            let position = egui::pos2(100., 100.);
            let button = |pressed| egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(&settings, vec![], 0.);
            let (_, request) = frame(
                &settings,
                vec![egui::Event::PointerMoved(position), button(true)],
                0.1,
            );
            let request = request.unwrap();
            assert!(if advanced {
                crate::editor::direct_color::active(&ctx)
            } else {
                active(&ctx)
            });
            frame(&settings, vec![button(false)], 0.2);
            assert!(!active(&ctx));
            assert!(!crate::editor::direct_color::active(&ctx));
            assert_eq!(photo_tools::selected(&ctx), Tool::Pointer);
            let reply = ColorPickReply {
                tool: request.tool,
                token: request.token,
                path: request.path,
                sample: Ok(foveonic::render::ColorRangeSample {
                    weights: [0.; 8],
                    color: if advanced {
                        [235., 0.6, 1., 0.]
                    } else {
                        [0.1, 0.2, 0.3, 1.]
                    },
                }),
            };
            if advanced {
                crate::editor::direct_color::receive(&ctx, reply);
            } else {
                receive(&ctx, &reply);
            }
            assert!(frame(&settings, vec![], 0.3).0);
            assert!(!pending(&ctx));
            assert!(!crate::editor::direct_color::dragging(&ctx));
            assert!(frame(&settings, vec![button(true)], 0.4).1.is_none());
        }
    }
    #[test]
    fn the_automatic_color_follows_the_estimated_photo_and_recipe() {
        let ctx = egui::Context::default();
        source(&ctx, Some(Path::new("first.x3f")));
        let mut settings = Settings::default();
        let rgb = [0.12345678, 0.2345679, 0.3456789];
        set_estimate(&ctx, &settings, Some(rgb));
        assert_eq!(auto_color(&ctx, &settings), Some(rgb));
        settings.dehaze.shadow = Some([0.5; 3]);
        settings.dehaze.amount = 45.;
        settings.working_scale = foveonic::render::Scale::new(2).unwrap();
        assert_eq!(auto_color(&ctx, &settings), Some(rgb));
        settings.white_balance = "Shade".into();
        assert_eq!(auto_color(&ctx, &settings), None);
        set_estimate(&ctx, &settings, Some(rgb));
        source(&ctx, Some(Path::new("second.x3f")));
        assert_eq!(auto_color(&ctx, &settings), None);
    }
    #[test]
    fn changed_photos_and_recipes_cannot_receive_old_shadow_colors() {
        for (photo_changed, recipe_changed) in [(false, false), (true, false), (false, true)] {
            let ctx = egui::Context::default();
            let mut settings = Settings::default();
            settings.dehaze.shadow = None;
            photo_tools::select(&ctx, Tool::Dehaze);
            let mut s = state(&ctx);
            s.pending = Some(Pending {
                token: 7,
                path: "first.x3f".into(),
                expected: settings.clone(),
                result: None,
            });
            store(&ctx, s);
            assert!(receive(
                &ctx,
                &ColorPickReply {
                    tool: ColorPickTool::Dehaze,
                    token: 7,
                    path: "first.x3f".into(),
                    sample: Ok(foveonic::render::ColorRangeSample {
                        weights: [0.; 8],
                        color: [0.1, 0.2, 0.3, 1.]
                    })
                }
            ));
            let mut current = settings;
            if recipe_changed {
                current.exposure = 1.;
            }
            let _ = ctx.run(Default::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (_, response) = ui
                        .allocate_exact_size(egui::vec2(200., 200.), egui::Sense::click_and_drag());
                    let (picked, request) = photo(
                        ui,
                        &response,
                        None,
                        Path::new(if photo_changed {
                            "second.x3f"
                        } else {
                            "first.x3f"
                        }),
                        &current,
                        true,
                    );
                    assert_eq!(picked.is_some(), !photo_changed && !recipe_changed);
                    assert!(request.is_none());
                });
            });
            exit(&ctx);
            assert!(!active(&ctx));
        }
    }
}
