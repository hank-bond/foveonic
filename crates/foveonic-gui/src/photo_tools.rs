//! Composes the photograph toolbar and shares Hand, Escape and tool-availability routing.

use crate::{
    crop_tool, editor, toolbar,
    widgets::{self, ControlSize},
};
use eframe::egui;
use foveonic::crop::Crop;

#[derive(Clone, Copy)]
enum Tool {
    Hand,
    Crop,
}
impl Tool {
    const ALL: [Self; 2] = [Self::Hand, Self::Crop];
    fn width(self) -> f32 {
        match self {
            Self::Hand => ControlSize::Large.side(),
            Self::Crop => ControlSize::Large.split_width(),
        }
    }
}

pub fn active(ctx: &egui::Context) -> bool {
    crop_tool::active(ctx)
        || editor::direct_color::active(ctx)
        || editor::dehaze::active(ctx)
        || editor::white_balance::active(ctx)
}
pub fn menu_open(ctx: &egui::Context) -> bool {
    crop_tool::menu_open(ctx) || editor::direct_color::menu_open(ctx)
}
pub fn hand(ctx: &egui::Context) {
    crop_tool::exit(ctx);
    editor::direct_color::exit(ctx);
    editor::dehaze::exit(ctx);
    editor::white_balance::exit(ctx);
}
pub fn escape(ctx: &egui::Context) {
    crop_tool::escape(ctx);
    editor::direct_color::exit(ctx);
    editor::dehaze::exit(ctx);
    editor::white_balance::exit(ctx);
}

pub fn show(
    ui: &mut egui::Ui,
    enabled: bool,
    crop: &mut Crop,
    extent: Option<[u32; 2]>,
    custom: &mut Vec<crop_tool::NamedRatio>,
) -> bool {
    let before = *crop;
    toolbar::tool_group(ui, &Tool::ALL.map(Tool::width), |ui| {
        ui.add_enabled_ui(enabled, |ui| {
            for tool in Tool::ALL {
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
                }
            }
        });
    });
    crop_tool::dialog(ui.ctx(), crop, extent, custom);
    before != *crop
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
