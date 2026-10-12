//! Exercises organizer selection, stable focus, and virtualized scrolling with real egui input.

use super::*;
use crate::preferences::Organizer;

struct Fixture {
    ctx: egui::Context,
    browser: Browser,
    selected: Option<PathBuf>,
    mode: Organizer,
    width: f32,
    text: String,
}
struct Frame {
    focus: egui::Id,
    grid: egui::Rect,
    outside: egui::Rect,
    field: egui::Rect,
    offset: f32,
}
impl Fixture {
    fn new(mode: Organizer) -> Self {
        let ctx = egui::Context::default();
        let mut browser = Browser::new(ctx.clone());
        browser.files = (0..60)
            .map(|i| PathBuf::from(format!("image-{i:02}.x3f")))
            .collect();
        // These fixtures test interaction without asynchronous file reads.
        browser.cache = browser.files.iter().map(|p| (p.clone(), None)).collect();
        Self {
            ctx,
            browser,
            selected: None,
            mode,
            width: 350.,
            text: String::new(),
        }
    }
    fn frame(&mut self, events: Vec<egui::Event>) -> Frame {
        let mut result = None;
        let _ = self.ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900., 400.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                let mut grid = egui::Rect::NOTHING;
                let mut focus = egui::Id::NULL;
                let mut offset = 0.;
                egui::SidePanel::left("test organizer")
                    .exact_width(self.width)
                    .show(ctx, |ui| {
                        focus = ui.make_persistent_id("organizer focus");
                        grid = ui.available_rect_before_wrap();
                        if let Some(path) =
                            self.browser
                                .organizer(ui, self.selected.as_deref(), self.mode)
                        {
                            self.selected = Some(path);
                        }
                        offset = egui::scroll_area::State::load(
                            ctx,
                            ui.make_persistent_id(egui::Id::new("organizer items")),
                        )
                        .unwrap_or_default()
                        .offset
                        .y;
                    });
                egui::CentralPanel::default().show(ctx, |ui| {
                    let outside = ui.label("Click here without focusing another control").rect;
                    let field = ui.text_edit_singleline(&mut self.text).rect;
                    result = Some(Frame {
                        focus,
                        grid,
                        outside,
                        field,
                        offset,
                    });
                });
            },
        );
        result.unwrap()
    }
    fn click(&mut self, pos: egui::Pos2) -> Frame {
        for pressed in [true, false] {
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
        self.frame(vec![])
    }
    fn key(&mut self, key: egui::Key, repeat: bool) -> Frame {
        self.frame(vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers: Default::default(),
        }])
    }
    fn index(&self) -> Option<usize> {
        self.selected
            .as_ref()
            .and_then(|p| self.browser.files.iter().position(|file| file == p))
    }
    fn focused(&self, frame: &Frame) -> bool {
        self.ctx.memory(|memory| memory.has_focus(frame.focus))
    }
    fn select_first(&mut self) -> Frame {
        self.frame(vec![]);
        let frame = self.frame(vec![]);
        self.click(frame.grid.min + egui::vec2(30., 20.))
    }
}

#[test]
fn arrow_geometry_handles_edges_partial_rows_and_empty_folders() {
    use egui::Key::*;
    for (current, count, columns, key, expected) in [
        (None, 0, 3, ArrowRight, None),
        (None, 8, 3, ArrowDown, Some(0)),
        (Some(0), 8, 3, ArrowLeft, Some(0)),
        (Some(0), 8, 3, ArrowUp, Some(0)),
        (Some(2), 8, 3, ArrowUp, Some(2)),
        (Some(6), 8, 3, ArrowDown, Some(6)),
        (Some(2), 8, 3, ArrowRight, Some(3)),
        (Some(5), 8, 3, ArrowDown, Some(7)),
        (Some(7), 8, 3, ArrowDown, Some(7)),
        (Some(7), 8, 3, ArrowRight, Some(7)),
        (Some(7), 8, 3, ArrowUp, Some(4)),
    ] {
        assert_eq!(arrow_selection(current, count, columns, key), expected);
    }
}

#[test]
fn clicking_an_image_gives_the_whole_organizer_persistent_arrow_focus() {
    for mode in [Organizer::Icons, Organizer::List] {
        let mut fixture = Fixture::new(mode);
        fixture.key(egui::Key::ArrowDown, false);
        assert_eq!(fixture.index(), None);
        let frame = fixture.select_first();
        assert_eq!(fixture.index(), Some(0));
        assert!(fixture.focused(&frame));
        let stride = if mode == Organizer::Icons { 2 } else { 1 };
        for repeat in [false, true, true] {
            let before = fixture.index().unwrap();
            let frame = fixture.key(egui::Key::ArrowDown, repeat);
            assert_eq!(fixture.index(), Some(before + stride));
            assert!(fixture.focused(&frame));
        }
        fixture.key(egui::Key::ArrowRight, false);
        assert_eq!(fixture.index(), Some(3 * stride + 1));
        fixture.key(egui::Key::ArrowLeft, false);
        let frame = fixture.key(egui::Key::ArrowUp, false);
        assert_eq!(fixture.index(), Some(2 * stride));
        assert!(fixture.focused(&frame));
        let frame = fixture.frame(vec![]); // asynchronous repaints must not steal focus
        assert!(fixture.focused(&frame));
    }
}

#[test]
fn first_arrow_after_click_does_not_transfer_focus_to_tools() {
    let mut fixture = Fixture::new(Organizer::Icons);
    fixture.frame(vec![]);
    let frame = fixture.frame(vec![]);
    let pos = frame.grid.min + egui::vec2(30., 20.);
    for pressed in [true, false] {
        fixture.frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            },
        ]);
    }
    let frame = fixture.key(egui::Key::ArrowRight, false);
    assert_eq!(fixture.index(), Some(1));
    assert!(fixture.focused(&frame));
}

#[test]
fn keyboard_selection_scrolls_beyond_virtualized_rows_and_back() {
    for mode in [Organizer::Icons, Organizer::List] {
        let mut fixture = Fixture::new(mode);
        fixture.select_first();
        for _ in 0..60 {
            let frame = fixture.key(egui::Key::ArrowDown, true);
            let row = fixture.index().unwrap() / if mode == Organizer::Icons { 2 } else { 1 };
            let height = if mode == Organizer::Icons { 146. } else { 40. };
            let stride = height + fixture.ctx.style().spacing.item_spacing.y;
            let top = row as f32 * stride;
            assert!(top >= frame.offset - 1.);
            assert!(
                top + height <= frame.offset + frame.grid.height() + 1.,
                "mode={mode:?} row={row} top={top} height={height} offset={} viewport={}",
                frame.offset,
                frame.grid.height()
            );
            assert!(fixture.focused(&frame));
        }
        assert_eq!(
            fixture.index(),
            Some(if mode == Organizer::Icons { 58 } else { 59 })
        );
        for _ in 0..60 {
            fixture.key(egui::Key::ArrowUp, true);
        }
        assert_eq!(fixture.index(), Some(0));
        assert_eq!(fixture.frame(vec![]).offset, 0.);
    }
}

#[test]
fn grid_arrows_follow_reflowed_columns_and_list_mode() {
    let mut fixture = Fixture::new(Organizer::Icons);
    fixture.select_first();
    fixture.key(egui::Key::ArrowDown, false);
    assert_eq!(fixture.index(), Some(2));
    fixture.width = 520.;
    fixture.frame(vec![]);
    fixture.frame(vec![]);
    fixture.key(egui::Key::ArrowDown, false);
    assert_eq!(fixture.index(), Some(5));
    fixture.mode = Organizer::List;
    fixture.key(egui::Key::ArrowDown, false);
    assert_eq!(fixture.index(), Some(6));
}

#[test]
fn clicking_elsewhere_or_tabbing_releases_organizer_focus() {
    for mode in [Organizer::Icons, Organizer::List] {
        let mut fixture = Fixture::new(mode);
        let frame = fixture.select_first();
        let frame = fixture.click(frame.outside.center());
        assert!(!fixture.focused(&frame));
        fixture.key(egui::Key::ArrowDown, false);
        assert_eq!(fixture.index(), Some(0));
        let frame = fixture.select_first();
        fixture.click(frame.field.center());
        fixture.key(egui::Key::ArrowDown, false);
        assert_eq!(fixture.index(), Some(0));
        fixture.select_first();
        let frame = fixture.key(egui::Key::Tab, false);
        assert!(!fixture.focused(&frame));
        fixture.key(egui::Key::ArrowRight, false);
        assert_eq!(fixture.index(), Some(0));
    }
}
