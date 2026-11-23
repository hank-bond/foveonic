//! Owns photograph-tool selection, cancellation and pointer routing, and composes the toolbar.

use crate::{
    crop_tool, editor, loupe_tool, rotate_tool, toolbar,
    widgets::{self, ControlSize},
};
use eframe::egui;
use foveonic::crop::Crop;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Pointer,
    Loupe,
    Crop,
    Rotate,
    ColorRange,
    Dehaze,
    WhiteBalance,
}
impl Tool {
    const ALL: [Self; 7] = [
        Self::Pointer,
        Self::Loupe,
        Self::Crop,
        Self::Rotate,
        Self::ColorRange,
        Self::Dehaze,
        Self::WhiteBalance,
    ];
    const TOOLBAR: [Self; 4] = [Self::Pointer, Self::Loupe, Self::Crop, Self::Rotate];
    fn width(self) -> f32 {
        match self {
            Self::Loupe | Self::Crop | Self::Rotate => ControlSize::Large.split_width(),
            _ => ControlSize::Large.side(),
        }
    }
    fn cancel(self, ctx: &egui::Context) {
        match self {
            Self::Pointer => {}
            Self::Loupe => loupe_tool::cancel(ctx),
            Self::Crop => crop_tool::cancel(ctx),
            Self::Rotate => rotate_tool::cancel(ctx),
            Self::ColorRange => editor::direct_color::cancel(ctx),
            Self::Dehaze => editor::dehaze::cancel(ctx),
            Self::WhiteBalance => editor::white_balance::cancel(ctx),
        }
    }
}
pub fn selected(ctx: &egui::Context) -> Tool {
    ctx.data_mut(|data| data.get_temp(egui::Id::new("photograph tool")))
        .unwrap_or_default()
}
/// Explicit selection cancels both gestures and delayed picker replies, including disarmed pickers.
pub fn select(ctx: &egui::Context, tool: Tool) {
    for previous in Tool::ALL {
        previous.cancel(ctx);
    }
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("photograph tool"), tool));
    ctx.request_repaint();
}
/// A one-shot picker returns to Pointer while its validated asynchronous result is still pending.
pub fn release(ctx: &egui::Context, tool: Tool) {
    if selected(ctx) == tool {
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("photograph tool"), Tool::Pointer));
        ctx.request_repaint();
    }
}
pub fn source(ctx: &egui::Context, path: Option<&std::path::Path>) {
    crop_tool::source(ctx, path);
    rotate_tool::source(ctx, path);
    editor::dehaze::source(ctx, path);
    editor::white_balance::source(ctx, path);
}
pub fn takes_pointer(ctx: &egui::Context) -> bool {
    match selected(ctx) {
        Tool::Pointer => false,
        Tool::Loupe | Tool::Rotate => true,
        Tool::Crop | Tool::ColorRange | Tool::Dehaze | Tool::WhiteBalance => true,
    }
}
/// Released pickers still consume their pending reply. Selecting another tool cancels that reply.
/// Each picker returns the edit to the one field it sets.
pub fn pick(
    ui: &egui::Ui,
    response: &egui::Response,
    uv: Option<[f32; 2]>,
    path: &std::path::Path,
    settings: &foveonic::render::Settings,
    allowed: bool,
) -> (
    Option<crate::multi_edit_controls::Edit>,
    Option<crate::rendering::worker::ColorPickRequest>,
) {
    use crate::multi_edit_controls::{field, Edit};
    let mut picked = settings.clone();
    if editor::white_balance::active(ui.ctx()) || editor::white_balance::pending(ui.ctx()) {
        let (changed, request) =
            editor::white_balance::photo(ui, response, uv, path, &mut picked, allowed);
        let edit = Edit::set(field::WHITE_BALANCE_BIAS, &picked.white_balance_bias);
        (changed.then_some(edit), request)
    } else if editor::dehaze::active(ui.ctx()) || editor::dehaze::pending(ui.ctx()) {
        let (changed, request) =
            editor::dehaze::photo(ui, response, uv, path, &mut picked, allowed);
        let edit = Edit::set(field::DEHAZE_SHADOW, &picked.dehaze.shadow);
        (changed.then_some(edit), request)
    } else {
        let (changed, request) =
            editor::direct_color::photo(ui, response, uv, path, &mut picked, allowed);
        let edit = Edit::set(field::COLOR_EDITOR, &picked.photographic.color_editor);
        (changed.then_some(edit), request)
    }
}
pub fn framing(ctx: &egui::Context) -> bool {
    matches!(selected(ctx), Tool::Crop | Tool::Rotate)
}
/// A shared presentation changes only when entering or leaving framing.
pub fn take_view_change(ctx: &egui::Context) -> bool {
    let current = framing(ctx);
    ctx.data_mut(|data| {
        let id = egui::Id::new("framing plane");
        let previous = data.get_temp::<bool>(id).unwrap_or(false);
        data.insert_temp(id, current);
        previous != current
    })
}
pub fn selects_photos(ctx: &egui::Context) -> bool {
    selected(ctx) == Tool::Pointer
}
/// Capture eligibility before drawing tools, since a one-shot picker can return to Pointer on release.
pub fn selection_gesture(
    response: &egui::Response,
    eligible_before_tools: bool,
) -> Option<crate::viewport_selection::Gesture> {
    use crate::viewport_selection::Gesture;
    let (pressed, released, space) = response.ctx.input(|input| {
        (
            input.pointer.primary_pressed(),
            input.pointer.primary_released(),
            input.key_down(egui::Key::Space),
        )
    });
    // Response::clicked reads input through the context. Snapshot it before
    // taking the data lock, including coalesced press/release frames.
    let clicked = response.clicked();
    let press_owned =
        eligible_before_tools && !space && (response.is_pointer_button_down_on() || clicked);
    let id = response.id.with("selection press");
    let owned = response.ctx.data_mut(|data| {
        if pressed {
            data.insert_temp(id, press_owned);
        }
        let owned = data.get_temp::<bool>(id).unwrap_or(false);
        if released {
            data.remove::<bool>(id);
        }
        owned
    });
    if !owned || !eligible_before_tools || !clicked || !selects_photos(&response.ctx) {
        return None;
    }
    response.ctx.input(|input| {
        if input.key_down(egui::Key::Space) {
            None
        } else if input.modifiers.shift {
            Some(Gesture::Range)
        } else if input.modifiers.alt {
            Some(Gesture::Toggle)
        } else {
            Some(Gesture::Replace)
        }
    })
}
pub fn set_viewport_rect(ctx: &egui::Context, rect: egui::Rect) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("photograph viewport"), rect));
}
pub fn viewport_contains(ctx: &egui::Context, pos: egui::Pos2) -> bool {
    viewport_rect(ctx).is_some_and(|rect| rect.contains(pos))
}
pub fn viewport_rect(ctx: &egui::Context) -> Option<egui::Rect> {
    ctx.data_mut(|data| data.get_temp::<egui::Rect>(egui::Id::new("photograph viewport")))
}
pub fn menu_open(ctx: &egui::Context) -> bool {
    crop_tool::menu_open(ctx)
        || rotate_tool::menu_open(ctx)
        || editor::direct_color::menu_open(ctx)
        || loupe_tool::menu_open(ctx)
}
pub fn pointer(ctx: &egui::Context) {
    select(ctx, Tool::Pointer);
}
pub fn escape(ctx: &egui::Context) {
    match selected(ctx) {
        Tool::Crop => crop_tool::escape(ctx),
        Tool::Rotate => rotate_tool::escape(ctx),
        _ => pointer(ctx),
    }
}

pub fn show(
    ui: &mut egui::Ui,
    crop: &mut Crop,
    rotation: &mut foveonic::rotation::Rotation,
    extent: Option<[u32; 2]>,
    custom: &mut Vec<crop_tool::NamedRatio>,
    loupe: &mut loupe_tool::Options,
) -> bool {
    let before = (*crop, *rotation);
    toolbar::tool_group(ui, &Tool::TOOLBAR.map(Tool::width), |ui| {
        for tool in Tool::TOOLBAR {
            match tool {
                Tool::Pointer => {
                    let selected = selects_photos(ui.ctx());
                    let button = widgets::icon_button(ui, "Pointer", selected, ControlSize::Large);
                    pointer_icon(ui, &button, selected);
                    if button
                        .on_hover_text("Pointer (V) · Click to select photographs, drag to pan")
                        .clicked()
                    {
                        pointer(ui.ctx());
                    }
                }
                Tool::Loupe => loupe_tool::button(ui, loupe),
                Tool::Crop => crop_tool::button(ui, crop, extent, custom),
                Tool::Rotate => {
                    ui.add_enabled_ui(extent.is_some(), |ui| {
                        rotate_tool::button(ui, rotation, crop)
                    });
                }
                _ => unreachable!("only photograph toolbar buttons are listed"),
            }
        }
    });
    crop_tool::dialog(ui.ctx(), crop, extent, custom);
    before != (*crop, *rotation)
}

fn pointer_icon(ui: &egui::Ui, response: &egui::Response, on: bool) {
    let rect = widgets::icon_rect(ui, response);
    let points = [
        [5., 2.],
        [5., 20.],
        [10., 15.],
        [14., 23.],
        [18., 21.],
        [14., 13.],
        [21., 13.],
    ]
    .map(|[x, y]| rect.min + egui::vec2(x / 24. * rect.width(), y / 24. * rect.height()));
    ui.painter().add(egui::Shape::closed_line(
        points.to_vec(),
        egui::Stroke::new(1.15_f32, widgets::visuals(ui, response, on).text_color()),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn switching_to_pointer_during_a_held_tool_gesture_does_not_select_on_release() {
        let ctx = egui::Context::default();
        let pos = egui::pos2(40., 40.);
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let run = |time, events| {
            let mut gesture = None;
            let _ = ctx.run(
                egui::RawInput {
                    time: Some(time),
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(200., 200.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let response = ui.interact(
                            ui.max_rect(),
                            egui::Id::new("photo"),
                            egui::Sense::click_and_drag(),
                        );
                        gesture = selection_gesture(&response, selects_photos(ctx));
                    });
                },
            );
            gesture
        };
        select(&ctx, Tool::Loupe);
        run(0., vec![]);
        run(0.1, vec![egui::Event::PointerMoved(pos), button(true)]);
        pointer(&ctx);
        assert_eq!(run(0.2, vec![button(false)]), None);
        run(0.5, vec![button(true)]);
        assert_eq!(
            run(0.6, vec![button(false)]),
            Some(crate::viewport_selection::Gesture::Replace)
        );
        let space = |pressed| egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Default::default(),
        };
        run(1., vec![space(true), button(true)]);
        run(1.1, vec![space(false)]);
        assert_eq!(run(1.2, vec![button(false)]), None);
    }
    #[test]
    fn every_activation_path_selects_only_its_own_tool() {
        type Activate = fn(&egui::Context);
        let entries: [(Tool, Activate); 6] = [
            (Tool::Loupe, loupe_tool::activate),
            (Tool::Crop, crop_tool::toggle),
            (Tool::Rotate, |ctx| {
                rotate_tool::activate(ctx, rotate_tool::Mode::Straighten)
            }),
            (Tool::ColorRange, editor::direct_color::toggle),
            (Tool::Dehaze, editor::dehaze::toggle),
            (Tool::WhiteBalance, editor::white_balance::toggle),
        ];
        for previous in Tool::ALL {
            for (target, activate) in entries {
                let ctx = egui::Context::default();
                select(&ctx, previous);
                activate(&ctx);
                let expected =
                    if previous == target && !matches!(target, Tool::Rotate | Tool::Loupe) {
                        Tool::Pointer
                    } else {
                        target
                    };
                assert_eq!(selected(&ctx), expected, "{previous:?} -> {target:?}");
                let active_flags = [
                    loupe_tool::active(&ctx),
                    crop_tool::active(&ctx),
                    rotate_tool::active(&ctx),
                    editor::direct_color::active(&ctx),
                    editor::dehaze::active(&ctx),
                    editor::white_balance::active(&ctx),
                ];
                assert_eq!(
                    active_flags.iter().filter(|on| **on).count(),
                    usize::from(expected != Tool::Pointer)
                );
            }
        }
    }
    #[test]
    fn pointer_is_default_and_only_pointer_selects_photos() {
        let ctx = egui::Context::default();
        assert_eq!(selected(&ctx), Tool::Pointer);
        for tool in Tool::ALL {
            select(&ctx, tool);
            assert_eq!(selects_photos(&ctx), tool == Tool::Pointer);
            assert_eq!(takes_pointer(&ctx), tool != Tool::Pointer);
            release(&ctx, tool);
            assert_eq!(selected(&ctx), Tool::Pointer);
            select(&ctx, tool);
            escape(&ctx);
            assert_eq!(selected(&ctx), Tool::Pointer);
        }
    }
    #[test]
    fn one_shot_picker_release_does_not_also_select_the_photograph() {
        for tool in [Tool::WhiteBalance, Tool::Dehaze] {
            let ctx = egui::Context::default();
            select(&ctx, tool);
            let settings = foveonic::render::Settings::default();
            let mut requests = 0;
            let pos = egui::pos2(100., 100.);
            let mut draw = |time: f64, pressed: Option<bool>| {
                let mut gesture = None;
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(400., 300.),
                        )),
                        time: Some(time),
                        events: pressed.map_or_else(Vec::new, |pressed| {
                            vec![
                                egui::Event::PointerMoved(pos),
                                egui::Event::PointerButton {
                                    pos,
                                    button: egui::PointerButton::Primary,
                                    pressed,
                                    modifiers: egui::Modifiers::NONE,
                                },
                            ]
                        }),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let eligible_before_tools = selects_photos(ctx);
                            let (_, response) = ui.allocate_exact_size(
                                ui.available_size(),
                                egui::Sense::click_and_drag(),
                            );
                            let (_, request) = pick(
                                ui,
                                &response,
                                Some([0.5, 0.5]),
                                std::path::Path::new("photo.x3f"),
                                &settings,
                                true,
                            );
                            requests += usize::from(request.is_some());
                            gesture = selection_gesture(&response, eligible_before_tools);
                        });
                    },
                );
                gesture
            };
            assert_eq!(draw(0., None), None);
            assert_eq!(draw(0.1, Some(true)), None);
            assert_eq!(draw(0.2, Some(false)), None);
            assert_eq!(selected(&ctx), Tool::Pointer, "{tool:?} disarms on release");
            assert_eq!(draw(1., Some(true)), None);
            assert_eq!(
                draw(1.1, Some(false)),
                Some(crate::viewport_selection::Gesture::Replace)
            );
            assert_eq!(requests, 1, "{tool:?} performs its pick");
        }
    }

    #[test]
    fn coalesced_selection_clicks_complete_without_reentering_context_data() {
        let (tx, rx) = std::sync::mpsc::channel();
        // A regression deadlocks its own egui context. Bound the test without
        // joining that thread, so one failure cannot hang the entire suite.
        std::thread::spawn(move || {
            let ctx = egui::Context::default();
            let pos = egui::pos2(100., 100.);
            let mut observed = Vec::new();
            for frame in 0..3 {
                let events = if frame == 0 {
                    Vec::new()
                } else {
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]
                };
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(400., 300.),
                        )),
                        time: Some(frame as f64 * 0.1),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let (_, response) = ui.allocate_exact_size(
                                ui.available_size(),
                                egui::Sense::click_and_drag(),
                            );
                            let clicked = response.clicked();
                            let down = response.is_pointer_button_down_on();
                            let edges = ctx.input(|input| {
                                (
                                    input.pointer.primary_pressed(),
                                    input.pointer.primary_released(),
                                )
                            });
                            let gesture = selection_gesture(&response, selects_photos(ctx));
                            observed.push((clicked, down, edges, gesture));
                        });
                    },
                );
            }
            let _ = tx.send(observed);
        });
        let observed = rx
            .recv_timeout(std::time::Duration::from_secs(3))
            .expect("selection handler did not finish the coalesced click frames");
        assert!(
            observed
                .iter()
                .filter(|(clicked, _, _, _)| *clicked)
                .count()
                >= 2
        );
        for (clicked, down, edges, gesture) in
            observed.into_iter().filter(|(clicked, _, _, _)| *clicked)
        {
            assert!(clicked);
            assert!(!down);
            assert_eq!(edges, (true, true));
            assert_eq!(gesture, Some(crate::viewport_selection::Gesture::Replace));
        }
    }

    #[test]
    fn viewport_routing_uses_the_whole_canvas() {
        let ctx = egui::Context::default();
        assert!(!viewport_contains(&ctx, egui::pos2(200., 100.)));
        set_viewport_rect(
            &ctx,
            egui::Rect::from_min_max(egui::pos2(100., 20.), egui::pos2(700., 420.)),
        );
        for point in [
            egui::pos2(110., 30.),
            egui::pos2(400., 220.),
            egui::pos2(690., 410.),
        ] {
            assert!(viewport_contains(&ctx, point));
        }
        assert!(!viewport_contains(&ctx, egui::pos2(90., 100.)));
    }
    #[test]
    fn crop_and_all_rotation_modes_share_the_editing_plane() {
        let ctx = egui::Context::default();
        crop_tool::toggle(&ctx);
        assert!(take_view_change(&ctx));
        rotate_tool::activate(&ctx, rotate_tool::Mode::Straighten);
        assert!(!crop_tool::active(&ctx));
        assert!(!take_view_change(&ctx));
        for mode in [
            rotate_tool::Mode::Freehand,
            rotate_tool::Mode::Left,
            rotate_tool::Mode::Right,
            rotate_tool::Mode::Straighten,
        ] {
            rotate_tool::activate(&ctx, mode);
            assert!(!take_view_change(&ctx));
        }
        crop_tool::toggle(&ctx);
        assert!(!rotate_tool::active(&ctx));
        assert!(!take_view_change(&ctx));
        pointer(&ctx);
        assert!(take_view_change(&ctx));
    }
}
