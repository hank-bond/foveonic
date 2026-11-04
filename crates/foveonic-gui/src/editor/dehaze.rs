//! Tracks automatic shadow colors and asynchronous patch picking while rejecting stale replies.

use crate::worker::{ColorPickReply, ColorPickRequest, ColorPickTool};
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
pub struct State {
    pub picking: bool,
    pressed: bool,
    pub notice: String,
    generation: u64,
    pending: Option<Pending>,
    source: Option<PathBuf>,
    estimate: Option<Estimate>,
}
fn id() -> egui::Id {
    egui::Id::new("Dehaze shadow picker")
}
pub fn state(ctx: &egui::Context) -> State {
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
    }
}
fn estimate_settings(settings: &Settings) -> Settings {
    let mut basis = settings.clone();
    basis.dehaze = Default::default();
    basis.dehaze_estimate = false;
    basis.preview_quality = foveonic::render::PreviewQuality::Full;
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
pub fn set_auto(ctx: &egui::Context, settings: &mut Settings, automatic: bool) -> bool {
    let color = if automatic {
        None
    } else {
        auto_color(ctx, settings).or(foveonic::render::DehazeControls::default().shadow)
    };
    let changed = settings.dehaze.shadow != color;
    settings.dehaze.shadow = color;
    exit(ctx);
    changed
}
pub fn active(ctx: &egui::Context) -> bool {
    state(ctx).picking
}
pub fn pending(ctx: &egui::Context) -> bool {
    state(ctx).pending.is_some()
}
pub fn exit(ctx: &egui::Context) {
    let mut s = state(ctx);
    s.picking = false;
    s.pressed = false;
    s.pending = None;
    s.generation += 1;
    s.notice.clear();
    store(ctx, s);
}
pub fn toggle(ctx: &egui::Context) {
    let mut s = state(ctx);
    s.picking = !s.picking;
    s.pressed = false;
    s.pending = None;
    s.generation += 1;
    s.notice.clear();
    store(ctx, s);
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
pub fn photo(
    ui: &egui::Ui,
    response: &egui::Response,
    uv: Option<[f32; 2]>,
    path: &Path,
    settings: &mut Settings,
    allowed: bool,
) -> (bool, Option<ColorPickRequest>) {
    if !settings.corrections_enabled {
        exit(ui.ctx());
        return (false, None);
    }
    let mut s = state(ui.ctx());
    if !allowed {
        s.pending = None;
        store(ui.ctx(), s);
        return (false, None);
    }
    if s.pending
        .as_ref()
        .is_some_and(|p| p.path != path || p.expected != *settings)
    {
        s.pending = None;
    }
    let mut changed = false;
    if let Some(result) = s.pending.as_mut().and_then(|p| p.result.take()) {
        s.pending = None;
        match result {
            Ok(rgb) => {
                changed = settings.dehaze.shadow != Some(rgb);
                settings.dehaze.shadow = Some(rgb);
                s.notice.clear();
            }
            Err(error) => s.notice = error,
        }
    }
    let pan = ui.input(|i| i.key_down(egui::Key::Space));
    if s.picking && response.hovered() && !pan {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    let mut request = None;
    if s.pressed && ui.input(|i| i.pointer.primary_released()) {
        s.picking = false;
        s.pressed = false;
    }
    if s.picking
        && !pan
        && response.is_pointer_button_down_on()
        && ui.input(|i| i.pointer.primary_pressed())
    {
        s.pressed = true;
        if let Some(uv) = uv {
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
            s.notice = "Selecting haze color…".into();
        }
    }
    store(ui.ctx(), s);
    (changed, request)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pickers_disarm_on_release_and_finish_the_pending_selection() {
        for advanced in [false, true] {
            let ctx = egui::Context::default();
            let mut settings = Settings::default();
            if advanced {
                crate::editor::direct_color::toggle_advanced(&ctx);
            } else {
                toggle(&ctx);
            }
            let frame = |settings: &mut Settings, events: Vec<egui::Event>, time| {
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
                                crate::editor::direct_color::photo(
                                    ui,
                                    &response,
                                    Some([0.5, 0.5]),
                                    Path::new("photo.x3f"),
                                    settings,
                                    true,
                                )
                            } else {
                                photo(
                                    ui,
                                    &response,
                                    Some([0.5, 0.5]),
                                    Path::new("photo.x3f"),
                                    settings,
                                    true,
                                )
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
            frame(&mut settings, vec![], 0.);
            let (_, request) = frame(
                &mut settings,
                vec![egui::Event::PointerMoved(position), button(true)],
                0.1,
            );
            let request = request.unwrap();
            assert!(if advanced {
                crate::editor::direct_color::active(&ctx)
            } else {
                active(&ctx)
            });
            frame(&mut settings, vec![button(false)], 0.2);
            assert!(!active(&ctx));
            assert!(!crate::editor::direct_color::active(&ctx));
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
            assert!(frame(&mut settings, vec![], 0.3).0);
            assert!(!pending(&ctx));
            assert!(!crate::editor::direct_color::dragging(&ctx));
            assert!(frame(&mut settings, vec![button(true)], 0.4).1.is_none());
        }
    }
    #[test]
    fn automatic_color_can_be_frozen_adjusted_and_reset_without_losing_precision() {
        let ctx = egui::Context::default();
        source(&ctx, Some(Path::new("first.x3f")));
        let mut settings = Settings::default();
        let rgb = [0.12345678, 0.2345679, 0.3456789];
        set_estimate(&ctx, &settings, Some(rgb));
        assert_eq!(auto_color(&ctx, &settings), Some(rgb));
        assert!(set_auto(&ctx, &mut settings, false));
        assert_eq!(settings.dehaze.shadow, Some(rgb));
        settings.dehaze.shadow.as_mut().unwrap()[0] += 0.001;
        assert_eq!(auto_color(&ctx, &settings), Some(rgb));
        assert!(set_auto(&ctx, &mut settings, true));
        assert_eq!(settings.dehaze.shadow, None);
        assert_eq!(auto_color(&ctx, &settings), Some(rgb));
        settings.dehaze.amount = 45.;
        settings.preview_quality = foveonic::render::PreviewQuality::Fast;
        assert_eq!(auto_color(&ctx, &settings), Some(rgb));
        settings.white_balance = "Shade".into();
        assert_eq!(auto_color(&ctx, &settings), None);
        assert!(set_auto(&ctx, &mut settings, false));
        assert_eq!(
            settings.dehaze.shadow,
            foveonic::render::DehazeControls::default().shadow
        );
        set_estimate(&ctx, &settings, Some(rgb));
        source(&ctx, Some(Path::new("second.x3f")));
        assert_eq!(auto_color(&ctx, &settings), None);
        set_estimate(&ctx, &settings, None);
        assert!(!set_auto(&ctx, &mut settings, false));
        assert_eq!(
            settings.dehaze.shadow,
            foveonic::render::DehazeControls::default().shadow
        );
    }
    #[test]
    fn changed_photos_and_recipes_cannot_receive_old_shadow_colors() {
        for (photo_changed, recipe_changed) in [(false, false), (true, false), (false, true)] {
            let ctx = egui::Context::default();
            let mut settings = Settings::default();
            settings.dehaze.shadow = None;
            let mut s = state(&ctx);
            s.picking = true;
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
                    let (changed, request) = photo(
                        ui,
                        &response,
                        None,
                        Path::new(if photo_changed {
                            "second.x3f"
                        } else {
                            "first.x3f"
                        }),
                        &mut current,
                        true,
                    );
                    assert_eq!(changed, !photo_changed && !recipe_changed);
                    assert!(request.is_none());
                });
            });
            assert_eq!(
                current.dehaze.shadow.is_some(),
                !photo_changed && !recipe_changed
            );
            exit(&ctx);
            assert!(!active(&ctx));
        }
    }
}
