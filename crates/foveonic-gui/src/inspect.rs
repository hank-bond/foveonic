//! Groups viewport preview quality, pipeline views, and display-only Input/Output and Difference comparisons.

use crate::{help, sections, widgets};
use eframe::egui;
use foveonic::{
    inspection::{self, Channels},
    render::Settings,
    source::{Metadata, ProcessingFamily},
};

// Amplify small diagnostic differences without changing photographic output.
const DEFAULT_DIFFERENCE_GAIN: f32 = 32.;

pub fn components(channels: Channels) -> &'static [(i32, &'static str)] {
    match channels {
        Channels::Layers => &[(0, "Layer 1 (R)"), (1, "Layer 2 (G)"), (2, "Layer 3 (B)")],
        Channels::Rgb => &[(-1, "RGB"), (0, "Red"), (1, "Green"), (2, "Blue")],
        Channels::Scalar => &[(-1, "Scalar")],
    }
}
fn choose(output: &mut String, channel: &mut i32, name: &str, component: i32) {
    *output = name.into();
    *channel = component;
}

fn inspection_help(response: egui::Response, name: &str) -> egui::Response {
    // Recipe catalogues can grow independently of explanatory UI copy.
    // Missing help must never prevent opening or selecting a supported view.
    match help::inspection(name) {
        Some(text) => response.on_hover_text(text),
        None => response,
    }
}

/// A full-width disclosure row. Opening it never selects a processing output.
fn group_row(ui: &mut egui::Ui, name: &str, selected: bool) -> bool {
    let id = ui.make_persistent_id(("inspect group", name));
    let mut open = ui.data_mut(|d| d.get_persisted::<bool>(id).unwrap_or(false));
    let padding = crate::widgets::button_padding(ui);
    let galley = egui::WidgetText::from(name).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        ui.available_width(),
        egui::TextStyle::Button,
    );
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(
            ui.available_width(),
            ui.spacing()
                .interact_size
                .y
                .max(galley.size().y + 2. * padding.y),
        ),
        egui::Sense::click(),
    );
    if response.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_persisted(id, open));
    }
    let color = crate::widgets::menu_cell(ui, &response, selected);
    ui.painter().galley(
        egui::pos2(
            rect.left() + padding.x,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        color,
    );
    sections::disclosure_icon(
        ui,
        rect.right_center() - egui::vec2(padding.x + 4., 0.),
        !open,
        color,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::CollapsingHeader, ui.is_enabled(), name)
    });
    inspection_help(response, name);
    open
}

/// Keep a restored recipe's selection valid before its first worker request.
pub fn normalize(
    settings: &Settings,
    output: &mut String,
    channel: &mut i32,
    metadata: Option<&Metadata>,
    family: Option<ProcessingFamily>,
) {
    let outputs = inspection::image_outputs_with_family(settings, metadata, family);
    if !outputs.iter().any(|o| o.name == output) && outputs.iter().any(|o| o.name == "final") {
        choose(output, channel, "final", -1);
    }
    let Some(current) = outputs.iter().find(|o| o.name == output) else {
        return;
    };
    let choices = components(current.channels);
    if !choices.iter().any(|(c, _)| c == channel) {
        *channel = choices[0].0;
    }
}

fn menu_width(ui: &egui::Ui, outputs: &[inspection::ImageOutput], selected: &str) -> f32 {
    let stages = crate::widgets::dropdown_width(
        ui,
        std::iter::once(selected).chain(outputs.iter().map(|image| image.name)),
    );
    let components = crate::widgets::dropdown_width(
        ui,
        outputs
            .iter()
            .filter(|image| image.channels != Channels::Scalar)
            .flat_map(|image| components(image.channels).iter().map(|(_, label)| *label)),
    ) + ui.spacing().indent;
    // The popup frame and scrollbar consume width inside the header's span.
    // Keep the disclosure column clear even when a floating scrollbar is visible.
    let spacing = ui.spacing();
    let gutter = spacing.scroll.allocated_width().max(
        spacing.scroll.bar_width
            + spacing.scroll.bar_inner_margin
            + spacing.scroll.bar_outer_margin,
    );
    stages.max(components)
        + f32::from(spacing.menu_margin.left)
        + f32::from(spacing.menu_margin.right)
        + gutter
}

/// Returns true only when processing must produce a different buffer.
pub fn show(
    ui: &mut egui::Ui,
    settings: &Settings,
    output: &mut String,
    channel: &mut i32,
    metadata: Option<&Metadata>,
    family: Option<ProcessingFamily>,
) -> bool {
    let before = output.clone();
    normalize(settings, output, channel, metadata, family);
    let outputs = inspection::image_outputs_with_family(settings, metadata, family);
    let current = outputs.iter().find(|o| o.name == output);
    let selected = if let Some(current) = current {
        let choices = components(current.channels);
        let label = choices.iter().find(|(c, _)| c == channel).unwrap().1;
        if current.channels == Channels::Scalar {
            output.clone()
        } else {
            format!("{output} · {label}")
        }
    } else {
        format!("{output} (unavailable)")
    };
    let current_help = match current {
        Some(image) => help::inspection(image.name)
            .unwrap_or("Shows a processing stage used to make the photo."),
        None => "This processing output is not available for this camera. Choose a supported inspection view.",
    };
    let width = menu_width(ui, &outputs, &selected);
    let combo = egui::ComboBox::from_id_salt("artifact")
        .selected_text(selected)
        .width(width)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    crate::widgets::dropdown(ui, combo, |ui| {
            ui.spacing_mut().item_spacing.y = 0.;
            for image in outputs {
                if image.channels == Channels::Scalar {
                    if inspection_help(crate::widgets::menu_row(ui, image.name, image.name == output), image.name)
                        .clicked()
                    {
                        choose(output, channel, image.name, -1);
                        ui.memory_mut(|memory| memory.close_popup());
                    }
                } else {
                    if group_row(ui, image.name, image.name == output) {
                        ui.indent(image.name, |ui| {
                        for &(component, label) in components(image.channels) {
                            if crate::widgets::menu_row(
                                    ui, label,
                                    image.name == output && component == *channel,
                                )
                                .on_hover_text(if image.channels == Channels::Layers {
                                    "Shows one of the camera's three color-sensitive sensor layers in gray. Each responds to overlapping colors; it is not a finished red, green, or blue channel."
                                } else if component == -1 {
                                    "Shows the normal color view of this processing stage."
                                } else {
                                    "Shows this color channel in gray: brighter means more of that channel. This changes the display only; it does not remove colors from the photo or change the scopes."
                                })
                                .clicked()
                            {
                                choose(output, channel, image.name, component);
                                ui.memory_mut(|memory| memory.close_popup());
                            }
                        }
                        });
                    }
                }
            }
        })
        .response
        .on_hover_text(format!(
            "{}\n\nInspect shows steps used to make the photo. Earlier steps do not include later edits. Changing this view does not change your export: export always uses the finished photo.",
            current_help,
        ));
    before != *output
}

/// Session-only presentation controls, independent of photographic settings.
pub struct Comparison {
    pub input: bool,
    pub difference: bool,
    pub gain: f32,
}
impl Default for Comparison {
    fn default() -> Self {
        Self {
            input: false,
            difference: false,
            gain: DEFAULT_DIFFERENCE_GAIN,
        }
    }
}
impl Comparison {
    pub fn requested(&self) -> bool {
        self.input || self.difference
    }

    pub fn mapping(&self, channel: i32, has_pair: bool) -> crate::viewport::Mapping {
        let mut mapping = crate::viewport::Mapping::normal(channel);
        if has_pair {
            mapping.input = self.input && !self.difference;
            if self.difference && channel >= 0 {
                mapping.difference_gain = self.gain;
            }
        }
        mapping
    }

    /// Returns true only when the user newly requests a comparison pair.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        settings: &Settings,
        output: &str,
        channel: i32,
        metadata: Option<&Metadata>,
    ) -> bool {
        let before = self.requested();
        let input = inspection::comparison_input(settings, output, metadata);
        let available = input.is_some() && channel >= 0;
        if input.is_none() {
            self.input = false;
        }
        if !available {
            self.difference = false;
        }
        let side_help = if input.is_none() {
            "This view has no matching input comparison."
        } else if self.difference {
            "Turn Difference off to switch between Input and Output."
        } else {
            "Input shows the image before this step. Output shows it after. Scopes always measure the output. This does not change edits or export."
        };
        ui.add_enabled_ui(input.is_some() && !self.difference, |ui| {
            for (side, label) in [(true, "Input"), (false, "Output")] {
                crate::widgets::selectable_value(ui, &mut self.input, side, label)
                    .on_hover_text(side_help)
                    .on_disabled_hover_text(side_help);
            }
        });
        let explanation = if input.is_some() && channel < 0 {
            "Choose an individual RGB channel for Difference. Input and Output can still show combined RGB."
        } else if input.is_none() {
            "This view has no matching input comparison. Original readings and diagnostic maps stand alone. Color and tone conversion change the meaning or units of the values."
        } else {
            "Shows output minus input. Middle gray means no change. Lighter means increased and darker means decreased. Scopes still measure the output. This does not change edits or export."
        };
        ui.add_enabled_ui(available, |ui| {
            crate::widgets::checkbox(ui, &mut self.difference, "Δ Difference")
        })
        .inner
        .on_disabled_hover_text(explanation)
        .on_hover_text(explanation);
        if self.difference {
            ui.label("Amplify");
            ui.add(egui::DragValue::new(&mut self.gain).range(1. ..=1024.).speed(1.).suffix("×"))
                .on_hover_text("Magnifies small differences for display only. Amplification can clip this diagnostic view without clipping the photograph. At 1×, a full nominal signal-range change reaches black or white.");
        }
        !before && self.requested()
    }
}

/// Capture properties resolve architecture support before pixel decoding/rendering.
/// Unknown metadata leaves the application preference visible.
pub fn preview_support(
    settings: &Settings,
    metadata: Option<&Metadata>,
    capture: Option<&std::collections::BTreeMap<String, String>>,
) -> Option<bool> {
    if let Some(metadata) = metadata {
        return Some(settings.fast_preview_available(metadata));
    }
    let model = capture?.get("CAMMODEL")?.trim().to_ascii_lowercase();
    Some(
        !settings.is_monochrome()
            && matches!(
                model.strip_prefix("sigma ").unwrap_or(&model),
                "dp1 merrill"
                    | "dp2 merrill"
                    | "dp3 merrill"
                    | "sd1 merrill"
                    | "dp0 quattro"
                    | "dp1 quattro"
                    | "dp2 quattro"
                    | "dp3 quattro"
                    | "sd quattro"
                    | "sd quattro h"
            ),
    )
}

/// Preview quality is a workspace choice, independent of photo edits and export.
pub fn preview_quality(
    ui: &mut egui::Ui,
    enabled: bool,
    available: Option<bool>,
    selected: &mut crate::preferences::PreviewMode,
) -> bool {
    use crate::preferences::PreviewMode;
    let mut changed = false;
    let effective = if available != Some(false) {
        *selected
    } else {
        PreviewMode::Full
    };
    let explanation = if available != Some(false) {
        "Fast gives a smaller color preview for quicker browsing and edits. Smart shows Fast first and switches to Full above 100% for detail and panning. Exports use Full."
    } else {
        "Fast and Smart previews are available for Merrill and Quattro color photographs."
    };
    widgets::button_group(ui, |ui| {
        widgets::centered_control_row(ui, |ui| {
            ui.label("Preview");
            for (quality, label) in [
                (PreviewMode::Full, "Full"),
                (PreviewMode::Fast, "Fast"),
                (PreviewMode::Smart, "Smart"),
            ] {
                let response = ui
                    .add_enabled_ui(
                        enabled && (quality == PreviewMode::Full || available != Some(false)),
                        |ui| widgets::button(ui, label, effective == quality, egui::Vec2::ZERO),
                    )
                    .inner
                    .on_hover_text(explanation);
                if response.clicked() && *selected != quality {
                    *selected = quality;
                    changed = true;
                }
            }
        });
    });
    changed
}

#[cfg(test)]
#[path = "inspect_test.rs"]
mod tests;
