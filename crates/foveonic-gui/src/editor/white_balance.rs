//! Selects a neutral patch asynchronously and keeps Custom balance separate from camera presets.

use crate::worker::{ColorPickReply, ColorPickRequest, ColorPickTool};
use eframe::egui;
use foveonic::render::{Settings, WhiteBalanceBias};
use std::path::{Path, PathBuf};

#[derive(Clone)]
struct Pending {
    token: u64,
    path: PathBuf,
    expected: Settings,
    result: Option<Result<WhiteBalanceBias, String>>,
}

#[derive(Clone, Default)]
struct State {
    active: bool,
    token: u64,
    source: Option<PathBuf>,
    pending: Option<Pending>,
}
fn state(ctx: &egui::Context) -> State {
    ctx.data_mut(|d| d.get_temp(egui::Id::new("White balance picker")))
        .unwrap_or_default()
}
fn store(ctx: &egui::Context, s: State) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("White balance picker"), s));
}
pub fn active(ctx: &egui::Context) -> bool {
    state(ctx).active
}
pub fn pending(ctx: &egui::Context) -> bool {
    state(ctx).pending.is_some()
}
pub fn exit(ctx: &egui::Context) {
    let s = state(ctx);
    store(
        ctx,
        State {
            token: s.token + 1,
            source: s.source,
            ..Default::default()
        },
    );
}
pub fn source(ctx: &egui::Context, path: Option<&Path>) {
    if state(ctx).source.as_deref() != path {
        exit(ctx);
        let mut s = state(ctx);
        s.source = path.map(Path::to_owned);
        store(ctx, s);
    }
}
pub fn button(ui: &mut egui::Ui, height: f32) {
    let on = active(ui.ctx());
    let button = crate::widgets::icon_button_with_height(ui, "Pick white balance", on, height);
    crate::widgets::eyedropper_icon(ui, &button, on);
    if button
        .on_hover_text("Pick a neutral gray or white area in the photograph.")
        .clicked()
    {
        toggle(ui.ctx());
    }
}
pub fn toggle(ctx: &egui::Context) {
    let on = active(ctx);
    crate::photo_tools::hand(ctx);
    let mut s = state(ctx);
    s.active = !on;
    store(ctx, s);
}
pub fn receive(ctx: &egui::Context, reply: &ColorPickReply) -> bool {
    if reply.tool != ColorPickTool::WhiteBalance {
        return false;
    }
    let mut s = state(ctx);
    if let Some(pending) = s
        .pending
        .as_mut()
        .filter(|p| p.token == reply.token && p.path == reply.path)
    {
        pending.result = Some(
            reply
                .sample
                .as_ref()
                .map_err(Clone::clone)
                .and_then(|sample| {
                    WhiteBalanceBias::from_sample([
                        sample.color[0],
                        sample.color[1],
                        sample.color[2],
                    ])
                    .map_err(|e| e.to_string())
                }),
        );
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
    let mut s = state(ui.ctx());
    if !allowed
        || settings.is_monochrome()
        || s.pending
            .as_ref()
            .is_some_and(|p| p.path != path || p.expected != *settings)
    {
        s.pending = None;
    }
    let mut changed = false;
    if let Some(result) = s.pending.as_mut().and_then(|p| p.result.take()) {
        s.pending = None;
        match result {
            Ok(balance) => {
                changed = settings.white_balance_bias != balance;
                settings.white_balance_bias = balance;
                crate::activity::communicate(
                    ui.ctx(),
                    crate::activity::Event::Operation("White balance selected.".into()),
                );
            }
            Err(error) => {
                crate::activity::communicate(ui.ctx(), crate::activity::Event::Error(error));
            }
        }
    }
    let pan = ui.input(|i| i.key_down(egui::Key::Space));
    if s.active && response.hovered() && !pan {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    let mut request = None;
    if s.active && allowed && !settings.is_monochrome() && !pan && response.clicked() {
        if let Some(uv) = uv {
            s.token += 1;
            s.pending = Some(Pending {
                token: s.token,
                path: path.to_owned(),
                expected: settings.clone(),
                result: None,
            });
            crate::activity::communicate(
                ui.ctx(),
                crate::activity::Event::Operation("Selecting white balance…".into()),
            );
            request = Some(ColorPickRequest {
                tool: ColorPickTool::WhiteBalance,
                token: s.token,
                path: path.to_owned(),
                settings: settings.clone(),
                uv,
            });
            s.active = false;
        }
    }
    store(ui.ctx(), s);
    (changed, request)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_waits_for_release_and_rejects_changed_settings() {
        for stale in [false, true] {
            let ctx = egui::Context::default();
            store(
                &ctx,
                State {
                    active: true,
                    ..Default::default()
                },
            );
            let mut settings = Settings::default();
            let mut tick = 0.;
            let mut frame = |settings: &mut Settings, events: Vec<egui::Event>| {
                tick += 0.1;
                let mut result = (false, None);
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(400., 300.),
                        )),
                        events,
                        time: Some(tick),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let (_, response) = ui.allocate_exact_size(
                                ui.available_size(),
                                egui::Sense::click_and_drag(),
                            );
                            result = photo(
                                ui,
                                &response,
                                Some([0.5, 0.5]),
                                Path::new("photo.x3f"),
                                settings,
                                true,
                            );
                        });
                    },
                );
                result
            };
            let pos = egui::pos2(100., 100.);
            let event = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            frame(&mut settings, vec![]);
            assert!(frame(
                &mut settings,
                vec![egui::Event::PointerMoved(pos), event(true)]
            )
            .1
            .is_none());
            assert!(active(&ctx));
            let request = frame(&mut settings, vec![event(false)]).1.unwrap();
            assert!(!active(&ctx));
            if stale {
                settings.exposure = 1.;
            }
            assert!(receive(
                &ctx,
                &ColorPickReply {
                    tool: ColorPickTool::WhiteBalance,
                    token: request.token,
                    path: request.path,
                    sample: Ok(foveonic::render::ColorRangeSample {
                        weights: [0.; 8],
                        color: [1.1, 1., 0.7, 1.]
                    })
                }
            ));
            assert_eq!(frame(&mut settings, vec![]).0, !stale);
            assert_eq!(settings.white_balance_bias.neutral.is_some(), !stale);
            assert!(!pending(&ctx));
            store(
                &ctx,
                State {
                    active: true,
                    ..state(&ctx)
                },
            );
            crate::photo_tools::hand(&ctx);
            assert!(!active(&ctx));
        }
    }
}
