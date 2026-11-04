//! Composes the Advanced range wheel and saved adjustment list with shared controls.

use super::common::LabelColumn;
use crate::{
    editor::{advanced_color as tool, direct_color},
    widgets,
};
use eframe::egui;
use foveonic::render::{AdvancedColorRange, Settings, MAX_COLOR_RANGES};

pub(super) fn show(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    let mut s = tool::state(ui.ctx());
    let mut changed = false;
    let mono = settings.is_monochrome();
    let ranges = &mut settings.photographic.color_editor.advanced;
    s.selected = s.selected.min(ranges.count.saturating_sub(1));
    let selected = ranges.count > 0;
    if selected {
        changed |= super::color_wheel::show(ui, &mut ranges.ranges[s.selected]);
    } else {
        super::color_wheel::empty(ui);
    }
    let mut placeholder = AdvancedColorRange::default();
    let r = if selected {
        &mut ranges.ranges[s.selected]
    } else {
        &mut placeholder
    };
    ui.add_enabled_ui(selected, |ui| {
        let column = LabelColumn::new(ui);
        changed |= column.adjustment(
            ui,
            ("Advanced Smoothness", "Smoothness"),
            &mut r.smoothness,
            0. ..=1.,
            0.2,
        );
        ui.add_enabled_ui(!mono, |ui| {
            changed |= column.adjustment(
                ui,
                ("Advanced Hue", "Hue"),
                &mut r.adjustment.hue,
                -1. ..=1.,
                0.,
            );
            changed |= column.saturation_adjustment(
                ui,
                ("Advanced Saturation", "Saturation"),
                &mut r.adjustment.saturation,
            );
        });
        changed |= column.adjustment(
            ui,
            ("Advanced Lightness", "Lightness"),
            &mut r.adjustment.lightness,
            -1. ..=1.,
            0.,
        );
    });
    ui.separator();
    egui::ScrollArea::vertical()
        .id_salt("Advanced color edits")
        .max_height(140.)
        .show(ui, |ui| {
            ui.set_min_height(widgets::control_height(ui));
            if ranges.count == 0 {
                ui.weak("Pick a color in the photograph to create a range.");
            }
            for i in 0..ranges.count {
                ui.horizontal(|ui| {
                    changed |= widgets::checkbox(ui, &mut ranges.ranges[i].enabled, "")
                        .on_hover_text("Enable this edit")
                        .changed();
                    let eye = preview_eye(ui, &mut settings.color_range_preview, i);
                    changed |= eye.changed();
                    if eye.clicked() {
                        s.selected = i;
                    }
                    let color: egui::Color32 =
                        egui::ecolor::Hsva::new(ranges.ranges[i].hue / 360., 0.65, 0.8, 1.).into();
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(12., 12.), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2., color);
                    let _ = range_name(ui, &mut s, i, &mut ranges.ranges[i]);
                });
            }
        });
    widgets::centered_control_row(ui, |ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let active = direct_color::advanced_active(ui.ctx());
            let button = widgets::icon_button(ui, "Pick color range", active);
            widgets::eyedropper_icon(ui, &button, active);
            if button
                .on_hover_text(
                    "Click a color and drag to adjust it. Release to return to navigation.",
                )
                .clicked()
            {
                crate::editor::dehaze::exit(ui.ctx());
                direct_color::toggle_advanced(ui.ctx());
            }
            let remove = ui
                .add_enabled_ui(ranges.count > 0, |ui| {
                    widgets::icon_button(ui, "Delete color range", false)
                })
                .inner;
            ui.painter().text(
                remove.rect.center(),
                egui::Align2::CENTER_CENTER,
                "−",
                egui::TextStyle::Body.resolve(ui.style()),
                ui.visuals().text_color(),
            );
            if remove.clicked() {
                direct_color::exit(ui.ctx());
                let removed = s.selected;
                ranges.remove(removed);
                settings.color_range_preview = settings.color_range_preview.and_then(|i| {
                    if i == removed {
                        None
                    } else {
                        Some(i - usize::from(i > removed))
                    }
                });
                s.rename = None;
                s.selected = s.selected.min(ranges.count.saturating_sub(1));
                changed = true;
            }
            let add = ui
                .add_enabled_ui(ranges.count < MAX_COLOR_RANGES, |ui| {
                    widgets::icon_button(ui, "Add color range", false)
                })
                .inner;
            ui.painter().text(
                add.rect.center(),
                egui::Align2::CENTER_CENTER,
                "+",
                egui::TextStyle::Body.resolve(ui.style()),
                ui.visuals().text_color(),
            );
            if add.clicked() {
                direct_color::exit(ui.ctx());
                s.rename = None;
                s.selected = ranges.push(AdvancedColorRange::default()).unwrap();
                changed = true;
            }
        });
    });
    let preview = settings.color_range_preview.filter(|i| *i < ranges.count);
    changed |= preview != settings.color_range_preview;
    settings.color_range_preview = preview;
    let notice = direct_color::notice(ui.ctx());
    if !notice.is_empty() {
        ui.weak(notice);
    }
    tool::store(ui.ctx(), s);
    changed
}

fn range_name(
    ui: &mut egui::Ui,
    state: &mut tool::State,
    index: usize,
    range: &mut AdvancedColorRange,
) -> egui::Response {
    if let Some(rename) = state.rename.as_mut().filter(|rename| rename.index == index) {
        let response = ui.add_sized(
            [ui.available_width(), widgets::control_height(ui)],
            egui::TextEdit::singleline(&mut rename.text)
                .id_salt(("range name", index))
                .char_limit(foveonic::render::MAX_COLOR_RANGE_NAME_CHARS),
        );
        let first_frame = std::mem::take(&mut rename.focus);
        if first_frame {
            response.request_focus();
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            state.rename = None;
        } else if !first_frame
            && (response.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)))
        {
            let rename = state.rename.take().unwrap();
            // This is metadata only. The existing sidecar change detector saves
            // it, while no image-edit signal schedules another render.
            range.name = rename.text.trim().try_into().expect("bounded range name");
        }
        return response;
    }
    let label = if range.name.is_empty() {
        format!("Range {}", index + 1)
    } else {
        range.name.to_string()
    };
    let response = ui
        .allocate_ui_with_layout(
            egui::vec2(ui.available_width(), widgets::control_height(ui)),
            egui::Layout::left_to_right(egui::Align::Center).with_main_justify(true),
            |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                ui.add(egui::SelectableLabel::new(state.selected == index, &label))
            },
        )
        .inner
        .on_hover_text("Double-click to rename");
    if response.clicked() {
        direct_color::exit(ui.ctx());
        state.selected = index;
        state.rename = None;
    }
    if response.double_clicked() {
        state.rename = Some(tool::Rename {
            index,
            text: label,
            focus: true,
        });
    }
    response
}

fn preview_eye(ui: &mut egui::Ui, preview: &mut Option<usize>, index: usize) -> egui::Response {
    let active = *preview == Some(index);
    let mut response =
        widgets::eye_button(ui, "View color range", active).on_hover_text(if active {
            "Stop viewing this range"
        } else {
            "View this color range"
        });
    if response.clicked() {
        *preview = (!active).then_some(index);
        response.mark_changed();
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            pressed,
            button: egui::PointerButton::Primary,
            modifiers: Default::default(),
        }
    }
    fn frame(
        ctx: &egui::Context,
        state: &mut tool::State,
        range: &mut AdvancedColorRange,
        time: f64,
        events: Vec<egui::Event>,
    ) -> (egui::Response, egui::Response) {
        let mut result = None;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(420., 300.),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let name = range_name(ui, state, 0, range);
                    let other = ui.button("Other control");
                    result = Some((name, other));
                });
            },
        );
        result.unwrap()
    }
    #[test]
    fn double_click_renames_and_enter_or_click_away_commits() {
        for click_away in [false, true] {
            let ctx = egui::Context::default();
            let mut state = tool::State::default();
            let mut range = AdvancedColorRange::default();
            let position = frame(&ctx, &mut state, &mut range, 0., vec![])
                .0
                .rect
                .center();
            for (time, pressed) in [(0.1, true), (0.15, false), (0.2, true), (0.25, false)] {
                frame(
                    &ctx,
                    &mut state,
                    &mut range,
                    time,
                    vec![
                        egui::Event::PointerMoved(position),
                        event(position, pressed),
                    ],
                );
            }
            assert!(state.rename.is_some());
            frame(&ctx, &mut state, &mut range, 0.3, vec![]);
            state.rename.as_mut().unwrap().text = "Sky".into();
            let (_, other) = frame(&ctx, &mut state, &mut range, 0.4, vec![]);
            let events = if click_away {
                vec![
                    egui::Event::PointerMoved(other.rect.center()),
                    event(other.rect.center(), true),
                ]
            } else {
                vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }]
            };
            frame(&ctx, &mut state, &mut range, 0.5, events);
            assert_eq!(range.name.as_str(), "Sky");
            assert!(state.rename.is_none());
            let mut a = Settings::default();
            a.photographic.color_editor.advanced.push(range);
            let mut b = a.clone();
            b.photographic.color_editor.advanced.ranges[0].name = "Blue sky".try_into().unwrap();
            assert!(crate::smart::same_edits(&a, &b));
        }
    }
    #[test]
    fn preview_eyes_are_exclusive_and_toggle_back_to_none() {
        let ctx = egui::Context::default();
        let mut preview = None;
        let mut draw = |time, events| {
            let mut rects = Vec::new();
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(420., 300.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        for index in 0..2 {
                            rects.push(preview_eye(ui, &mut preview, index).rect);
                        }
                    });
                },
            );
            (rects, preview)
        };
        let (rects, value) = draw(0., vec![]);
        assert_eq!(value, None);
        for (step, (index, expected)) in [(0, Some(0)), (1, Some(1)), (1, None)]
            .into_iter()
            .enumerate()
        {
            let p = rects[index].center();
            let time = 1. + step as f64;
            draw(time, vec![egui::Event::PointerMoved(p), event(p, true)]);
            assert_eq!(draw(time + 0.1, vec![event(p, false)]).1, expected);
        }
    }
}
