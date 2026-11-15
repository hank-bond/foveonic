//! Owns transient source-anchored loupe gestures, persisted options and independent lens geometry.

use crate::{
    photo_tools::{self, Tool},
    widgets::{self, ControlSize},
};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Size {
    Small,
    #[default]
    Medium,
    Large,
    ExtraLarge,
}
impl Size {
    pub const ALL: [Self; 4] = [Self::Small, Self::Medium, Self::Large, Self::ExtraLarge];
    pub fn diameter(self) -> f32 {
        match self {
            Self::Small => 160.,
            Self::Medium => 240.,
            Self::Large => 320.,
            Self::ExtraLarge => 400.,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Small => "Small",
            Self::Medium => "Medium",
            Self::Large => "Large",
            Self::ExtraLarge => "Extra Large",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Zoom {
    Percent25,
    Percent50,
    #[default]
    Percent100,
    Percent200,
    Percent400,
}
impl Zoom {
    pub const ALL: [Self; 5] = [
        Self::Percent25,
        Self::Percent50,
        Self::Percent100,
        Self::Percent200,
        Self::Percent400,
    ];
    pub fn multiple(self) -> f32 {
        match self {
            Self::Percent25 => 0.25,
            Self::Percent50 => 0.5,
            Self::Percent100 => 1.,
            Self::Percent200 => 2.,
            Self::Percent400 => 4.,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Percent25 => "25%",
            Self::Percent50 => "50%",
            Self::Percent100 => "100%",
            Self::Percent200 => "200%",
            Self::Percent400 => "400%",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub size: Size,
    pub zoom: Zoom,
    pub centered: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Gesture {
    pub path: PathBuf,
    /// Position in the displayed cropped and oriented photograph, from zero to one.
    pub focus_uv: [f32; 2],
    pub pointer: egui::Pos2,
}
#[derive(Clone, Default)]
struct State {
    gesture: Option<Gesture>,
    menu: bool,
}
fn state(ctx: &egui::Context) -> State {
    ctx.data_mut(|data| data.get_temp(egui::Id::new("Loupe tool")))
        .unwrap_or_default()
}
fn store(ctx: &egui::Context, state: State) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("Loupe tool"), state));
}
pub fn active(ctx: &egui::Context) -> bool {
    photo_tools::selected(ctx) == Tool::Loupe
}
pub fn gesture(ctx: &egui::Context) -> Option<Gesture> {
    state(ctx).gesture
}
pub fn menu_open(ctx: &egui::Context) -> bool {
    state(ctx).menu
}
pub fn cancel(ctx: &egui::Context) {
    let s = state(ctx);
    if s.gesture.is_some() || s.menu {
        store(ctx, State::default());
        ctx.request_repaint();
    }
}
pub fn activate(ctx: &egui::Context) {
    photo_tools::select(ctx, Tool::Loupe);
}

/// Call for each displayed photograph. A held gesture stays with its first source.
/// The caller cancels if that source leaves the displayed set.
pub fn update(ctx: &egui::Context, response: &egui::Response, path: &Path, image: egui::Rect) {
    let mut s = state(ctx);
    let (space, down, pressed, pointer) = ctx.input(|input| {
        (
            input.key_down(egui::Key::Space),
            input.pointer.primary_down(),
            input.pointer.primary_pressed(),
            input.pointer.latest_pos(),
        )
    });
    if active(ctx) && !space && response.hovered() {
        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    if !active(ctx) || space || !down {
        if s.gesture.take().is_some() {
            store(ctx, s);
            ctx.request_repaint();
        }
        return;
    }
    let Some(pointer) = pointer.filter(|point| point.is_finite()) else {
        return;
    };
    if !image.is_positive() || !image.min.is_finite() || !image.max.is_finite() {
        return;
    }
    if s.gesture
        .as_ref()
        .is_some_and(|gesture| gesture.path != path)
    {
        return;
    }
    let starting = s.gesture.is_none();
    if starting
        && !(pressed
            && response.is_pointer_button_down_on()
            && response.rect.contains(pointer)
            && image.contains(pointer))
    {
        return;
    }
    let uv = (pointer - image.min) / image.size();
    let gesture = Some(Gesture {
        path: path.to_owned(),
        focus_uv: [uv.x.clamp(0., 1.), uv.y.clamp(0., 1.)],
        pointer,
    });
    if s.gesture != gesture {
        s.gesture = gesture;
        store(ctx, s);
        ctx.request_repaint();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub lens: egui::Rect,
    pub focus_native: [f32; 2],
    /// UI points per native image pixel. At 100%, each native pixel occupies one physical pixel.
    pub points_per_native_pixel: f32,
    /// Coverage needed by the lens itself, before the request's reuse margin.
    pub visible: [u32; 4],
    /// Native image bounds with 32 pixels of overscan for compatible detail reuse.
    pub region: [u32; 4],
}

/// Clamp source focus and screen placement independently. Moving a lens inside the viewport
/// must not move the photographic point being inspected.
pub fn geometry(
    pointer: egui::Pos2,
    focus_native: [f32; 2],
    window: [u32; 4],
    options: Options,
    pixels_per_point: f32,
    viewport: egui::Rect,
) -> Option<Geometry> {
    if !pointer.is_finite()
        || focus_native.iter().any(|v| !v.is_finite())
        || !pixels_per_point.is_finite()
        || pixels_per_point <= 0.
        || window[2] <= window[0]
        || window[3] <= window[1]
        || !viewport.is_positive()
        || !viewport.min.is_finite()
        || !viewport.max.is_finite()
    {
        return None;
    }
    let diameter = options
        .size
        .diameter()
        .min(viewport.width())
        .min(viewport.height());
    let radius = diameter * 0.5;
    let offset = if options.centered {
        egui::Vec2::ZERO
    } else {
        egui::vec2(radius + 16., -radius - 16.)
    };
    let center = pointer + offset;
    let center = egui::pos2(
        center
            .x
            .clamp(viewport.left() + radius, viewport.right() - radius),
        center
            .y
            .clamp(viewport.top() + radius, viewport.bottom() - radius),
    );
    let focus = [
        focus_native[0].clamp(window[0] as f32, window[2] as f32),
        focus_native[1].clamp(window[1] as f32, window[3] as f32),
    ];
    let points_per_native_pixel = options.zoom.multiple() / pixels_per_point;
    let bounds = |native_radius: f32| {
        [
            (focus[0] - native_radius).floor().max(window[0] as f32) as u32,
            (focus[1] - native_radius).floor().max(window[1] as f32) as u32,
            (focus[0] + native_radius).ceil().min(window[2] as f32) as u32,
            (focus[1] + native_radius).ceil().min(window[3] as f32) as u32,
        ]
    };
    Some(Geometry {
        lens: egui::Rect::from_center_size(center, egui::Vec2::splat(diameter)),
        focus_native: focus,
        points_per_native_pixel,
        visible: bounds(radius / points_per_native_pixel),
        region: bounds(radius / points_per_native_pixel + 32.),
    })
}

pub fn button(ui: &mut egui::Ui, options: &mut Options) {
    let (button, menu) =
        widgets::split_icon_button(ui, "Loupe", active(ui.ctx()), ControlSize::Large, |ui| {
            ui.label("Loupe size");
            for size in Size::ALL {
                if widgets::menu_row(ui, size.label(), options.size == size).clicked() {
                    options.size = size;
                    ui.close_kind(egui::UiKind::Menu);
                }
            }
            ui.separator();
            ui.label("Magnification");
            for zoom in Zoom::ALL {
                if widgets::menu_row(ui, zoom.label(), options.zoom == zoom).clicked() {
                    options.zoom = zoom;
                    ui.close_kind(egui::UiKind::Menu);
                }
            }
            ui.separator();
            if widgets::menu_row(ui, "Use centered loupe", options.centered).clicked() {
                options.centered = !options.centered;
                ui.close_kind(egui::UiKind::Menu);
            }
        });
    let rect = widgets::icon_rect(ui, &button);
    let color = widgets::visuals(ui, &button, active(ui.ctx())).text_color();
    let stroke = egui::Stroke::new(1.4_f32, color);
    let center = rect.center() - rect.size() * 0.1;
    ui.painter()
        .circle_stroke(center, rect.width() * 0.29, stroke);
    ui.painter().line_segment(
        [center + rect.size() * 0.21, rect.max - rect.size() * 0.07],
        stroke,
    );
    if button
        .on_hover_text("Loupe (P) · Hold to inspect native detail, release to hide")
        .clicked()
    {
        if active(ui.ctx()) {
            photo_tools::pointer(ui.ctx());
        } else {
            activate(ui.ctx());
        }
    }
    let mut s = state(ui.ctx());
    s.menu = menu.inner.is_some();
    store(ui.ctx(), s);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_pixel_scale_and_overscan_follow_zoom_and_display_density() {
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 800.));
        for pixels_per_point in [1., 2.] {
            for zoom in Zoom::ALL {
                let g = geometry(
                    egui::pos2(400., 400.),
                    [1200., 900.],
                    [100, 50, 2500, 1800],
                    Options {
                        zoom,
                        centered: true,
                        ..Default::default()
                    },
                    pixels_per_point,
                    viewport,
                )
                .unwrap();
                assert_eq!(
                    g.points_per_native_pixel * pixels_per_point,
                    zoom.multiple()
                );
                let radius = 120. / g.points_per_native_pixel + 32.;
                assert_eq!(
                    g.region,
                    [
                        (1200. - radius).floor() as u32,
                        (900. - radius).floor().max(50.) as u32,
                        (1200. + radius).ceil() as u32,
                        (900. + radius).ceil().min(1800.) as u32
                    ]
                );
            }
        }
    }

    #[test]
    fn screen_clamping_never_moves_focus_and_small_viewports_keep_a_circle() {
        for size in Size::ALL {
            for centered in [false, true] {
                let viewport =
                    egui::Rect::from_min_max(egui::pos2(25., 40.), egui::pos2(525., 340.));
                for pointer in [viewport.min, viewport.max, viewport.center()] {
                    let g = geometry(
                        pointer,
                        [-15., 2000.],
                        [100, 50, 1100, 850],
                        Options {
                            size,
                            centered,
                            ..Default::default()
                        },
                        2.,
                        viewport,
                    )
                    .unwrap();
                    assert!(viewport.contains_rect(g.lens));
                    assert_eq!(g.focus_native, [100., 850.]);
                    assert_eq!(g.lens.width(), g.lens.height());
                    assert!(g.region[0] >= 100 && g.region[1] >= 50);
                    assert!(g.region[2] <= 1100 && g.region[3] <= 850);
                }
            }
        }
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(90., 60.));
        let g = geometry(
            egui::pos2(2., 2.),
            [20., 20.],
            [0, 0, 40, 40],
            Options::default(),
            1.,
            viewport,
        )
        .unwrap();
        assert_eq!(g.lens.size(), egui::Vec2::splat(60.));
        assert_eq!(g.region, [0, 0, 40, 40]);
        assert!(geometry(
            egui::Pos2::ZERO,
            [20., 20.],
            [0, 0, 40, 40],
            Options::default(),
            0.,
            viewport
        )
        .is_none());
    }

    fn draw(ctx: &egui::Context, time: f64, events: Vec<egui::Event>) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 300.),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    for (index, path) in [Path::new("a.x3f"), Path::new("b.x3f")]
                        .into_iter()
                        .enumerate()
                    {
                        let rect = egui::Rect::from_min_size(
                            egui::pos2(10. + index as f32 * 290., 10.),
                            egui::vec2(280., 280.),
                        );
                        let response =
                            ui.interact(rect, egui::Id::new(path), egui::Sense::click_and_drag());
                        update(ctx, &response, path, rect.shrink(10.));
                    }
                });
            },
        );
    }
    fn button(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }
    fn space(pressed: bool) -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn held_loupe_stays_with_its_source_and_release_hides_it() {
        let ctx = egui::Context::default();
        activate(&ctx);
        draw(&ctx, 0., vec![]);
        let start = egui::pos2(440., 150.);
        draw(&ctx, 0.1, button(start, true));
        let g = gesture(&ctx).unwrap();
        assert_eq!(g.path, Path::new("b.x3f"));
        assert_eq!(g.focus_uv, [0.5, 0.5]);
        let end = egui::pos2(100., 100.);
        draw(&ctx, 0.2, vec![egui::Event::PointerMoved(end)]);
        let g = gesture(&ctx).unwrap();
        assert_eq!(g.path, Path::new("b.x3f"));
        assert_eq!(g.focus_uv[0], 0.);
        assert_eq!(g.pointer, end);
        draw(&ctx, 0.3, button(end, false));
        assert!(gesture(&ctx).is_none());
        assert!(active(&ctx));
    }

    #[test]
    fn letterbox_space_and_tool_cancellation_do_not_start_or_retain_a_lens() {
        let ctx = egui::Context::default();
        activate(&ctx);
        draw(&ctx, 0., vec![]);
        draw(&ctx, 0.1, button(egui::pos2(15., 15.), true));
        assert!(
            gesture(&ctx).is_none(),
            "letterboxing does not start a gesture"
        );
        draw(&ctx, 0.2, button(egui::pos2(150., 150.), false));
        draw(&ctx, 1., button(egui::pos2(150., 150.), true));
        assert!(gesture(&ctx).is_some());
        draw(&ctx, 1.1, vec![space(true)]);
        assert!(gesture(&ctx).is_none());
        draw(&ctx, 1.2, vec![space(false)]);
        assert!(
            gesture(&ctx).is_none(),
            "holding does not restart after Space"
        );
        draw(&ctx, 1.3, button(egui::pos2(150., 150.), false));
        draw(&ctx, 2., button(egui::pos2(150., 150.), true));
        assert!(gesture(&ctx).is_some());
        photo_tools::pointer(&ctx);
        assert!(gesture(&ctx).is_none());
    }
}
