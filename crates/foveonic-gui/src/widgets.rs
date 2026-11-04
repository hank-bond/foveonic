//! Shares numeric display policy, control sizes and workspace widgets.

use eframe::egui;

/// Reserve one quarter of a form row for labels. A label may take more space
/// only when it cannot fit, temporarily reducing that row's control span.
pub fn form_label_width(ui: &egui::Ui, labels: &[&str], toggle: Option<&str>) -> f32 {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let measure = |text: &str| {
        ui.painter()
            .layout_no_wrap(text.into(), font.clone(), egui::Color32::PLACEHOLDER)
            .size()
            .x
    };
    let needed = labels
        .iter()
        .map(|text| measure(text))
        .chain(
            toggle
                .map(|text| measure(text) + ControlSize::Small.side() + ui.spacing().icon_spacing),
        )
        .fold(0., f32::max);
    (ui.available_width() / 4.).max(needed).ceil()
}

/// Editing subsection titles share a stronger hierarchy than parameter labels.
pub fn section_title(ui: &egui::Ui, title: &str) -> egui::RichText {
    let size = egui::TextStyle::Body.resolve(ui.style()).size * 1.08;
    let color = if ui.visuals().dark_mode {
        egui::Color32::from_gray(225)
    } else {
        ui.visuals().strong_text_color()
    };
    egui::RichText::new(title).size(size).color(color)
}

/// Headers use the same compact gap above and below their row.
pub fn section_spacing(ui: &egui::Ui) -> f32 {
    ui.spacing().item_spacing.y * 0.8
}

const VALUE_SIGNIFICANT_DIGITS: usize = 3;
const VALUE_DECIMAL_PLACES: usize = 2;

/// All numeric widgets and their layout measurements use this display policy.
pub(crate) fn format_numeric_value(value: f64, integral: bool) -> String {
    if integral && value.abs() < 1000. {
        format!("{value:.0}")
    } else {
        format_parameter_value(value)
    }
}

pub(crate) fn drag_value<T: egui::emath::Numeric>(value: &mut T) -> egui::DragValue<'_> {
    egui::DragValue::new(value)
        .custom_formatter(|value, _| format_numeric_value(value, T::INTEGRAL))
}

pub(crate) fn slider<T: egui::emath::Numeric>(
    value: &mut T,
    range: std::ops::RangeInclusive<T>,
) -> egui::Slider<'_> {
    egui::Slider::new(value, range)
        .custom_formatter(|value, _| format_numeric_value(value, T::INTEGRAL))
}

/// Compact readouts use at most three significant digits and two decimal places.
/// Scientific notation keeps small nonzero settings distinguishable from zero.
pub(crate) fn format_parameter_value(value: f64) -> String {
    if value == 0. {
        return "0.00".into();
    }
    if !value.is_finite() {
        return value.to_string();
    }
    // Read the exponent after rounding so carries such as 9.999 -> 10.0
    // choose the correct number of decimal places too.
    let scientific = format!(
        "{value:.precision$e}",
        precision = VALUE_SIGNIFICANT_DIGITS - 1
    );
    let (mantissa, exponent) = scientific.split_once('e').expect("formatted exponent");
    let exponent: i32 = exponent.parse().expect("numeric exponent");
    if !(-3..VALUE_SIGNIFICANT_DIGITS as i32).contains(&exponent) {
        return format!(
            "{}e{exponent}",
            mantissa.trim_end_matches('0').trim_end_matches('.')
        );
    }
    let decimals =
        ((VALUE_SIGNIFICANT_DIGITS as i32 - 1 - exponent) as usize).min(VALUE_DECIMAL_PLACES);
    let text = format!("{value:.decimals$}");
    if text.parse::<f64>() == Ok(0.) {
        return format!(
            "{}e{exponent}",
            mantissa.trim_end_matches('0').trim_end_matches('.')
        );
    }
    text
}

const RADIUS: u8 = 4;
pub const BUTTON_GROUP_MARGIN: i8 = 4;
pub const MEDIUM_CONTROL_GAP: f32 = 6.;
pub const MENU_BACKGROUND: egui::Color32 = egui::Color32::from_gray(29);
const PARAMETER_SUBGROUP_BACKGROUND: egui::Color32 = egui::Color32::from_gray(36);
/// Keep slider tracks distinct from both linked groups and the main panel.
pub const SLIDER_TRACK_FILL: egui::Color32 = egui::Color32::from_gray(60);

/// Give related controls a shared padded background without changing their spacing.
pub fn button_group<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    linked_group(
        ui,
        egui::Margin::same(BUTTON_GROUP_MARGIN),
        egui::Margin::ZERO,
        MENU_BACKGROUND,
        contents,
    )
}

/// Extend the background around parameter rows without moving their controls.
pub fn parameter_group<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    linked_group(
        ui,
        egui::Margin::same(BUTTON_GROUP_MARGIN),
        egui::Margin::symmetric(-BUTTON_GROUP_MARGIN, 0),
        MENU_BACKGROUND,
        contents,
    )
}

/// Distinguish a nested pair of parameter rows from its enclosing group.
pub fn parameter_subgroup<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    linked_group(
        ui,
        egui::Margin::same(BUTTON_GROUP_MARGIN),
        egui::Margin::symmetric(-BUTTON_GROUP_MARGIN, 0),
        PARAMETER_SUBGROUP_BACKGROUND,
        contents,
    )
}

fn linked_group<R>(
    ui: &mut egui::Ui,
    padding: egui::Margin,
    outer_margin: egui::Margin,
    background: egui::Color32,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    egui::Frame::new()
        .inner_margin(padding)
        .outer_margin(outer_margin)
        .corner_radius(6)
        .fill(background)
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), contents)
                .inner
        })
}

/// Keep the disclosure line on the parent label edge and its body one em inward.
pub fn disclosure<R>(
    ui: &mut egui::Ui,
    header: egui::CollapsingHeader,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::CollapsingResponse<R> {
    header.show_unindented(ui, |ui| {
        let start = ui.next_widget_position();
        let body = ui
            .scope(|ui| {
                ui.visuals_mut().indent_has_left_vline = false;
                ui.indent("disclosure body", contents)
            })
            .inner;
        ui.painter().line_segment(
            [start, egui::pos2(start.x, body.response.rect.bottom())],
            ui.visuals().widgets.noninteractive.bg_stroke,
        );
        body.inner
    })
}

pub fn install_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = egui::Color32::from_gray(22);
    visuals.window_fill = egui::Color32::from_gray(22);
    visuals.extreme_bg_color = egui::Color32::from_gray(12);
    visuals.selection.bg_fill = egui::Color32::from_rgb(35, 65, 100);
    visuals.selection.stroke = egui::Stroke::new(1_f32, egui::Color32::from_rgb(100, 160, 245));
    for (widget, fill, border) in [
        (&mut visuals.widgets.inactive, 29, 70),
        (&mut visuals.widgets.hovered, 34, 130),
        (&mut visuals.widgets.active, 24, 170),
        (&mut visuals.widgets.open, 34, 130),
        (&mut visuals.widgets.noninteractive, 22, 50),
    ] {
        widget.bg_fill = egui::Color32::from_gray(fill);
        widget.weak_bg_fill = widget.bg_fill;
        widget.bg_stroke = egui::Stroke::new(1_f32, egui::Color32::from_gray(border));
        widget.corner_radius = egui::CornerRadius::same(RADIUS);
        widget.expansion = 0.;
    }
    visuals.collapsing_header_frame = true;
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(7., 6.);
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(13.));
    // Leave one body em between a disclosure line and its child controls.
    style.spacing.indent = egui::TextStyle::Body.resolve(&style).size;
    style.visuals.indent_has_left_vline = true;
    ctx.set_style(style);
    // A system theme change must not restore egui's full-precision hints or
    // default formatting on widgets outside the photographic control rows.
    ctx.all_styles_mut(|style| {
        style.explanation_tooltips = false;
        style.number_formatter = egui::style::NumberFormatter::new(|value, decimals| {
            format_numeric_value(value, *decimals.end() == 0)
        });
    });
}

pub fn visuals(ui: &egui::Ui, response: &egui::Response, on: bool) -> egui::style::WidgetVisuals {
    let widgets = &ui.visuals().widgets;
    let pressed = response.is_pointer_button_down_on() || response.clicked();
    let hovered = response.hovered() || response.has_focus() || response.highlighted();
    let mut visuals = if !response.enabled() {
        widgets.noninteractive
    } else if pressed {
        widgets.active
    } else if hovered {
        widgets.hovered
    } else {
        widgets.inactive
    };
    visuals.corner_radius = egui::CornerRadius::same(RADIUS);
    if on {
        visuals.bg_fill = if pressed {
            egui::Color32::from_rgb(24, 45, 72)
        } else {
            ui.visuals().selection.bg_fill
        };
        visuals.weak_bg_fill = visuals.bg_fill;
        visuals.bg_stroke = egui::Stroke::new(
            1_f32,
            if hovered && response.enabled() {
                egui::Color32::from_rgb(140, 190, 255)
            } else {
                ui.visuals().selection.stroke.color
            },
        );
    }
    visuals
}

fn frame(ui: &egui::Ui, response: &egui::Response, on: bool, rect: egui::Rect) -> egui::Shape {
    let visuals = visuals(ui, response, on);
    egui::Shape::Rect(egui::epaint::RectShape::new(
        rect,
        visuals.corner_radius,
        visuals.weak_bg_fill,
        visuals.bg_stroke,
        egui::StrokeKind::Inside,
    ))
}

pub fn paint(ui: &egui::Ui, response: &egui::Response, on: bool) {
    ui.painter().add(frame(ui, response, on, response.rect));
}

pub fn button_padding(ui: &egui::Ui) -> egui::Vec2 {
    let style = ui.style();
    let em = style.override_font_id.as_ref().map_or_else(
        || {
            style
                .override_text_style
                .as_ref()
                .unwrap_or(&egui::TextStyle::Button)
                .resolve(style)
                .size
        },
        |font| font.size,
    );
    egui::vec2(0.5 * em, 0.25 * em)
}

/// Prefer the full label when its padded button fits the remaining row width.
pub fn fitting_button_label<'a>(
    ui: &egui::Ui,
    full: &'a str,
    compact: &'a str,
    available_width: f32,
) -> &'a str {
    if menu_label(ui, full).size().x + 2. * button_padding(ui).x <= available_width {
        full
    } else {
        compact
    }
}

/// Balance the leading gap with the layout's automatic trailing gap, then
/// reserve the full control height before laying out the first label.
pub fn control_row<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.add_space(ui.spacing().item_spacing.y);
    centered_control_row(ui, contents)
}

/// Center labels with padded controls, leaving outer spacing to the container.
pub fn centered_control_row<R>(
    ui: &mut egui::Ui,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let height = control_height(ui);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), height),
        egui::Layout::left_to_right(egui::Align::Center),
        contents,
    )
}

/// Reserve padded control height before placing centered labels or groups.
pub fn control_height(ui: &egui::Ui) -> f32 {
    let text_height = |style| {
        egui::WidgetText::from("M")
            .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, style)
            .size()
            .y
    };
    (text_height(egui::TextStyle::Button) + 2. * button_padding(ui).y)
        .max(text_height(egui::TextStyle::Body))
        .max(ui.spacing().interact_size.y)
}

pub fn button_group_height(ui: &egui::Ui) -> f32 {
    control_height(ui) + 2. * f32::from(BUTTON_GROUP_MARGIN)
}

/// Square controls share fixed sizes in logical pixels.
#[derive(Clone, Copy)]
pub enum ControlSize {
    Small,
    Medium,
    Large,
}

impl ControlSize {
    pub const fn side(self) -> f32 {
        match self {
            Self::Small => 14.,
            Self::Medium => 18.,
            Self::Large => 20.,
        }
    }

    pub const fn group_height(self) -> f32 {
        self.side() + 2. * BUTTON_GROUP_MARGIN as f32
    }
}

fn menu_label(ui: &egui::Ui, label: &str) -> std::sync::Arc<egui::Galley> {
    egui::WidgetText::from(label).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Button,
    )
}

fn menu_content_width<'a>(ui: &egui::Ui, labels: impl IntoIterator<Item = &'a str>) -> f32 {
    labels
        .into_iter()
        .map(|label| menu_label(ui, label).size().x)
        .fold(0., f32::max)
        + 2. * button_padding(ui).x
}

/// Fit the longest label plus the dropdown arrow and normal control padding.
pub fn dropdown_width<'a>(ui: &egui::Ui, labels: impl IntoIterator<Item = &'a str>) -> f32 {
    menu_content_width(ui, labels) + ui.spacing().icon_width + ui.spacing().icon_spacing
}

pub fn dropdown<R>(
    ui: &mut egui::Ui,
    combo: egui::ComboBox,
    menu_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    // ComboBox creates an inner horizontal row using interact_size.y before
    // measuring its text. Reserve the padded height so that row cannot start
    // lower than adjacent buttons. Keep the parent UI and popup IDs stable.
    let previous = ui.spacing().clone();
    ui.spacing_mut().button_padding = button_padding(ui);
    ui.spacing_mut().interact_size.y = control_height(ui);
    let width_key = ui.next_auto_id().with("dropdown outer width");
    let outer_width = ui
        .data(|d| d.get_temp::<f32>(width_key))
        .unwrap_or(ui.available_width());
    let response = combo.show_ui(ui, |ui| {
        // egui's popup starts with a provisional width. Pin its content to the
        // closed control, excluding the popup frame's own padding and border.
        let margin = egui::Frame::popup(ui.style()).total_margin().sum().x;
        ui.set_width((outer_width - margin).max(1.));
        menu_contents(ui)
    });
    ui.data_mut(|d| d.insert_temp(width_key, response.response.rect.width()));
    ui.spacing_mut().button_padding = previous.button_padding;
    ui.spacing_mut().interact_size = previous.interact_size;
    response
}

pub fn button(ui: &mut egui::Ui, label: &str, on: bool, minimum: egui::Vec2) -> egui::Response {
    // egui removes padding from frameless buttons. Keep its frame invisible
    // so we can use padded layout and paint our own border and colors.
    let background = ui.painter().add(egui::Shape::Noop);
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = button_padding(ui);
            ui.add(
                egui::Button::new(label)
                    .frame(true)
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(egui::Stroke::NONE)
                    .min_size(minimum),
            )
        })
        .inner;
    ui.painter()
        .set(background, frame(ui, &response, on, response.rect));
    response
}

/// Fixed-height text and icon buttons share their frame, spacing and states.
pub fn button_sized(ui: &mut egui::Ui, label: &str, on: bool, size: ControlSize) -> egui::Response {
    let text = menu_label(ui, label);
    let width = if label.is_empty() {
        size.side()
    } else {
        (text.size().x + 2. * button_padding(ui).x).max(size.side())
    };
    let (_, response) =
        ui.allocate_exact_size(egui::vec2(width, size.side()), egui::Sense::click());
    paint(ui, &response, on);
    ui.painter().galley(
        response.rect.center() - text.size() * 0.5,
        text,
        visuals(ui, &response, on).text_color(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), on, label)
    });
    response
}

/// Square icon controls share padding and interaction states across sizes.
pub fn icon_button(ui: &mut egui::Ui, label: &str, on: bool, size: ControlSize) -> egui::Response {
    let response = button_sized(ui, "", on, size);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), on, label)
    });
    response
}

/// Automatic controls share the square geometry of ordinary icon buttons.
pub fn auto_button(ui: &mut egui::Ui, on: bool) -> egui::Response {
    let response = icon_button(ui, "A", on, ControlSize::Large);
    let label = menu_label(ui, "A");
    ui.painter().galley(
        response.rect.center() - label.size() * 0.5,
        label,
        ui.style().interact_selectable(&response, on).text_color(),
    );
    response
}

fn menu_style(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing.y = 0.;
    ui.spacing_mut().button_padding = button_padding(ui);
}

/// Shared primary-click menus retain native drawing and content-sized rows.
pub fn menu_button<R>(
    ui: &mut egui::Ui,
    button: egui::Button<'_>,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let response = ui.add(button);
    menu_response(ui, response, contents)
}

fn menu_response<R>(
    ui: &mut egui::Ui,
    response: egui::Response,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let id = response.id.with("workspace menu");
    if response.clicked() {
        ui.memory_mut(|m| m.toggle_popup(id));
    }
    if !ui.memory(|m| m.is_popup_open(id)) {
        ui.data_mut(|d| d.remove::<egui::menu::MenuRoot>(id));
        return egui::InnerResponse::new(None, response);
    }
    let root = ui
        .data_mut(|d| d.get_temp::<egui::menu::MenuRoot>(id))
        .unwrap_or_else(|| egui::menu::MenuRoot::new(response.rect.left_bottom(), response.id));
    // egui 0.31 returns contents only when closing. Retain them while open too.
    let mut inner = None;
    let (close, _) = root.show(&response, |ui| {
        menu_style(ui);
        inner = Some(contents(ui));
    });
    let outside = ui.input(|i| {
        i.pointer.any_pressed()
            && i.pointer.interact_pos().is_some_and(|p| {
                !response.rect.contains(p) && !root.menu_state.read().area_contains(p)
            })
    });
    if close.is_close() || outside {
        ui.memory_mut(|m| m.close_popup());
        ui.data_mut(|d| d.remove::<egui::menu::MenuRoot>(id));
    } else {
        ui.data_mut(|d| d.insert_temp(id, root));
    }
    egui::InnerResponse::new(inner, response)
}

/// Group exclusive choices under an inert heading, with only the options indented.
pub fn menu_choices<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    heading: &str,
    selected: &mut T,
    choices: &[(T, &str)],
) -> bool {
    ui.add_enabled_ui(false, |ui| menu_row(ui, heading, false));
    ui.scope(|ui| {
        ui.visuals_mut().indent_has_left_vline = false;
        ui.indent(heading, |ui| {
            let mut changed = false;
            for &(value, label) in choices {
                let response = menu_value(ui, selected, value, label);
                changed |= response.changed();
                if response.clicked() {
                    ui.close_menu();
                }
            }
            changed
        })
        .inner
    })
    .inner
}

pub fn icon_menu_button<R>(
    ui: &mut egui::Ui,
    label: &str,
    size: ControlSize,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let background = ui.painter().add(egui::Shape::Noop);
    let (_, response) =
        ui.allocate_exact_size(egui::Vec2::splat(size.side()), egui::Sense::click());
    let result = menu_response(ui, response, contents);
    let on = result.inner.is_some();
    ui.painter().set(
        background,
        frame(ui, &result.response, on, result.response.rect),
    );
    result.response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), on, label)
    });
    result
}

/// Center a square glyph using the standard vertical padding on all four sides.
pub fn icon_rect(ui: &egui::Ui, response: &egui::Response) -> egui::Rect {
    let side =
        (response.rect.width().min(response.rect.height()) - 2. * button_padding(ui).y).max(0.);
    egui::Rect::from_center_size(response.rect.center(), egui::Vec2::splat(side))
}

pub fn checkbox(
    ui: &mut egui::Ui,
    value: &mut bool,
    label: impl Into<egui::WidgetText>,
) -> egui::Response {
    checkbox_sized(ui, value, label, ControlSize::Small)
}

pub fn checkbox_sized(
    ui: &mut egui::Ui,
    value: &mut bool,
    label: impl Into<egui::WidgetText>,
    size: ControlSize,
) -> egui::Response {
    let side = size.side();
    let background = ui.painter().add(egui::Shape::Noop);
    let (response, icon) = ui
        .scope(|ui| {
            let spacing = ui.spacing_mut();
            spacing.icon_width_inner *= side / spacing.icon_width;
            spacing.icon_width = side;
            let widgets = &mut ui.visuals_mut().widgets;
            for visual in [
                &mut widgets.inactive,
                &mut widgets.hovered,
                &mut widgets.active,
                &mut widgets.noninteractive,
            ] {
                visual.bg_fill = egui::Color32::TRANSPARENT;
                visual.bg_stroke = egui::Stroke::NONE;
            }
            let response = ui.checkbox(value, label);
            let (_, icon) = ui.spacing().icon_rectangles(response.rect);
            (response, icon)
        })
        .inner;
    ui.painter()
        .set(background, frame(ui, &response, false, icon));
    response
}

/// Noninteractive color samples align with the corresponding square controls.
pub fn swatch(ui: &mut egui::Ui, color: egui::Color32, size: ControlSize) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::Vec2::splat(size.side()), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(RADIUS), color);
    response
}

pub fn selectable_value<T: PartialEq>(
    ui: &mut egui::Ui,
    value: &mut T,
    choice: T,
    label: &str,
) -> egui::Response {
    let mut response = button(ui, label, *value == choice, egui::Vec2::ZERO);
    if response.clicked() && *value != choice {
        *value = choice;
        response.mark_changed();
    }
    response
}

/// Selection lists fill the available width with a left-aligned, truncated name.
pub fn selectable_row(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    size: ControlSize,
) -> egui::Response {
    let width = ui.available_width().max(size.side());
    let padding = ui.spacing().button_padding.x;
    let text = egui::WidgetText::from(label).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        (width - 2. * padding).max(0.),
        egui::TextStyle::Body,
    );
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, size.side()), egui::Sense::click());
    if selected || response.hovered() || response.has_focus() {
        paint(ui, &response, selected);
    }
    ui.painter().galley(
        egui::pos2(rect.left() + padding, rect.center().y - text.size().y * 0.5),
        text,
        visuals(ui, &response, selected).text_color(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    response
}

pub fn menu_cell(ui: &egui::Ui, response: &egui::Response, selected: bool) -> egui::Color32 {
    let hovered = response.enabled()
        && (response.hovered() || response.has_focus() || response.highlighted());
    let background = if selected {
        ui.visuals().selection.bg_fill
    } else if hovered {
        egui::Color32::from_gray(60)
    } else {
        MENU_BACKGROUND
    };
    ui.painter()
        .rect_filled(response.rect, egui::CornerRadius::ZERO, background);
    egui::Color32::from_gray(if hovered { 240 } else { 180 })
}

pub fn menu_row(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let padding = button_padding(ui);
    let galley = menu_label(ui, label);
    let intrinsic_width = galley.size().x + 2. * padding.x;
    // Popup sizing passes measure content before the available width is known.
    // Filling egui's provisional 400-point menu width here makes that width permanent.
    let width = if ui.is_sizing_pass() {
        intrinsic_width
    } else {
        ui.available_width().max(intrinsic_width)
    };
    let size = egui::vec2(
        width,
        ui.spacing()
            .interact_size
            .y
            .max(galley.size().y + 2. * padding.y),
    );
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let color = menu_cell(ui, &response, selected);
    ui.painter().galley(
        egui::pos2(
            rect.left() + padding.x,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        color,
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    response
}

pub fn menu_value<T: PartialEq>(
    ui: &mut egui::Ui,
    value: &mut T,
    choice: T,
    label: &str,
) -> egui::Response {
    let mut response = menu_row(ui, label, *value == choice);
    if response.clicked() && *value != choice {
        *value = choice;
        response.mark_changed();
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_menu_choices_fit_labels_indent_options_and_keep_heading_inert() {
        for em in [14., 20.] {
            for x in [10., 530.] {
                let ctx = egui::Context::default();
                install_style(&ctx);
                ctx.style_mut(|s| {
                    s.text_styles
                        .insert(egui::TextStyle::Button, egui::FontId::proportional(em));
                });
                let mut selected = 2;
                let mut draw = |time, events| {
                    let mut open = None;
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(600., 400.),
                            )),
                            time: Some(time),
                            events,
                            ..Default::default()
                        },
                        |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                let mut anchor = ui.new_child(
                                    egui::UiBuilder::new().id_salt("menu fixture").max_rect(
                                        egui::Rect::from_min_size(
                                            egui::pos2(x, 20.),
                                            egui::vec2(55., 25.),
                                        ),
                                    ),
                                );
                                open = menu_button(&mut anchor, egui::Button::new("Menu"), |ui| {
                                    menu_choices(
                                        ui,
                                        "Auto exposure source",
                                        &mut selected,
                                        &[(0, "Bracket"), (1, "Dial"), (2, "Both")],
                                    );
                                    ui.separator();
                                    let _ = ui.button("Save preset…");
                                    let _ = ui.button("Set as default for Quattro");
                                    ui.min_rect()
                                })
                                .inner;
                            });
                        },
                    );
                    eprintln!(
                        "frame {time} open {} marker {}",
                        open.is_some(),
                        ctx.memory(|m| m.any_popup_open())
                    );
                    (output, open)
                };
                let text = |output: &egui::FullOutput, label: &str| {
                    output
                        .shapes
                        .iter()
                        .find_map(|s| match &s.shape {
                            egui::Shape::Text(t) if t.galley.job.text == label => {
                                Some(egui::Rect::from_min_size(t.pos, t.galley.size()))
                            }
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("missing {label}"))
                };
                let click = |pos, pressed| egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                };
                let (output, _) = draw(0., vec![]);
                let button = text(&output, "Menu").center();
                draw(
                    1.,
                    vec![egui::Event::PointerMoved(button), click(button, true)],
                );
                draw(1.05, vec![click(button, false)]);
                let (output, open) = draw(1.1, vec![]);
                let heading = text(&output, "Auto exposure source");
                let bracket = text(&output, "Bracket");
                let dial = text(&output, "Dial");
                let both = text(&output, "Both");
                let save = text(&output, "Save preset…");
                let longest = text(&output, "Set as default for Quattro")
                    .width()
                    .max(heading.width());
                assert!(
                    open.unwrap().width() < longest + em * 2.5,
                    "menu stretches beyond its labels"
                );
                assert!(
                    (heading.left() - save.left()).abs() < 1.,
                    "heading must remain unindented"
                );
                for option in [bracket, dial, both] {
                    assert!(
                        (option.left() - heading.left() - ctx.style().spacing.indent).abs() < 1.
                    );
                }
                assert!(
                    (dial.top() - bracket.bottom() - em * 0.5).abs() < 1.,
                    "choices must keep shared row spacing"
                );
                assert!((both.top() - dial.bottom() - em * 0.5).abs() < 1.);
                draw(
                    2.,
                    vec![
                        egui::Event::PointerMoved(heading.center()),
                        click(heading.center(), true),
                    ],
                );
                draw(2.05, vec![click(heading.center(), false)]);
                assert!(
                    draw(2.1, vec![]).1.is_some(),
                    "the heading must not select or dismiss"
                );
                assert!(ctx.memory(|m| m.any_popup_open()));
                draw(
                    3.,
                    vec![
                        egui::Event::PointerMoved(dial.center()),
                        click(dial.center(), true),
                    ],
                );
                draw(3.05, vec![click(dial.center(), false)]);
                assert!(
                    draw(3.1, vec![]).1.is_none(),
                    "choice must dismiss the menu"
                );
                assert!(!ctx.memory(|m| m.any_popup_open()));
                draw(
                    4.,
                    vec![egui::Event::PointerMoved(button), click(button, true)],
                );
                draw(4.05, vec![click(button, false)]);
                let outside = egui::pos2(300., 380.);
                draw(
                    4.1,
                    vec![egui::Event::PointerMoved(outside), click(outside, true)],
                );
                draw(4.15, vec![click(outside, false)]);
                assert!(draw(4.2, vec![]).1.is_none());
                draw(
                    5.,
                    vec![egui::Event::PointerMoved(button), click(button, true)],
                );
                draw(5.05, vec![click(button, false)]);
                draw(
                    5.1,
                    vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Default::default(),
                    }],
                );
                assert!(draw(5.15, vec![]).1.is_none());
                assert_eq!(selected, 1);
            }
        }
    }

    #[test]
    fn disclosure_line_aligns_with_parent_and_preserves_body_indentation() {
        let ctx = egui::Context::default();
        install_style(&ctx);
        let mut parent_left = 0.;
        let mut child_left = 0.;
        let mut em = 0.;
        let output = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                em = egui::TextStyle::Body.resolve(ui.style()).size;
                parent_left = ui.available_rect_before_wrap().left();
                disclosure(
                    ui,
                    egui::CollapsingHeader::new("Options").default_open(true),
                    |ui| {
                        child_left = ui.available_rect_before_wrap().left();
                        ui.label("Child");
                    },
                );
            });
        });
        assert_eq!(child_left - parent_left, em);
        assert!(output.shapes.iter().any(|shape| match &shape.shape {
            egui::Shape::LineSegment { points, .. } => {
                points[0].x == parent_left && points[1].x == parent_left
            }
            _ => false,
        }));
    }

    fn render(
        ctx: &egui::Context,
        on: bool,
        enabled: bool,
        time: f64,
        events: Vec<egui::Event>,
    ) -> (egui::Response, egui::style::WidgetVisuals, egui::Color32) {
        let mut result = None;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400., 100.),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add_enabled_ui(enabled, |ui| {
                        let response = auto_button(ui, on);
                        let visual = visuals(ui, &response, on);
                        result = Some((response, visual));
                    });
                });
            },
        );
        let (response, visual) = result.unwrap();
        let fill = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.rect.contains(response.rect.center())
                        && rect.rect.width() < 30.
                        && rect.stroke.width > 0. =>
                {
                    assert_eq!(rect.corner_radius, egui::CornerRadius::same(RADIUS));
                    Some(rect.fill)
                }
                _ => None,
            })
            .unwrap();
        (response, visual, fill)
    }

    #[test]
    fn file_count_and_zoom_rows_center_labels_and_buttons_from_the_first_frame() {
        for em in [13., 20.] {
            for (label, buttons) in [
                ("16 files", &["Icons", "List"][..]),
                ("Zoom 41%", &["100%", "200%", "400%"][..]),
            ] {
                let ctx = egui::Context::default();
                install_style(&ctx);
                ctx.style_mut(|style| {
                    style.override_font_id = Some(egui::FontId::proportional(em))
                });
                let mut label_rect = egui::Rect::NOTHING;
                let mut button_rects = Vec::new();
                let output = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        control_row(ui, |ui| {
                            label_rect = ui.label(label).rect;
                            for name in buttons {
                                button_rects.push(button(ui, name, false, egui::Vec2::ZERO).rect);
                            }
                        });
                    });
                });
                for rect in button_rects {
                    assert!((rect.center().y - label_rect.center().y).abs() < 0.1);
                }
                let text_center = |label: &str| {
                    output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.job.text == label => {
                                Some(text.pos.y + text.galley.size().y * 0.5)
                            }
                            _ => None,
                        })
                        .unwrap()
                };
                for name in buttons {
                    assert!((text_center(name) - text_center(label)).abs() < 1.);
                }
            }
        }
    }

    #[test]
    fn file_count_and_zoom_bands_balance_their_surrounding_vertical_spacing() {
        for pixels_per_point in [1., 2.] {
            for em in [13., 20.] {
                let ctx = egui::Context::default();
                install_style(&ctx);
                ctx.set_pixels_per_point(pixels_per_point);
                ctx.style_mut(|style| {
                    style.override_font_id = Some(egui::FontId::proportional(em))
                });
                let _ = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        for label in ["16 files", "Zoom 50%"] {
                            let before = ui.next_widget_position().y;
                            let gap = ui.spacing().item_spacing.y;
                            let row = control_row(ui, |ui| {
                                ui.label(label);
                                button(ui, "Mode", false, egui::Vec2::ZERO);
                            })
                            .response
                            .rect;
                            let after = ui.next_widget_position().y;
                            assert!((row.top() - before - gap).abs() < 0.01);
                            assert!((after - row.bottom() - gap).abs() < 0.01);
                        }
                    });
                });
            }
        }
    }

    #[test]
    fn fitting_button_labels_follow_actual_button_width_and_restore_on_resize() {
        for em in [14., 20.] {
            let ctx = egui::Context::default();
            install_style(&ctx);
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.style_mut().override_font_id = Some(egui::FontId::proportional(em));
                    let width = button(ui, "Advanced", false, egui::Vec2::ZERO).rect.width();
                    for (available, expected) in [
                        (width + 1., "Advanced"),
                        (width - 1., "Adv."),
                        (width + 1., "Advanced"),
                    ] {
                        assert_eq!(
                            fitting_button_label(ui, "Advanced", "Adv.", available),
                            expected
                        );
                    }
                });
            });
        }
    }

    #[test]
    fn buttons_pad_text_in_font_units_without_changing_item_spacing() {
        for em in [14., 20.] {
            let ctx = egui::Context::default();
            install_style(&ctx);
            let mut rect = egui::Rect::NOTHING;
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.style_mut().override_font_id = Some(egui::FontId::proportional(em));
                    ui.horizontal(|ui| {
                        let spacing = ui.spacing().clone();
                        rect = button(ui, "Padding", false, egui::Vec2::ZERO).rect;
                        let next = button(ui, "Next", true, egui::Vec2::ZERO).rect;
                        assert!((next.left() - rect.right() - spacing.item_spacing.x).abs() < 0.01);
                        assert_eq!(ui.spacing().button_padding, spacing.button_padding);
                        assert_eq!(ui.spacing().item_spacing, spacing.item_spacing);
                    });
                });
            });
            let text = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == "Padding" => Some(text),
                    _ => None,
                })
                .unwrap();
            let size = text.galley.size();
            assert!((rect.width() - size.x - em).abs() < 1.);
            assert!((rect.height() - size.y - 0.5 * em).abs() < 1.);
            let bounds = egui::Rect::from_min_size(text.pos, size);
            for gap in [bounds.left() - rect.left(), rect.right() - bounds.right()] {
                assert!((gap - 0.5 * em).abs() < 1.);
            }
            for gap in [bounds.top() - rect.top(), rect.bottom() - bounds.bottom()] {
                assert!((gap - 0.25 * em).abs() < 1.);
            }
        }
    }

    #[test]
    fn dropdowns_share_button_padding_without_changing_ids_or_parent_spacing() {
        for em in [14., 20.] {
            for enabled in [false, true] {
                let ctx = egui::Context::default();
                install_style(&ctx);
                let mut rect = egui::Rect::NOTHING;
                let output = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.style_mut().override_font_id = Some(egui::FontId::proportional(em));
                        ui.add_enabled_ui(enabled, |ui| {
                            centered_control_row(ui, |ui| {
                                let spacing = ui.spacing().clone();
                                let before = button(ui, "Before", false, egui::Vec2::ZERO).rect;
                                let id = ui.make_persistent_id(egui::Id::new("padded dropdown"));
                                let response = dropdown(
                                    ui,
                                    egui::ComboBox::from_id_salt("padded dropdown")
                                        .selected_text("Padding"),
                                    |_| {},
                                )
                                .response;
                                assert_eq!(response.id, id);
                                assert_eq!(response.enabled(), enabled);
                                rect = response.rect;
                                let next = button(ui, "Next", false, egui::Vec2::ZERO).rect;
                                assert!((next.height() - rect.height()).abs() < 1.);
                                assert!(
                                    (before.center().y - rect.center().y).abs() < 0.5,
                                    "before={before:?} dropdown={rect:?} next={next:?}"
                                );
                                assert!((next.center().y - rect.center().y).abs() < 0.5);
                                assert!(
                                    (next.left() - rect.right() - spacing.item_spacing.x).abs()
                                        < 0.01
                                );
                                assert_eq!(ui.spacing().button_padding, spacing.button_padding);
                                assert_eq!(ui.spacing().item_spacing, spacing.item_spacing);
                                assert_eq!(ui.spacing().interact_size, spacing.interact_size);
                            });
                        });
                    });
                });
                let text = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == "Padding" => Some(text),
                        _ => None,
                    })
                    .unwrap();
                let bounds = egui::Rect::from_min_size(text.pos, text.galley.size());
                assert!((bounds.left() - rect.left() - 0.5 * em).abs() < 1.);
                for gap in [bounds.top() - rect.top(), rect.bottom() - bounds.bottom()] {
                    assert!((gap - 0.25 * em).abs() < 1.);
                }
            }
        }
    }

    #[test]
    fn open_dropdown_options_keep_font_based_padding_and_gap_free_rows() {
        for em in [14., 20.] {
            let ctx = egui::Context::default();
            install_style(&ctx);
            // Popup areas use the context's font style rather than local overrides.
            ctx.style_mut(|style| {
                style
                    .text_styles
                    .insert(egui::TextStyle::Button, egui::FontId::proportional(em));
            });
            let draw = |time| {
                let mut response = None;
                let output = ctx.run(
                    egui::RawInput {
                        time: Some(time),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let spacing = ui.spacing().clone();
                            response = Some(dropdown(
                                ui,
                                egui::ComboBox::from_id_salt("padded options")
                                    .selected_text("Closed"),
                                |ui| {
                                    ui.spacing_mut().item_spacing.y = 0.;
                                    let first = menu_row(ui, "Option", false).rect;
                                    let next = menu_row(ui, "Next", true).rect;
                                    if !ui.is_sizing_pass() {
                                        assert_eq!(first.width(), next.width());
                                    }
                                    assert_eq!(first.bottom(), next.top());
                                    first
                                },
                            ));
                            assert_eq!(ui.spacing().button_padding, spacing.button_padding);
                            assert_eq!(ui.spacing().item_spacing, spacing.item_spacing);
                        });
                    },
                );
                (response.unwrap(), output.shapes)
            };
            let (closed, _) = draw(0.);
            ctx.memory_mut(|memory| memory.open_popup(closed.response.id.with("popup")));
            draw(0.02);
            let (open, shapes) = draw(0.04);
            let rect = open.inner.unwrap();
            let text = shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == "Option" => Some(text),
                    _ => None,
                })
                .unwrap();
            assert_eq!(text.galley.job.sections[0].format.font_id.size, em);
            let bounds = egui::Rect::from_min_size(text.pos, text.galley.size());
            assert!(
                (bounds.left() - rect.left() - 0.5 * em).abs() < 1.,
                "{em} pt dropdown: option bounds {bounds:?}, row {rect:?}, font {:?}",
                text.galley.job.sections[0].format.font_id,
            );
            for gap in [bounds.top() - rect.top(), rect.bottom() - bounds.bottom()] {
                assert!((gap - 0.25 * em).abs() < 1.);
            }
        }
    }

    #[test]
    fn dropdown_cells_are_full_width_square_borderless_and_use_selection_hover_colors() {
        for selected in [false, true] {
            let ctx = egui::Context::default();
            install_style(&ctx);
            let render = |time, events| {
                let mut rect = egui::Rect::NOTHING;
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(400., 200.),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.;
                            let width = ui.available_width();
                            rect = menu_row(ui, "Option", selected).rect;
                            assert_eq!(rect.width(), width);
                            let next = menu_row(ui, "Next", false).rect;
                            assert_eq!(next.top(), rect.bottom());
                            assert_eq!(next.width(), rect.width());
                        });
                    },
                );
                let background = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Rect(shape) if shape.rect == rect => {
                            assert_eq!(shape.corner_radius, egui::CornerRadius::ZERO);
                            assert_eq!(shape.stroke, egui::Stroke::NONE);
                            Some(shape.fill)
                        }
                        _ => None,
                    })
                    .unwrap();
                let text = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == "Option" => {
                            Some(text.fallback_color)
                        }
                        _ => None,
                    })
                    .unwrap();
                (rect, background, text)
            };
            render(0., vec![]);
            let (rect, background, text) = render(0.02, vec![]);
            assert_eq!(
                background,
                if selected {
                    egui::Color32::from_rgb(35, 65, 100)
                } else {
                    egui::Color32::from_gray(29)
                }
            );
            assert_eq!(text, egui::Color32::from_gray(180));
            let (_, background, text) =
                render(0.04, vec![egui::Event::PointerMoved(rect.center())]);
            assert_eq!(
                background,
                if selected {
                    egui::Color32::from_rgb(35, 65, 100)
                } else {
                    egui::Color32::from_gray(60)
                }
            );
            assert_eq!(text, egui::Color32::from_gray(240));
        }
    }

    #[test]
    fn dropdown_cell_edges_select_values_and_keep_changed_signals() {
        let ctx = egui::Context::default();
        install_style(&ctx);
        let mut value = 0;
        let mut render = |time, events| {
            let mut result = None;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 200.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.set_width(260.);
                        result = Some(menu_value(ui, &mut value, 1, "Choice"));
                    });
                },
            );
            (result.unwrap(), value)
        };
        let (response, _) = render(0., vec![]);
        let point = response.rect.right_center() - egui::vec2(2., 0.);
        render(0.02, vec![egui::Event::PointerMoved(point)]);
        let event = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        assert!(!render(0.04, vec![event(true)]).0.changed());
        let (response, value) = render(0.06, vec![event(false)]);
        assert!(response.changed());
        assert_eq!(value, 1);
    }

    #[test]
    fn neutral_and_accent_states_keep_rounding_hover_press_and_disabled_feedback() {
        for on in [false, true] {
            let ctx = egui::Context::default();
            install_style(&ctx);
            let (response, normal, normal_fill) = render(&ctx, on, true, 0., vec![]);
            assert_eq!(response.rect.width(), response.rect.height());
            assert!(response.rect.height() >= 20.);
            assert_eq!(
                normal.bg_fill,
                if on {
                    egui::Color32::from_rgb(35, 65, 100)
                } else {
                    egui::Color32::from_gray(29)
                }
            );
            let point = response.rect.center();
            let (_, hovered, _) =
                render(&ctx, on, true, 1., vec![egui::Event::PointerMoved(point)]);
            assert_ne!(normal.bg_stroke, hovered.bg_stroke);
            let event = |pressed| egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            let (_, pressed, _) = render(&ctx, on, true, 1.1, vec![event(true)]);
            assert_eq!(
                pressed.bg_fill,
                if on {
                    egui::Color32::from_rgb(24, 45, 72)
                } else {
                    egui::Color32::from_gray(24)
                }
            );
            assert!(render(&ctx, on, true, 1.2, vec![event(false)]).0.clicked());
            let (disabled, _, disabled_fill) = render(&ctx, on, false, 2., vec![event(true)]);
            assert!(!disabled.enabled());
            assert!(!disabled.clicked());
            assert_ne!(normal_fill, disabled_fill);
            assert!(!render(&ctx, on, false, 2.1, vec![event(false)]).0.clicked());
        }
    }

    #[test]
    fn keyboard_focus_uses_hover_and_enter_still_activates_buttons() {
        let ctx = egui::Context::default();
        install_style(&ctx);
        let (response, _, _) = render(&ctx, true, true, 0., vec![]);
        ctx.memory_mut(|memory| memory.request_focus(response.id));
        let (response, focused, _) = render(&ctx, true, true, 1., vec![]);
        assert!(response.has_focus());
        assert_eq!(focused.bg_fill, egui::Color32::from_rgb(35, 65, 100));
        assert_eq!(
            focused.bg_stroke.color,
            egui::Color32::from_rgb(140, 190, 255)
        );
        let (response, pressed, _) = render(
            &ctx,
            true,
            true,
            1.1,
            vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert!(response.clicked());
        assert_eq!(pressed.bg_fill, egui::Color32::from_rgb(24, 45, 72));
    }

    #[test]
    fn checkboxes_stay_neutral_and_keep_checkmarks_and_native_changed_signals() {
        let ctx = egui::Context::default();
        install_style(&ctx);
        let mut checked = false;
        let mut render = |time, events| {
            let mut result = None;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 100.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let response = checkbox(ui, &mut checked, "Enabled");
                        result = Some((response, checked));
                    });
                },
            );
            let (response, checked) = result.unwrap();
            let checkmark = output.shapes.iter().any(|shape| {
                matches!(&shape.shape,
                    egui::Shape::Path(path) if path.points.len() == 3 && path.stroke.width > 0.
                )
            });
            assert_eq!(checkmark, checked);
            let fill = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if rect.rect.width() < 30. && rect.stroke.width > 0. =>
                    {
                        assert_eq!(rect.corner_radius, egui::CornerRadius::same(RADIUS));
                        assert_eq!(rect.stroke.color.r(), rect.stroke.color.g());
                        assert_eq!(rect.stroke.color.g(), rect.stroke.color.b());
                        assert_eq!(rect.fill.r(), rect.fill.g());
                        assert_eq!(rect.fill.g(), rect.fill.b());
                        Some(rect.fill)
                    }
                    _ => None,
                })
                .unwrap();
            (response, checked, fill)
        };
        let (response, value, fill) = render(0., vec![]);
        assert!(!value);
        assert_eq!(fill, egui::Color32::from_gray(29));
        let point = response.rect.center();
        assert_eq!(
            render(1., vec![egui::Event::PointerMoved(point)]).2,
            egui::Color32::from_gray(34)
        );
        let event = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let (response, value, fill) = render(1.1, vec![event(true)]);
        assert!(!response.changed());
        assert!(!value);
        assert_eq!(fill, egui::Color32::from_gray(24));
        let (response, value, fill) = render(1.2, vec![event(false)]);
        assert!(response.changed());
        assert!(value);
        assert_eq!(fill, egui::Color32::from_gray(24));
        let (response, value, fill) = render(1.3, vec![]);
        assert!(!response.changed());
        assert!(value);
        assert_eq!(fill, egui::Color32::from_gray(34));
        ctx.memory_mut(|memory| memory.surrender_focus(response.id));
        let (_, value, fill) = render(1.4, vec![egui::Event::PointerMoved(egui::pos2(350., 80.))]);
        assert!(value);
        assert_eq!(fill, egui::Color32::from_gray(29));
    }
}

#[cfg(test)]
#[path = "widgets_numeric_test.rs"]
mod numeric_tests;

#[cfg(test)]
mod icon_tests {
    use super::*;

    #[test]
    fn icon_controls_keep_fixed_squares_centered_with_text_buttons_across_styles() {
        for theme in [egui::Theme::Dark, egui::Theme::Light] {
            for em in [13., 20., 28.] {
                let ctx = egui::Context::default();
                install_style(&ctx);
                ctx.set_theme(theme);
                ctx.style_mut(|s| s.override_font_id = Some(egui::FontId::proportional(em)));
                let _ = ctx.run(egui::RawInput::default(), |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let before = ui.spacing().clone();
                        centered_control_row(ui, |ui| {
                            let browse = button(ui, "Browse…", false, egui::Vec2::ZERO);
                            let auto = auto_button(ui, false);
                            let picker = icon_button(ui, "Picker", false, ControlSize::Large);
                            let menu = icon_menu_button(ui, "Settings", ControlSize::Large, |ui| {
                                ui.label("Options");
                            });
                            let menu = menu.response;
                            for response in [&auto, &picker, &menu] {
                                assert!(
                                    (response.rect.height() - ControlSize::Large.side()).abs()
                                        < 0.01,
                                    "{theme:?}, {em}"
                                );
                                assert!(
                                    (response.rect.center().y - browse.rect.center().y).abs()
                                        < 0.01
                                );
                            }
                            for response in [&auto, &picker, &menu] {
                                assert!(
                                    (response.rect.width() - response.rect.height()).abs() < 0.01
                                );
                                let glyph = icon_rect(ui, response);
                                assert_eq!(glyph.center(), response.rect.center());
                                let padding = button_padding(ui).y;
                                assert!(
                                    (glyph.left() - response.rect.left() - padding).abs() < 0.01
                                );
                                assert!((glyph.top() - response.rect.top() - padding).abs() < 0.01);
                            }
                        });
                        assert_eq!(before.button_padding, ui.spacing().button_padding);
                        assert_eq!(before.interact_size, ui.spacing().interact_size);
                    });
                });
            }
        }
    }
}

/// Shared eyedropper glyph inside the standard icon-button inset.
pub fn eyedropper_icon(ui: &egui::Ui, response: &egui::Response, on: bool) {
    let icon = icon_rect(ui, response);
    let scale = icon.width() / 16.;
    let at = |x, y| icon.min + egui::vec2(x, y) * scale;
    let color = visuals(ui, response, on).fg_stroke.color;
    let stroke = egui::Stroke::new(1.5 * scale, color);
    ui.painter().add(egui::Shape::closed_line(
        vec![
            at(1., 15.),
            at(2., 11.),
            at(10., 3.),
            at(13., 6.),
            at(5., 14.),
        ],
        stroke,
    ));
    ui.painter().line_segment([at(8., 1.), at(15., 8.)], stroke);
    ui.painter()
        .circle_filled(at(13.5, 2.5), 2.5 * scale, color);
}

pub fn eye_icon(ui: &egui::Ui, rect: egui::Rect, color: egui::Color32, crossed: bool) {
    let point = |x, y| rect.min + egui::vec2(x * rect.width(), y * rect.height());
    let stroke = egui::Stroke::new(1.3_f32, color);
    for height in [0.04, 0.96] {
        ui.painter().add(egui::Shape::CubicBezier(
            egui::epaint::CubicBezierShape::from_points_stroke(
                [
                    point(0., 0.5),
                    point(0.28, height),
                    point(0.72, height),
                    point(1., 0.5),
                ],
                false,
                egui::Color32::TRANSPARENT,
                stroke,
            ),
        ));
    }
    ui.painter()
        .circle_stroke(rect.center(), rect.height() * 0.15, stroke);
    if crossed {
        ui.painter()
            .line_segment([point(0., 1.), point(1., 0.)], stroke);
    }
}

pub fn eye_button(ui: &mut egui::Ui, label: &str, on: bool) -> egui::Response {
    let response = icon_button(ui, label, on, ControlSize::Medium);
    let color = visuals(ui, &response, on).fg_stroke.color;
    eye_icon(ui, icon_rect(ui, &response), color, !on);
    response
}

#[cfg(test)]
mod dropdown_width_tests {
    use super::*;
    #[test]
    fn ordinary_options_stay_within_the_closed_control_width() {
        for em in [14., 20.] {
            for x in [10., 450.] {
                let ctx = egui::Context::default();
                install_style(&ctx);
                ctx.style_mut(|s| {
                    s.text_styles
                        .insert(egui::TextStyle::Button, egui::FontId::proportional(em));
                });
                let draw = |time, events| {
                    let mut result = None;
                    let _ = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(720., 400.),
                            )),
                            time: Some(time),
                            events,
                            ..Default::default()
                        },
                        |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| {
                                let mut anchor = ui.new_child(
                                    egui::UiBuilder::new().id_salt("width fixture").max_rect(
                                        egui::Rect::from_min_size(
                                            egui::pos2(x, 20.),
                                            egui::vec2(220., 300.),
                                        ),
                                    ),
                                );
                                let response = dropdown(
                                    &mut anchor,
                                    egui::ComboBox::from_id_salt("method")
                                        .width(220.)
                                        .selected_text("Universal"),
                                    |ui| {
                                        let mut value = 0;
                                        menu_value(ui, &mut value, 0, "Reference");
                                        menu_value(ui, &mut value, 1, "Universal");
                                        let margin = egui::Frame::popup(ui.style()).total_margin();
                                        (ui.min_rect(), margin)
                                    },
                                );
                                result = Some(response);
                            });
                        },
                    );
                    result.unwrap()
                };
                let response = draw(0., vec![]).response;
                let p = response.rect.center();
                let button = |pressed| egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                };
                draw(0.1, vec![egui::Event::PointerMoved(p), button(true)]);
                draw(0.15, vec![button(false)]);
                let open = draw(0.2, vec![]);
                let (content, margin) = open.inner.expect("dropdown must open");
                let outer_width = content.width() + margin.sum().x;
                assert!(
                    (outer_width - open.response.rect.width()).abs() < 1.,
                    "menu {outer_width}, button {:?}",
                    open.response.rect
                );
                assert!((content.left() - margin.left - open.response.rect.left()).abs() < 1.);
            }
        }
    }
}
