//! Checks that a pick on one editing photo samples that photo and edits every editing photo.

use super::*;
use crate::{
    multi_edit_runtime::{Change, Runtime},
    rendering::worker::ColorPickReply,
    sidecars,
};
use foveonic::render::{ColorRangeSample, WhiteBalanceBias};
use std::sync::Arc;

const TILE: f32 = 200.;
const NEUTRAL: [f32; 4] = [1.1, 1., 0.7, 1.];

/// The editing photos, held by the editing store as in the app.
struct Photos {
    runtime: Runtime,
    sidecars: sidecars::Session,
    editing: Vec<PathBuf>,
}
impl PickPhotos for Photos {
    fn settings(&self, path: &Path) -> Option<&Settings> {
        self.runtime.settings(path)
    }
    fn editing(&self) -> &[PathBuf] {
        &self.editing
    }
    fn editable(&self) -> bool {
        self.runtime.editable_ready()
    }
    fn edit(&mut self, edits: Vec<Edit>) -> bool {
        self.runtime
            .edit(&self.editing, &Change::Fields(edits), &mut self.sidecars)
    }
}

/// Photos drawn side by side as viewport tiles and picked through `pick_on_photo`.
/// The last photo is drawn but outside the editing selection.
struct Tiles {
    ctx: egui::Context,
    photos: Photos,
    outside: PathBuf,
    /// Photos no longer drawn.
    hidden: Vec<PathBuf>,
    /// Photos whose drawn image does not show their current recipe.
    stale: Vec<PathBuf>,
    time: f64,
    cursor: egui::CursorIcon,
    requests: Vec<ColorPickRequest>,
}

impl Tiles {
    fn new(editing: Vec<Settings>) -> Self {
        let ctx = egui::Context::default();
        let mut sidecars = sidecars::Session::new(
            ctx.clone(),
            false,
            Arc::new(foveonic::decode_cache::Cache::new()),
        );
        sidecars.set_memory_only(true);
        let paths: Vec<_> = (0..editing.len())
            .map(|index| PathBuf::from(format!("{index}.x3f")))
            .collect();
        // The photo outside the editing selection is loaded too, as a displayed photo is.
        let outside = PathBuf::from("outside.x3f");
        let runtime = Runtime::rendered(
            paths
                .iter()
                .cloned()
                .zip(editing)
                .chain([(outside.clone(), Settings::default())])
                .map(|(path, settings)| (path, settings.clone(), settings, None)),
        );
        let mut tiles = Self {
            ctx,
            photos: Photos {
                runtime,
                sidecars,
                editing: paths,
            },
            outside,
            hidden: Vec::new(),
            stale: Vec::new(),
            time: 0.,
            cursor: egui::CursorIcon::Default,
            requests: Vec::new(),
        };
        tiles.frame(vec![]);
        tiles
    }

    fn paths(&self) -> Vec<PathBuf> {
        let mut paths = self.photos.editing.clone();
        paths.push(self.outside.clone());
        paths
    }

    fn path(&self, index: usize) -> PathBuf {
        self.photos.editing[index].clone()
    }

    fn rect(index: usize) -> egui::Rect {
        egui::Rect::from_min_size(egui::pos2(index as f32 * TILE, 0.), egui::vec2(TILE, TILE))
    }

    fn center(&self, path: &Path) -> egui::Pos2 {
        Self::rect(self.paths().iter().position(|p| p == path).unwrap()).center()
    }

    fn settings(&self, index: usize) -> &Settings {
        self.photos
            .runtime
            .settings(&self.photos.editing[index])
            .unwrap()
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        self.time += 0.1;
        let paths = self.paths();
        let ctx = self.ctx.clone();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(paths.len() as f32 * TILE, TILE),
                )),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let pointer = ctx.input(|i| i.pointer.interact_pos());
                    let tiles: Vec<_> = paths
                        .iter()
                        .enumerate()
                        .filter(|(_, path)| !self.hidden.contains(path))
                        .map(|(index, path)| {
                            let rect = Self::rect(index);
                            let uv = pointer.filter(|pos| rect.contains(*pos)).map(|pos| {
                                let uv = (pos - rect.min) / TILE;
                                [uv.x, uv.y]
                            });
                            PickTile {
                                path: path.clone(),
                                response: ui.interact(
                                    rect,
                                    egui::Id::new(path),
                                    egui::Sense::click_and_drag(),
                                ),
                                uv: uv.filter(|_| !self.stale.contains(path)),
                                allowed: true,
                            }
                        })
                        .collect();
                    let (_, request) = pick_on_photo(ui, &tiles, &mut self.photos);
                    self.requests.extend(request);
                });
            },
        );
        self.cursor = output.platform_output.cursor_icon;
    }

    fn press(&mut self, pos: egui::Pos2, pressed: bool) {
        self.frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            },
        ]);
    }

    fn hover(&mut self, pos: egui::Pos2) {
        self.frame(vec![egui::Event::PointerMoved(pos)]);
    }

    fn click(&mut self, pos: egui::Pos2) {
        let before = self.requests.len();
        self.hover(pos);
        self.press(pos, true);
        self.press(pos, false);
        assert_eq!(self.requests.len(), before + 1, "the click picks");
    }

    /// The worker's reply to the latest pick.
    fn reply(&self, sample: ColorRangeSample) -> ColorPickReply {
        let request = self.requests.last().unwrap();
        ColorPickReply {
            tool: request.tool,
            token: request.token,
            path: request.path.clone(),
            sample: Ok(sample),
        }
    }
}

fn neutral() -> ColorRangeSample {
    ColorRangeSample {
        weights: [0.; 8],
        color: NEUTRAL,
    }
}

#[test]
fn a_pending_pick_survives_frames_with_the_pointer_over_another_photo() {
    let mut tiles = Tiles::new(vec![Settings::default(); 2]);
    editor::white_balance::toggle(&tiles.ctx);
    let first = tiles.path(0);
    let second = tiles.path(1);
    tiles.click(tiles.center(&first));
    assert_eq!(tiles.requests[0].path, first);
    tiles.hover(tiles.center(&second));
    tiles.hover(tiles.center(&tiles.outside.clone()));
    assert!(editor::white_balance::pending(&tiles.ctx));
    editor::white_balance::receive(&tiles.ctx, &tiles.reply(neutral()));
    tiles.hover(tiles.center(&second));
    assert!(!editor::white_balance::pending(&tiles.ctx));
    assert!(tiles.settings(0).white_balance_bias.neutral.is_some());
    assert_eq!(
        tiles.settings(0).white_balance_bias,
        tiles.settings(1).white_balance_bias
    );
}

#[test]
fn a_white_balance_pick_reaches_every_photo_when_the_sampled_one_has_that_bias() {
    let bias = WhiteBalanceBias::from_sample([NEUTRAL[0], NEUTRAL[1], NEUTRAL[2]]).unwrap();
    let sampled = Settings {
        white_balance_bias: bias,
        ..Default::default()
    };
    let mut tiles = Tiles::new(vec![sampled, Settings::default()]);
    editor::white_balance::toggle(&tiles.ctx);
    let second = tiles.path(1);
    tiles.hover(tiles.center(&second));
    assert_eq!(tiles.cursor, egui::CursorIcon::Crosshair);
    tiles.click(tiles.center(&tiles.path(0)));
    assert_eq!(selected(&tiles.ctx), Tool::Pointer);
    editor::white_balance::receive(&tiles.ctx, &tiles.reply(neutral()));
    tiles.frame(vec![]);
    assert_eq!(tiles.settings(0).white_balance_bias, bias);
    assert_eq!(tiles.settings(1).white_balance_bias, bias);
}

#[test]
fn a_dehaze_pick_sets_every_photos_shadow_and_leaves_auto() {
    let mut manual = Settings::default();
    manual.dehaze.shadow = Some([0.5; 3]);
    let mut tiles = Tiles::new(vec![Settings::default(), manual]);
    editor::dehaze::toggle(&tiles.ctx);
    tiles.click(tiles.center(&tiles.path(1)));
    let reply = tiles.reply(ColorRangeSample {
        weights: [0.; 8],
        color: [0.1, 0.2, 0.3, 1.],
    });
    editor::dehaze::receive(&tiles.ctx, &reply);
    tiles.frame(vec![]);
    for index in 0..2 {
        assert_eq!(tiles.settings(index).dehaze.shadow, Some([0.1, 0.2, 0.3]));
    }
}

#[test]
fn a_basic_drag_offsets_each_photo_and_clamps_each_without_cancelling_itself() {
    let mut near_limit = Settings::default();
    near_limit.photographic.color_editor.bands[0].hue = 0.9;
    near_limit.photographic.color_editor.bands[2].lightness = -0.4;
    let mut tiles = Tiles::new(vec![Settings::default(), near_limit]);
    // Hue follows horizontal travel at the default gesture settings.
    editor::direct_color::toggle(&tiles.ctx);
    let anchor = tiles.center(&tiles.path(0));
    tiles.hover(anchor);
    tiles.press(anchor, true);
    let reply = tiles.reply([1., 0., 0., 0., 0., 0., 0., 0.].into());
    editor::direct_color::receive(&tiles.ctx, reply);
    let hue = |tiles: &Tiles, index| tiles.settings(index).photographic.color_editor.bands[0].hue;
    tiles.hover(anchor + egui::vec2(60., 0.));
    let first = hue(&tiles, 0);
    assert!(first > 0., "the drag moves the sampled photo");
    assert_eq!(hue(&tiles, 1), (0.9 + first).min(1.));
    // Each later frame continues the same drag from the store's recipe.
    tiles.hover(anchor + egui::vec2(120., 0.));
    assert!(editor::direct_color::dragging(&tiles.ctx));
    assert!((hue(&tiles, 0) - 2. * first).abs() < 1e-6);
    assert_eq!(hue(&tiles, 1), 1.);
    tiles.press(anchor + egui::vec2(120., 0.), false);
    assert!(!editor::direct_color::dragging(&tiles.ctx));
    assert_eq!(
        tiles.settings(1).photographic.color_editor.bands[2].lightness,
        -0.4
    );
}

#[test]
fn an_advanced_pick_with_different_range_lists_changes_nothing_and_reports_why() {
    let mut ranged = Settings::default();
    ranged
        .photographic
        .color_editor
        .advanced
        .push(foveonic::render::AdvancedColorRange {
            hue: 40.,
            ..Default::default()
        });
    let mut tiles = Tiles::new(vec![Settings::default(), ranged]);
    let before = [tiles.settings(0).clone(), tiles.settings(1).clone()];
    let mut log = crate::activity::Log::default();
    editor::direct_color::toggle_advanced(&tiles.ctx);
    let pos = tiles.center(&tiles.path(0));
    tiles.hover(pos);
    tiles.press(pos, true);
    tiles.press(pos, false);
    log.receive(&tiles.ctx);
    assert!(log.unread_errors());
    assert!(tiles.requests.is_empty());
    assert_eq!(
        [tiles.settings(0), tiles.settings(1)],
        [&before[0], &before[1]]
    );
}

#[test]
fn a_photo_outside_the_editing_selection_gets_no_pick() {
    let mut tiles = Tiles::new(vec![Settings::default(); 2]);
    editor::white_balance::toggle(&tiles.ctx);
    let outside = tiles.center(&tiles.outside.clone());
    tiles.hover(outside);
    assert_ne!(tiles.cursor, egui::CursorIcon::Crosshair);
    tiles.press(outside, true);
    tiles.press(outside, false);
    assert!(tiles.requests.is_empty());
    assert!(editor::white_balance::active(&tiles.ctx));
}

#[test]
fn a_photo_with_its_section_off_gets_no_pick_and_the_tool_stays_armed() {
    let off = Settings {
        corrections_enabled: false,
        ..Default::default()
    };
    let mut tiles = Tiles::new(vec![Settings::default(), off]);
    editor::dehaze::toggle(&tiles.ctx);
    let pos = tiles.center(&tiles.path(1));
    tiles.hover(pos);
    assert_ne!(tiles.cursor, egui::CursorIcon::Crosshair);
    tiles.press(pos, true);
    tiles.press(pos, false);
    assert!(tiles.requests.is_empty());
    assert!(editor::dehaze::active(&tiles.ctx));
    let pos = tiles.center(&tiles.path(0));
    tiles.hover(pos);
    assert_eq!(tiles.cursor, egui::CursorIcon::Crosshair);
}

#[test]
fn a_dehaze_press_on_a_stale_image_leaves_the_tool_armed() {
    let mut tiles = Tiles::new(vec![Settings::default(); 2]);
    tiles.stale.push(tiles.path(0));
    editor::dehaze::toggle(&tiles.ctx);
    let pos = tiles.center(&tiles.path(0));
    tiles.hover(pos);
    tiles.press(pos, true);
    tiles.press(pos, false);
    assert!(tiles.requests.is_empty());
    assert!(editor::dehaze::active(&tiles.ctx));
    tiles.stale.clear();
    tiles.click(pos);
    assert!(!editor::dehaze::active(&tiles.ctx));
}

#[test]
fn a_photo_leaving_mid_gesture_frees_the_tool_for_another_photo() {
    let mut tiles = Tiles::new(vec![Settings::default(); 2]);
    editor::direct_color::toggle(&tiles.ctx);
    let first = tiles.center(&tiles.path(0));
    tiles.hover(first);
    tiles.press(first, true);
    assert_eq!(tiles.requests.len(), 1);
    tiles.hidden.push(tiles.path(0));
    tiles.hover(first);
    assert!(!editor::direct_color::dragging(&tiles.ctx));
    tiles.press(first, false);
    assert!(editor::direct_color::active(&tiles.ctx));
    let second = tiles.center(&tiles.path(1));
    tiles.hover(second);
    tiles.press(second, true);
    assert_eq!(tiles.requests.len(), 2);
    assert_eq!(tiles.requests[1].path, tiles.path(1));
}

#[test]
fn a_pick_waits_until_every_editing_photo_is_rendered() {
    let mut tiles = Tiles::new(vec![Settings::default(); 2]);
    tiles.photos.runtime.unrendered(&tiles.path(1));
    editor::white_balance::toggle(&tiles.ctx);
    let pos = tiles.center(&tiles.path(0));
    tiles.hover(pos);
    assert_ne!(tiles.cursor, egui::CursorIcon::Crosshair);
    tiles.press(pos, true);
    tiles.press(pos, false);
    assert!(tiles.requests.is_empty());
    assert!(editor::white_balance::active(&tiles.ctx));
}

#[test]
fn a_monochrome_photo_gets_no_white_balance_pick() {
    let monochrome = Settings {
        rendering: foveonic::render::Rendering::Monochrome,
        ..Default::default()
    };
    let mut tiles = Tiles::new(vec![monochrome, Settings::default()]);
    editor::white_balance::toggle(&tiles.ctx);
    let pos = tiles.center(&tiles.path(0));
    tiles.hover(pos);
    tiles.press(pos, true);
    tiles.press(pos, false);
    assert!(tiles.requests.is_empty());
    tiles.click(tiles.center(&tiles.path(1)));
}

#[test]
fn a_pick_reaches_an_editing_photo_that_is_not_drawn() {
    let mut tiles = Tiles::new(vec![Settings::default(); 2]);
    tiles.hidden.push(tiles.path(1));
    editor::dehaze::toggle(&tiles.ctx);
    tiles.click(tiles.center(&tiles.path(0)));
    let reply = tiles.reply(ColorRangeSample {
        weights: [0.; 8],
        color: [0.1, 0.2, 0.3, 1.],
    });
    editor::dehaze::receive(&tiles.ctx, &reply);
    tiles.frame(vec![]);
    assert_eq!(tiles.settings(1).dehaze.shadow, Some([0.1, 0.2, 0.3]));
}

#[test]
fn an_advanced_pick_adds_its_range_to_every_photo_with_the_same_ranges() {
    let mut tiles = Tiles::new(vec![Settings::default(); 2]);
    editor::direct_color::toggle_advanced(&tiles.ctx);
    let pos = tiles.center(&tiles.path(0));
    tiles.hover(pos);
    tiles.press(pos, true);
    let mut reply = tiles.reply(ColorRangeSample {
        color: [240., 0.7, 1., 0.],
        weights: [1.; 8],
    });
    reply.tool = tiles.requests[0].tool;
    editor::direct_color::receive(&tiles.ctx, reply);
    tiles.hover(pos + egui::vec2(30., -30.));
    tiles.press(pos + egui::vec2(30., -30.), false);
    for index in 0..2 {
        let advanced = tiles.settings(index).photographic.color_editor.advanced;
        assert_eq!(advanced.count, 1, "photo {index}");
        assert_eq!(advanced.ranges[0].hue, 240.);
    }
    assert_eq!(
        tiles.settings(0).photographic.color_editor,
        tiles.settings(1).photographic.color_editor
    );
}
