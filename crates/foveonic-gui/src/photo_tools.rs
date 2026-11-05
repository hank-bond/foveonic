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
    Hand,
    Crop,
    Rotate,
    ColorRange,
    Dehaze,
    WhiteBalance,
}
impl Tool {
    const ALL: [Self; 6] = [
        Self::Hand,
        Self::Crop,
        Self::Rotate,
        Self::ColorRange,
        Self::Dehaze,
        Self::WhiteBalance,
    ];
    const TOOLBAR: [Self; 3] = [Self::Hand, Self::Crop, Self::Rotate];
    fn width(self) -> f32 {
        match self {
            Self::Crop | Self::Rotate => ControlSize::Large.split_width(),
            _ => ControlSize::Large.side(),
        }
    }
    fn cancel(self, ctx: &egui::Context) {
        match self {
            Self::Hand => {}
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
/// A one-shot picker returns to Hand while its validated asynchronous result is still pending.
pub fn release(ctx: &egui::Context, tool: Tool) {
    if selected(ctx) == tool {
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("photograph tool"), Tool::Hand));
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
        Tool::Hand => false,
        Tool::Rotate => rotate_tool::editing(ctx),
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
) -> (bool, Option<crate::worker::ColorPickRequest>) {
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
pub fn active(ctx: &egui::Context) -> bool {
    selected(ctx) != Tool::Hand
}
pub fn menu_open(ctx: &egui::Context) -> bool {
    crop_tool::menu_open(ctx) || rotate_tool::menu_open(ctx) || editor::direct_color::menu_open(ctx)
}
pub fn hand(ctx: &egui::Context) {
    select(ctx, Tool::Hand);
}
pub fn escape(ctx: &egui::Context) {
    match selected(ctx) {
        Tool::Crop => crop_tool::escape(ctx),
        Tool::Rotate => rotate_tool::escape(ctx),
        _ => hand(ctx),
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
                    Tool::Hand => {
                        let selected = !active(ui.ctx());
                        let button = widgets::icon_button(ui, "Hand", selected, ControlSize::Large);
                        hand_icon(ui, &button, selected);
                        if button.on_hover_text("Hand (H) · Drag to pan").clicked() {
                            hand(ui.ctx());
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

fn hand_icon(ui: &egui::Ui, response: &egui::Response, on: bool) {
    let rect = widgets::icon_rect(ui, response);
    let p =
        |[x, y]: [f32; 2]| rect.min + egui::vec2(x / 24. * rect.width(), y / 24. * rect.height());
    let mut previous = p([5., 15.]);
    let mut points = vec![previous];
    for [a, b, end] in [
        [[5., 15.], [2.3, 9.2], [2.3, 9.2]],
        [[1.4, 6.9], [4.1, 5.9], [5.1, 8.]],
        [[5.1, 8.], [7.2, 12.5], [7.2, 12.5]],
        [[7.2, 12.5], [5.4, 4.8], [5.4, 4.8]],
        [[4.9, 2.5], [7.7, 1.9], [8.3, 4.1]],
        [[8.3, 4.1], [10.1, 11.5], [10.1, 11.5]],
        [[10.1, 11.5], [9.8, 2.2], [9.8, 2.2]],
        [[9.8, 0.1], [12.7, 0.1], [12.7, 2.2]],
        [[12.7, 2.2], [13., 11.3], [13., 11.3]],
        [[13., 11.3], [14.4, 4.], [14.4, 4.]],
        [[14.7, 1.8], [17.6, 2.2], [17.3, 4.5]],
        [[17.3, 4.5], [16.3, 14.1], [16.3, 14.1]],
        [[16.1, 15.6], [17., 16.2], [18.1, 15.1]],
        [[18.1, 15.1], [20., 13.2], [20., 13.2]],
        [[21.5, 11.5], [23.8, 13.8], [22.2, 15.6]],
        [[22.2, 15.6], [17., 21.3], [17., 21.3]],
        [[13.3, 25.2], [6.9, 23.3], [5.4, 19.5]],
        [[5.4, 19.5], [5., 15.], [5., 15.]],
    ] {
        let end = p(end);
        points.extend(
            egui::epaint::CubicBezierShape::from_points_stroke(
                [previous, p(a), p(b), end],
                false,
                egui::Color32::TRANSPARENT,
                egui::Stroke::NONE,
            )
            .flatten(Some(0.1))
            .into_iter()
            .skip(1),
        );
        previous = end;
    }
    ui.painter().add(egui::Shape::closed_line(
        points,
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
                    Tool::Hand
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
                    usize::from(expected != Tool::Hand)
                );
                release(&ctx, Tool::Hand);
                assert_eq!(selected(&ctx), expected);
            }
        }
    }
    #[test]
    fn crop_and_rotation_switch_controls_without_changing_the_shared_plane() {
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
        ] {
            rotate_tool::activate(&ctx, mode);
            assert!(!take_view_change(&ctx));
        }
        crop_tool::toggle(&ctx);
        assert!(!rotate_tool::active(&ctx));
        assert!(!take_view_change(&ctx));
        hand(&ctx);
        assert!(take_view_change(&ctx));
    }
}
