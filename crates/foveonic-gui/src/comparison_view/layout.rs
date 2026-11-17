//! Packs photograph viewports into bounded aspect-aware rows or columns and defers reflow during gestures.

use eframe::egui;
use std::path::PathBuf;

pub(super) const GAP: f32 = 8.;
pub(super) const MIN_TILE: egui::Vec2 = egui::vec2(120., 112.);
pub(super) const CAPTION: f32 = 22.;
const PAD: f32 = 6.;
const FOOT: f32 = CAPTION + 3.;

pub(super) fn pointer_gesture(
    ctx: &egui::Context,
    widgets: impl IntoIterator<Item = egui::Id>,
) -> bool {
    widgets.into_iter().any(|id| {
        // egui resolves ownership before drawing, using the previous pass's
        // widgets. A pane divider can overlap a photo without owning its input.
        ctx.read_response(id).is_some_and(|response| {
            response.is_pointer_button_down_on() || response.clicked() || response.drag_stopped()
        })
    })
}

#[derive(Clone, Debug)]
pub(super) struct Tile {
    pub index: usize,
    pub rect: egui::Rect,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Layout {
    pub tiles: Vec<Tile>,
    pub shown: usize,
    pub overflow: usize,
}

#[derive(Default)]
pub(crate) struct State {
    paths: Vec<PathBuf>,
    aspects: Vec<f32>,
    count: usize,
    available: egui::Vec2,
    layout: Option<Layout>,
}

impl State {
    pub(super) fn shown(&self) -> usize {
        self.layout.as_ref().map_or(0, |layout| layout.shown)
    }

    pub(super) fn get(
        &mut self,
        paths: &[PathBuf],
        aspects: &[f32],
        count: usize,
        available: egui::Vec2,
        freeze: bool,
    ) -> &Layout {
        let same_photos =
            paths.get(..self.paths.len()) == Some(self.paths.as_slice()) && self.count == count;
        if self.layout.is_none()
            || !same_photos
            || (!freeze && (self.available != available || self.aspects != aspects))
        {
            self.layout = Some(layout(count, available, aspects));
            self.paths = paths[..capacity(count, available).0].to_vec();
            self.aspects = aspects.to_vec();
            self.count = count;
            self.available = available;
        }
        self.layout.as_ref().unwrap()
    }
}

pub(super) fn capacity(count: usize, available: egui::Vec2) -> (usize, usize) {
    let available = available.max(egui::Vec2::splat(1.));
    let columns = (((available.x + GAP) / (MIN_TILE.x + GAP)).floor() as usize).max(1);
    let rows = (((available.y + GAP) / (MIN_TILE.y + GAP)).floor() as usize).max(1);
    let slots = count.min(columns.saturating_mul(rows));
    let overflow = if count > slots { count - slots + 1 } else { 0 };
    (slots.saturating_sub(usize::from(overflow > 0)), overflow)
}

fn layout(count: usize, available: egui::Vec2, aspects: &[f32]) -> Layout {
    let available = available.max(egui::Vec2::splat(1.));
    let (shown, overflow) = capacity(count, available);
    let mut aspects: Vec<_> = aspects.iter().take(shown).copied().collect();
    aspects.resize(shown, 1.5);
    for aspect in &mut aspects {
        if !aspect.is_finite() || *aspect <= 0. {
            *aspect = 1.5;
        }
    }
    if overflow > 0 {
        aspects.push(1.5);
    }
    let mut best = grid(&aspects, available, shown);
    let mut score = area(&best, &aspects, shown);
    let original: Vec<_> = (0..aspects.len()).collect();
    let mut sorted = original.clone();
    sorted.sort_by(|a, b| aspects[*a].total_cmp(&aspects[*b]));
    // Only two stable orders and two shelf directions are considered. Search
    // cost is bounded by the readable tile capacity, never browser folder size.
    for order in [&original, &sorted] {
        for columns in [false, true] {
            for groups in 1..=aspects.len() {
                let candidate = shelves(&aspects, order, available, groups, columns);
                if candidate.iter().any(|tile| {
                    tile.rect.width() + 0.01 < MIN_TILE.x.min(available.x)
                        || tile.rect.height() + 0.01 < MIN_TILE.y.min(available.y)
                }) {
                    continue;
                }
                let next = area(&candidate, &aspects, shown);
                if next > score + 0.01 {
                    best = candidate;
                    score = next;
                }
            }
        }
    }
    best.sort_by(|a, b| {
        a.rect
            .top()
            .total_cmp(&b.rect.top())
            .then(a.rect.left().total_cmp(&b.rect.left()))
            .then(a.index.cmp(&b.index))
    });
    Layout {
        tiles: best,
        shown,
        overflow,
    }
}

fn area(tiles: &[Tile], aspects: &[f32], shown: usize) -> f32 {
    tiles
        .iter()
        .filter(|tile| tile.index < shown)
        .map(|tile| {
            let size = (tile.rect.size() - egui::vec2(PAD, FOOT)).max(egui::Vec2::ZERO);
            let height = size.y.min(size.x / aspects[tile.index]);
            height * height * aspects[tile.index]
        })
        .sum()
}

fn grid(aspects: &[f32], available: egui::Vec2, shown: usize) -> Vec<Tile> {
    let mut best = Vec::new();
    let mut score = -1.;
    for columns in 1..=aspects.len() {
        let rows = aspects.len().div_ceil(columns);
        let size = (available - egui::vec2(GAP * (columns - 1) as f32, GAP * (rows - 1) as f32))
            / egui::vec2(columns as f32, rows as f32);
        if size.x <= 0. || size.y <= 0. {
            continue;
        }
        if size.x + 0.01 < MIN_TILE.x.min(available.x)
            || size.y + 0.01 < MIN_TILE.y.min(available.y)
        {
            continue;
        }
        let tiles: Vec<_> = (0..aspects.len())
            .map(|index| Tile {
                index,
                rect: egui::Rect::from_min_size(
                    egui::pos2(
                        (index % columns) as f32 * (size.x + GAP),
                        (index / columns) as f32 * (size.y + GAP),
                    ),
                    size,
                ),
            })
            .collect();
        let next = area(&tiles, aspects, shown);
        if next > score {
            best = tiles;
            score = next;
        }
    }
    best
}

// Positive shelf lengths allow greedy feasibility checks. Fixed bisection
// bounds work independently of photo count and avoid searching permutations.
fn partition(weights: &[f32], groups: usize) -> Vec<std::ops::Range<usize>> {
    let mut low = weights.iter().copied().fold(0., f32::max);
    let mut high: f32 = weights.iter().sum();
    for _ in 0..24 {
        let limit = (low + high) * 0.5;
        let mut used = 1;
        let mut sum = 0.;
        for weight in weights {
            if sum + weight > limit {
                used += 1;
                sum = 0.;
            }
            sum += weight;
        }
        if used <= groups {
            high = limit;
        } else {
            low = limit;
        }
    }
    let mut ranges = Vec::with_capacity(groups);
    let mut start = 0;
    let mut sum = 0.;
    for (index, weight) in weights.iter().enumerate() {
        let remaining_groups = groups - ranges.len();
        if index > start && (sum + weight > high || weights.len() - index < remaining_groups) {
            ranges.push(start..index);
            start = index;
            sum = 0.;
        }
        sum += weight;
    }
    ranges.push(start..weights.len());
    ranges
}

fn shelves(
    aspects: &[f32],
    order: &[usize],
    available: egui::Vec2,
    groups: usize,
    columns: bool,
) -> Vec<Tile> {
    let mut span = if columns {
        (available.x - GAP * (groups - 1) as f32) / groups as f32 - PAD
    } else {
        (available.y - GAP * (groups - 1) as f32) / groups as f32 - FOOT
    };
    if span <= 0. {
        return Vec::new();
    }
    let ratio = |index: usize| {
        if columns {
            1. / aspects[index]
        } else {
            aspects[index]
        }
    };
    let overhead = if columns { FOOT } else { PAD };
    let weights: Vec<_> = order
        .iter()
        .map(|index| span * ratio(*index) + overhead + GAP)
        .collect();
    let ranges = partition(&weights, groups);
    for range in &ranges {
        let total: f32 = order[range.clone()].iter().map(|index| ratio(*index)).sum();
        let length = if columns { available.y } else { available.x };
        span = span
            .min((length - overhead * range.len() as f32 - GAP * (range.len() - 1) as f32) / total);
    }
    if span <= 0. {
        return Vec::new();
    }
    let across =
        groups as f32 * (span + if columns { PAD } else { FOOT }) + GAP * (groups - 1) as f32;
    let mut tiles = Vec::with_capacity(order.len());
    for (group, range) in ranges.iter().enumerate() {
        let length: f32 = order[range.clone()]
            .iter()
            .map(|index| span * ratio(*index) + overhead)
            .sum::<f32>()
            + GAP * (range.len() - 1) as f32;
        let mut along = ((if columns { available.y } else { available.x }) - length) * 0.5;
        let across_start = ((if columns { available.x } else { available.y }) - across) * 0.5
            + group as f32 * (span + if columns { PAD } else { FOOT } + GAP);
        for index in &order[range.clone()] {
            let size = if columns {
                egui::vec2(span + PAD, span / aspects[*index] + FOOT)
            } else {
                egui::vec2(span * aspects[*index] + PAD, span + FOOT)
            };
            let min = if columns {
                egui::pos2(across_start, along)
            } else {
                egui::pos2(along, across_start)
            };
            tiles.push(Tile {
                index: *index,
                rect: egui::Rect::from_min_size(min, size),
            });
            along += if columns { size.y + GAP } else { size.x + GAP };
        }
    }
    tiles
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Workspace {
        ctx: egui::Context,
        state: State,
        aspects: [f32; 2],
        height: f32,
        empty_photos: bool,
    }

    struct Frame {
        left: egui::Rect,
        right: egui::Rect,
        canvas: egui::Rect,
        tiles: Vec<egui::Rect>,
        frozen: bool,
        pointer_down: bool,
    }

    impl Workspace {
        fn new() -> Self {
            Self {
                ctx: egui::Context::default(),
                state: State::default(),
                aspects: [1.5; 2],
                height: 700.,
                empty_photos: false,
            }
        }

        fn frame(&mut self, events: Vec<egui::Event>) -> Frame {
            let mut result = None;
            let paths = [PathBuf::from("a.x3f"), PathBuf::from("b.x3f")];
            let _ = self.ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400., self.height),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    let left = egui::SidePanel::left("organizer")
                        .default_width(230.)
                        .width_range(100.0..=400.0)
                        .resizable(true)
                        .show(ctx, |ui| ui.allocate_space(ui.available_size()))
                        .response
                        .rect;
                    let right = egui::SidePanel::right("settings")
                        .default_width(230.)
                        .width_range(100.0..=400.0)
                        .resizable(true)
                        .show(ctx, |ui| ui.allocate_space(ui.available_size()))
                        .response
                        .rect;
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let frozen = pointer_gesture(
                                ctx,
                                (0usize..2).flat_map(|index| {
                                    [
                                        if self.empty_photos {
                                            crate::viewport::Viewport::for_photo(&paths[index]).id()
                                        } else {
                                            egui::Id::new(index)
                                        },
                                        egui::Id::new(("caption", index)),
                                    ]
                                }),
                            );
                            let canvas = ui.available_rect_before_wrap();
                            let plan =
                                self.state
                                    .get(&paths, &self.aspects, 2, canvas.size(), frozen);
                            let tiles: Vec<_> = plan.tiles.iter().map(|tile| tile.rect).collect();
                            for tile in &plan.tiles {
                                let image = egui::Rect::from_min_max(
                                    tile.rect.min + canvas.min.to_vec2() + egui::vec2(3., 3.),
                                    tile.rect.max + canvas.min.to_vec2() - egui::vec2(3., CAPTION),
                                );
                                if self.empty_photos {
                                    let mut child =
                                        ui.new_child(egui::UiBuilder::new().max_rect(image));
                                    crate::viewport::Viewport::for_photo(&paths[tile.index])
                                        .show_empty(&mut child);
                                } else {
                                    ui.interact(
                                        image,
                                        egui::Id::new(tile.index),
                                        egui::Sense::click_and_drag(),
                                    );
                                }
                                ui.interact(
                                    egui::Rect::from_min_max(
                                        egui::pos2(tile.rect.left(), tile.rect.bottom() - CAPTION)
                                            + canvas.min.to_vec2(),
                                        tile.rect.max + canvas.min.to_vec2(),
                                    ),
                                    egui::Id::new(("caption", tile.index)),
                                    egui::Sense::click_and_drag(),
                                );
                            }
                            result = Some(Frame {
                                left,
                                right,
                                canvas,
                                tiles,
                                frozen,
                                pointer_down: ctx.input(|input| input.pointer.any_down()),
                            });
                        });
                },
            );
            result.unwrap()
        }
    }

    fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }

    #[test]
    fn both_pane_dividers_reflow_tiles_while_the_pointer_is_held() {
        for left in [true, false] {
            let mut workspace = Workspace::new();
            workspace.frame(vec![]);
            let initial = workspace.frame(vec![]);
            // The divider grab area extends into the canvas. Ownership,
            // rather than the press coordinates alone, must choose the gesture.
            let start = egui::pos2(
                if left {
                    initial.left.right() + 1.
                } else {
                    initial.right.left() - 1.
                },
                initial.canvas.center().y,
            );
            workspace.frame(vec![egui::Event::PointerMoved(start), button(start, true)]);
            let moved = start + egui::vec2(if left { 80. } else { -80. }, 0.);
            let dragged = workspace.frame(vec![egui::Event::PointerMoved(moved)]);
            assert!(dragged.pointer_down);
            assert!(
                dragged.canvas.width() < initial.canvas.width() - 50.,
                "left={left}, start={start:?}, before={:?}, after={:?}, frozen={}",
                initial.canvas,
                dragged.canvas,
                dragged.frozen
            );
            assert!(!dragged.frozen, "left={left}");
            assert_ne!(dragged.tiles, initial.tiles);
            if left {
                assert!(dragged.canvas.left() > initial.canvas.left());
            } else {
                assert_eq!(dragged.canvas.left(), initial.canvas.left());
            }
            let released = workspace.frame(vec![button(moved, false)]);
            assert!(!released.frozen);
        }
    }

    #[test]
    fn photo_click_and_drag_hold_layout_through_release_then_reflow() {
        for drag in [false, true] {
            let mut workspace = Workspace::new();
            workspace.frame(vec![]);
            let initial = workspace.frame(vec![]);
            let start = initial.tiles[0].center() + initial.canvas.min.to_vec2();
            let pressed =
                workspace.frame(vec![egui::Event::PointerMoved(start), button(start, true)]);
            assert!(pressed.frozen);
            workspace.aspects = [2. / 3., 1.5];
            workspace.height -= 100.;
            let end = if drag {
                start + egui::vec2(40., 20.)
            } else {
                start
            };
            let held = workspace.frame(vec![egui::Event::PointerMoved(end)]);
            assert!(held.frozen);
            assert_eq!(held.tiles, initial.tiles);
            let released = workspace.frame(vec![button(end, false)]);
            assert!(released.frozen);
            assert_eq!(released.tiles, initial.tiles);
            let settled = workspace.frame(vec![]);
            assert!(!settled.frozen);
            assert_ne!(settled.tiles, initial.tiles);
        }
    }

    #[test]
    fn cancelled_caption_click_holds_layout_through_release() {
        let mut workspace = Workspace::new();
        workspace.frame(vec![]);
        let initial = workspace.frame(vec![]);
        let tile = initial.tiles[0];
        let start = egui::pos2(tile.center().x, tile.bottom() - CAPTION * 0.5)
            + initial.canvas.min.to_vec2();
        let pressed = workspace.frame(vec![egui::Event::PointerMoved(start), button(start, true)]);
        assert!(pressed.frozen);
        workspace.aspects = [2. / 3., 1.5];
        let outside = start - egui::vec2(0., CAPTION * 2.);
        let moved = workspace.frame(vec![egui::Event::PointerMoved(outside)]);
        assert!(moved.frozen);
        assert_eq!(moved.tiles, initial.tiles);
        let released = workspace.frame(vec![button(outside, false)]);
        let caption = workspace
            .ctx
            .read_response(egui::Id::new(("caption", 0usize)))
            .unwrap();
        assert!(!caption.clicked());
        assert!(caption.drag_stopped());
        assert!(released.frozen);
        assert_eq!(released.tiles, initial.tiles);
        let settled = workspace.frame(vec![]);
        assert!(!settled.frozen);
        assert_ne!(settled.tiles, initial.tiles);
    }

    #[test]
    fn empty_photo_holds_layout_through_cancelled_drag_release() {
        let mut workspace = Workspace::new();
        workspace.empty_photos = true;
        workspace.frame(vec![]);
        let initial = workspace.frame(vec![]);
        let start = initial.tiles[0].center() + initial.canvas.min.to_vec2();
        let pressed = workspace.frame(vec![egui::Event::PointerMoved(start), button(start, true)]);
        assert!(pressed.frozen);
        workspace.aspects = [2. / 3., 1.5];
        let moved = start + egui::vec2(40., 20.);
        let held = workspace.frame(vec![egui::Event::PointerMoved(moved)]);
        assert!(held.frozen);
        assert_eq!(held.tiles, initial.tiles);
        let released = workspace.frame(vec![button(moved, false)]);
        assert!(released.frozen);
        assert_eq!(released.tiles, initial.tiles);
        let settled = workspace.frame(vec![]);
        assert!(!settled.frozen);
        assert_ne!(settled.tiles, initial.tiles);
    }

    fn check(plan: &Layout, count: usize, available: egui::Vec2) {
        assert_eq!(plan.shown + plan.overflow, count);
        let mut indices: Vec<_> = plan.tiles.iter().map(|tile| tile.index).collect();
        indices.sort_unstable();
        assert_eq!(
            indices,
            (0..plan.shown + usize::from(plan.overflow > 0)).collect::<Vec<_>>()
        );
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, available);
        for (index, tile) in plan.tiles.iter().enumerate() {
            assert!(tile.rect.is_finite());
            assert!(
                canvas.expand(0.01).contains_rect(tile.rect),
                "{available:?}: {tile:?}"
            );
            assert!(tile.rect.width() + 0.01 >= MIN_TILE.x.min(available.x));
            assert!(tile.rect.height() + 0.01 >= MIN_TILE.y.min(available.y));
            for other in &plan.tiles[index + 1..] {
                assert!(!tile.rect.shrink(0.01).intersects(other.rect.shrink(0.01)));
            }
        }
    }

    #[test]
    fn mixed_orientations_gain_photo_area_and_can_reorder() {
        let available = egui::vec2(1200., 800.);
        let aspects = [1.5, 2. / 3., 1.5];
        let plan = layout(3, available, &aspects);
        check(&plan, 3, available);
        let equal = grid(&aspects, available, 3);
        assert!(area(&plan.tiles, &aspects, 3) > area(&equal, &aspects, 3) * 1.2);
        assert_ne!(
            plan.tiles.iter().map(|tile| tile.index).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        // Two photos can use the viewport rather than stop at the old preferred boxes.
        let pair = layout(2, available, &[1.5, 1.5]);
        assert!(area(&pair.tiles, &[1.5, 1.5], 2) > 2. * 320. * 260. * 2.);
    }

    #[test]
    fn packing_keeps_every_identity_readable_and_inside_the_viewport() {
        for available in [
            egui::vec2(800., 600.),
            egui::vec2(1200., 800.),
            egui::vec2(400., 1000.),
            egui::vec2(1., 1.),
            egui::vec2(100., 90.),
            egui::vec2(1280., 120.),
        ] {
            for count in [0, 1, 2, 3, 8, 12, 100_000, usize::MAX] {
                let (shown, _) = capacity(count, available);
                let aspects: Vec<_> = (0..shown)
                    .map(|index| [1.5, 2. / 3., 1., 12., 0.1][index % 5])
                    .collect();
                check(&layout(count, available, &aspects), count, available);
            }
        }
    }

    #[test]
    fn unknown_and_extreme_aspects_keep_the_readable_grid_fallback() {
        let available = egui::vec2(800., 600.);
        for aspects in [[f32::NAN, 0., -1.], [1000., 0.001, 1.5]] {
            check(&layout(3, available, &aspects), 3, available);
        }
    }

    #[test]
    fn cache_defers_aspect_and_size_changes_through_the_release_frame() {
        let mut state = State::default();
        let paths = vec![
            PathBuf::from("a.x3f"),
            PathBuf::from("b.x3f"),
            PathBuf::from("c.x3f"),
        ];
        let available = egui::vec2(1200., 800.);
        let original = state.get(&paths, &[1.5; 3], 3, available, false).clone();
        for _ in 0..3 {
            // press, drag and release all freeze the same rectangles
            let held = state.get(
                &paths,
                &[1.5, 2. / 3., 1.5],
                3,
                egui::vec2(1000., 700.),
                true,
            );
            assert_eq!(
                held.tiles
                    .iter()
                    .map(|tile| (tile.index, tile.rect))
                    .collect::<Vec<_>>(),
                original
                    .tiles
                    .iter()
                    .map(|tile| (tile.index, tile.rect))
                    .collect::<Vec<_>>()
            );
        }
        state.get(
            &paths,
            &[1.5, 2. / 3., 1.5],
            3,
            egui::vec2(1000., 700.),
            false,
        );
        assert_eq!(state.available, egui::vec2(1000., 700.));
        assert_eq!(state.aspects, vec![1.5, 2. / 3., 1.5]);
        let many: Vec<_> = (0..12)
            .map(|index| PathBuf::from(format!("photo-{index}.x3f")))
            .collect();
        state.get(&many, &[1.5; 12], 12, available, false);
        // A resize can lower readable capacity while the pointer remains held.
        assert_eq!(
            state
                .get(&many, &[], 12, egui::vec2(200., 150.), true)
                .shown,
            12
        );
        let resized = state.get(&many, &[], 12, egui::vec2(200., 150.), false);
        assert_eq!(resized.shown, 0);
        assert_eq!(resized.overflow, 12);
        let replaced = vec![PathBuf::from("d.x3f"), paths[1].clone(), paths[2].clone()];
        state.get(&replaced, &[1.5; 3], 3, available, true);
        assert_eq!(state.paths, replaced);
    }
}
