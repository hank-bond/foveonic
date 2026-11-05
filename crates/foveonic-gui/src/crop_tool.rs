//! Owns crop gestures and aspect options within the shared photograph-tool lifecycle.

mod gesture;
mod ratios;
pub use ratios::NamedRatio;

use crate::{
    photo_tools::{self, Tool},
    widgets::{self, ControlSize},
};
use eframe::egui;
use foveonic::crop::Crop;
use std::path::{Path, PathBuf};

#[derive(Clone, Default)]
struct State {
    source: Option<PathBuf>,
    drag: Option<gesture::Drag>,
    dialog: Option<ratios::Dialog>,
    menu_open: bool,
}
fn id() -> egui::Id {
    egui::Id::new("crop tool")
}
fn state(ctx: &egui::Context) -> State {
    ctx.data_mut(|d| d.get_temp(id())).unwrap_or_default()
}
fn store(ctx: &egui::Context, state: State) {
    ctx.data_mut(|d| d.insert_temp(id(), state));
}

pub fn active(ctx: &egui::Context) -> bool {
    photo_tools::selected(ctx) == Tool::Crop
}
pub fn menu_open(ctx: &egui::Context) -> bool {
    let s = state(ctx);
    s.menu_open || s.dialog.is_some()
}
pub fn source(ctx: &egui::Context, path: Option<&Path>) {
    let mut s = state(ctx);
    if s.source.as_deref() != path {
        s.source = path.map(Path::to_path_buf);
        s.drag = None;
        s.dialog = None;
        store(ctx, s);
    }
}
pub(crate) fn cancel(ctx: &egui::Context) {
    let mut s = state(ctx);
    s.drag = None;
    s.dialog = None;
    s.menu_open = false;
    store(ctx, s);
}
pub fn exit(ctx: &egui::Context) {
    cancel(ctx);
    photo_tools::release(ctx, Tool::Crop);
}
pub fn escape(ctx: &egui::Context) {
    let mut s = state(ctx);
    if s.drag.take().is_none() {
        exit(ctx);
    } else {
        store(ctx, s);
    }
}
pub fn toggle(ctx: &egui::Context) {
    photo_tools::select(ctx, if active(ctx) { Tool::Hand } else { Tool::Crop });
}
pub fn displayed(ctx: &egui::Context, crop: Crop) -> (Crop, bool) {
    let s = state(ctx);
    (s.drag.as_ref().map_or(crop, |d| d.draft), s.drag.is_some())
}

pub fn button(ui: &mut egui::Ui, crop: &mut Crop, extent: Option<[u32; 2]>, custom: &[NamedRatio]) {
    let mut s = state(ui.ctx());
    ui.add_enabled_ui(extent.is_some(), |ui| {
        let mut action = None;
        let (button, menu) =
            widgets::split_icon_button(ui, "Crop", active(ui.ctx()), ControlSize::Large, |ui| {
                action = ratios::menu(ui, *crop, custom, &mut s.dialog);
            });
        crop_icon(ui, &button, active(ui.ctx()));
        if button
            .on_hover_text("Crop (C) · Drag handles to resize, or drag inside to move")
            .clicked()
        {
            toggle(ui.ctx());
            s = state(ui.ctx());
        }
        s.menu_open = menu.inner.is_some();
        menu.response.on_hover_text("Aspect ratios and crop reset");
        if let (Some(action), Some(extent)) = (action, extent) {
            match action {
                ratios::Action::Reset => *crop = Crop::default(),
                ratios::Action::Flip => {
                    crop.flipped = !crop.flipped;
                    *crop = gesture::fit(*crop, extent);
                }
                ratios::Action::Aspect(aspect) => {
                    crop.aspect = aspect;
                    *crop = gesture::fit(*crop, extent);
                }
            }
            photo_tools::select(ui.ctx(), Tool::Crop);
            s = state(ui.ctx());
        }
    });
    store(ui.ctx(), s);
}

pub fn dialog(
    ctx: &egui::Context,
    crop: &mut Crop,
    extent: Option<[u32; 2]>,
    custom: &mut Vec<NamedRatio>,
) {
    let mut s = state(ctx);
    if let Some(aspect) = ratios::show_dialog(ctx, &mut s.dialog, custom) {
        if let Some(extent) = extent {
            crop.aspect = aspect;
            *crop = gesture::fit(*crop, extent);
            photo_tools::select(ctx, Tool::Crop);
            s = state(ctx);
        }
    }
    store(ctx, s);
}

pub fn photo(
    ui: &mut egui::Ui,
    response: &egui::Response,
    image: egui::Rect,
    extent: [u32; 2],
    crop: &mut Crop,
) -> bool {
    let mut s = state(ui.ctx());
    if !active(ui.ctx()) || extent.contains(&0) {
        return false;
    }
    if s.drag.as_ref().is_some_and(|drag| drag.original != *crop) {
        s.drag = None;
    }
    let position = ui.input(|i| i.pointer.interact_pos());
    let panning = ui.input(|i| i.key_down(egui::Key::Space));
    if panning {
        s.drag = None;
    }
    if !panning {
        if let Some(position) = position {
            let native = egui::Pos2::ZERO
                + (position - image.min) / image.size()
                    * egui::vec2(extent[0] as f32, extent[1] as f32);
            let rect = crate::framing::crop_rect(*crop, extent, image);
            let handle = gesture::hit(rect, position);
            if response.hovered()
                && image.contains(position)
                && ui.input(|i| i.pointer.primary_pressed())
            {
                s.drag = Some(gesture::Drag::new(*crop, extent, handle, native));
            }
            if let Some(drag) = &mut s.drag {
                if drag.moved || ui.input(|i| i.pointer.is_decidedly_dragging()) {
                    drag.update(native);
                }
                ui.ctx().set_cursor_icon(gesture::cursor(drag.handle, true));
            } else if response.hovered() && image.contains(position) {
                ui.ctx().set_cursor_icon(gesture::cursor(handle, false));
            }
        }
    }
    let mut changed = false;
    if !panning && response.double_clicked() && position.is_some_and(|p| image.contains(p)) {
        s.drag = None;
        changed = *crop != Crop::default();
        *crop = Crop::default();
    } else if ui.input(|i| i.pointer.primary_released()) {
        if let Some(drag) = s.drag.take().filter(|d| d.moved) {
            *crop = drag.draft;
            changed = drag.original != drag.draft;
        }
    }
    store(ui.ctx(), s);
    changed
}

fn crop_icon(ui: &egui::Ui, response: &egui::Response, on: bool) {
    let rect = widgets::icon_rect(ui, response);
    let point =
        |x: f32, y: f32| rect.min + egui::vec2(x / 16. * rect.width(), y / 16. * rect.height());
    let stroke = egui::Stroke::new(1.5_f32, widgets::visuals(ui, response, on).text_color());
    ui.painter().add(egui::Shape::line(
        vec![point(3., 0.), point(3., 13.), point(16., 13.)],
        stroke,
    ));
    ui.painter().add(egui::Shape::line(
        vec![point(0., 3.), point(13., 3.), point(13., 16.)],
        stroke,
    ));
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tool_changes_are_transient_and_escape_cancels_a_draft_before_leaving() {
        let ctx = egui::Context::default();
        toggle(&ctx);
        assert!(active(&ctx));
        assert!(crate::photo_tools::take_view_change(&ctx));
        let mut s = state(&ctx);
        s.drag = Some(gesture::Drag::new(
            Crop::default(),
            [600, 400],
            gesture::Handle::Move,
            egui::Pos2::ZERO,
        ));
        store(&ctx, s);
        escape(&ctx);
        assert!(active(&ctx));
        assert!(!crate::photo_tools::take_view_change(&ctx));
        escape(&ctx);
        assert!(!active(&ctx));
        assert!(crate::photo_tools::take_view_change(&ctx));
    }
}
