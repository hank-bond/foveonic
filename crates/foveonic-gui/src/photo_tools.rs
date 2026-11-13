//! Owns photograph-tool selection, cancellation and pointer routing, and composes the toolbar.

use crate::{
    crop_tool, editor, rotate_tool, toolbar,
    widgets::{self, ControlSize},
};
use eframe::egui;
use foveonic::crop::Crop;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Pointer,
    Crop,
    Rotate,
    ColorRange,
    Dehaze,
    WhiteBalance,
}
impl Tool {
    const ALL: [Self; 6] = [
        Self::Pointer,
        Self::Crop,
        Self::Rotate,
        Self::ColorRange,
        Self::Dehaze,
        Self::WhiteBalance,
    ];
    const TOOLBAR: [Self; 3] = [Self::Pointer, Self::Crop, Self::Rotate];
    fn width(self) -> f32 {
        match self {
            Self::Crop | Self::Rotate => ControlSize::Large.split_width(),
            _ => ControlSize::Large.side(),
        }
    }
    fn cancel(self, ctx: &egui::Context) {
        match self {
            Self::Pointer => {}
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
        Tool::Rotate => true,
        Tool::Crop | Tool::ColorRange | Tool::Dehaze | Tool::WhiteBalance => true,
    }
}
/// Released pickers still consume their pending reply. Selecting another tool cancels that reply.
pub fn pick(
    ui: &egui::Ui,
    response: &egui::Response,
    uv: Option<[f32; 2]>,
    path: &std::path::Path,
    settings: &mut foveonic::render::Settings,
    allowed: bool,
) -> (bool, Option<crate::rendering::worker::ColorPickRequest>) {
    if editor::white_balance::active(ui.ctx()) || editor::white_balance::pending(ui.ctx()) {
        editor::white_balance::photo(ui, response, uv, path, settings, allowed)
    } else if editor::dehaze::active(ui.ctx()) || editor::dehaze::pending(ui.ctx()) {
        editor::dehaze::photo(ui, response, uv, path, settings, allowed)
    } else {
        editor::direct_color::photo(ui, response, uv, path, settings, allowed)
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
    if !eligible_before_tools || !response.clicked() || !selects_photos(&response.ctx) {
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
    crop_tool::menu_open(ctx) || rotate_tool::menu_open(ctx) || editor::direct_color::menu_open(ctx)
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
    enabled: bool,
    crop: &mut Crop,
    rotation: &mut foveonic::rotation::Rotation,
    extent: Option<[u32; 2]>,
    custom: &mut Vec<crop_tool::NamedRatio>,
) -> bool {
    let before = (*crop, *rotation);
    toolbar::tool_group(ui, &Tool::TOOLBAR.map(Tool::width), |ui| {
        ui.add_enabled_ui(enabled, |ui| {
            for tool in Tool::TOOLBAR {
                match tool {
                    Tool::Pointer => {
                        let selected = selects_photos(ui.ctx());
                        let button =
                            widgets::icon_button(ui, "Pointer", selected, ControlSize::Large);
                        pointer_icon(ui, &button, selected);
                        if button
                            .on_hover_text("Pointer (V) · Click to select photographs, drag to pan")
                            .clicked()
                        {
                            pointer(ui.ctx());
                        }
                    }
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
    fn every_activation_path_selects_only_its_own_tool() {
        type Activate = fn(&egui::Context);
        let entries: [(Tool, Activate); 5] = [
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
                let expected = if previous == target && target != Tool::Rotate {
                    Tool::Pointer
                } else {
                    target
                };
                assert_eq!(selected(&ctx), expected, "{previous:?} -> {target:?}");
                let active_flags = [
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
            let mut settings = foveonic::render::Settings::default();
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
                                &mut settings,
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
