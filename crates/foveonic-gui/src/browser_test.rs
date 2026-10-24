//! Exercises workspace shortcuts, text focus, organizer selection, and virtualized scrolling with real egui input.

use super::*;
use crate::preferences::Organizer;

struct Fixture {
    ctx: egui::Context,
    browser: Browser,
    selected: Option<PathBuf>,
    mode: Organizer,
    width: f32,
    text: String,
    control_checked: bool,
    control_actions: usize,
    number: f64,
}
struct ControlFrame {
    button: egui::Response,
    checkbox: egui::Response,
    slider: egui::Response,
    number: egui::Response,
}
struct Frame {
    focus: egui::Id,
    grid: egui::Rect,
    outside: egui::Rect,
    field: egui::Rect,
    offset: f32,
    markers: Vec<egui::Pos2>,
    eye: Option<egui::Rect>,
    controls: ControlFrame,
    labels: Vec<String>,
}
impl Fixture {
    fn new(mode: Organizer) -> Self {
        let ctx = egui::Context::default();
        crate::widgets::install_style(&ctx);
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
            control_checked: false,
            control_actions: 0,
            number: 0.5,
        }
    }
    fn frame(&mut self, events: Vec<egui::Event>) -> Frame {
        let mut result = None;
        let modifiers = events
            .iter()
            .rev()
            .find_map(|event| match event {
                egui::Event::Key { modifiers, .. }
                | egui::Event::PointerButton { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .unwrap_or_default();
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900., 400.),
            )),
            modifiers,
            events,
            ..Default::default()
        };
        self.browser.capture_shortcuts(&self.ctx, &mut input, true);
        let output = self.ctx.run(input, |ctx| {
            let mut grid = egui::Rect::NOTHING;
            let mut focus = egui::Id::NULL;
            let mut offset = 0.;
            let mut eye = None;
            let mut controls = None;
            egui::TopBottomPanel::top("test toolbar").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let button = ui.button("Action");
                    if button.clicked() {
                        self.control_actions += 1;
                    }
                    let checkbox = ui.checkbox(&mut self.control_checked, "Option");
                    let slider =
                        ui.add(egui::Slider::new(&mut self.number, 0.0..=1.0).show_value(false));
                    let number = ui.add(egui::DragValue::new(&mut self.number).speed(0.1));
                    controls = Some(ControlFrame {
                        button,
                        checkbox,
                        slider,
                        number,
                    });
                });
            });
            egui::SidePanel::left("test organizer")
                .exact_width(self.width)
                .show(ctx, |ui| {
                    focus = ui.make_persistent_id("organizer focus");
                    if self.browser.import_mode {
                        eye = Some(self.browser.visibility_button(ui, focus).rect);
                    }
                    grid = ui.available_rect_before_wrap();
                    if let Some(path) =
                        self.browser
                            .organizer(ui, self.selected.as_deref(), self.mode)
                    {
                        self.selected = Some(path);
                    }
                    if self.browser.clear_selection {
                        self.selected = None;
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
                    markers: Vec::new(),
                    eye,
                    controls: controls.take().unwrap(),
                    labels: Vec::new(),
                });
            });
        });
        let mut result = result.unwrap();
        result.labels = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
                _ => None,
            })
            .collect();
        result.markers = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if (rect.rect.width() - rect.rect.height()).abs() < 0.01
                        && rect.rect.width() < 20.
                        && rect.stroke.width > 0.
                        && result.grid.contains(rect.rect.center()) =>
                {
                    Some(rect.rect.center())
                }
                _ => None,
            })
            .collect();
        result
    }
    fn click(&mut self, pos: egui::Pos2) -> Frame {
        self.modified_click(pos, egui::Modifiers::NONE)
    }
    fn modified_click(&mut self, pos: egui::Pos2, modifiers: egui::Modifiers) -> Frame {
        for pressed in [true, false] {
            self.frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers,
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
        let files = (0..count)
            .map(|i| PathBuf::from(format!("{i}.x3f")))
            .collect::<Vec<_>>();
        let layout = folders::Layout::new(&files, None, columns, GRID_TILE_HEIGHT, 20., 6.);
        assert_eq!(layout.arrow(current, key), expected);
    }
}

#[test]
fn review_shortcuts_require_import_mode_and_respect_text_focus_and_arrows_skip_hidden_files() {
    for mode in [Organizer::Icons, Organizer::List] {
        let mut fixture = Fixture::new(mode);
        fixture.select_first();
        let first = fixture.selected.clone().unwrap();
        fixture.key(egui::Key::Space, false);
        assert_eq!(fixture.browser.mark(&first), Mark::Unmarked);
        fixture.browser.import_mode = true;
        fixture.key(egui::Key::Space, false);
        assert_eq!(fixture.browser.mark(&first), Mark::Pick);
        let frame = fixture.frame(vec![]);
        fixture.click(frame.field.center());
        fixture.key(egui::Key::Space, false);
        fixture.key(egui::Key::Delete, false);
        assert_eq!(fixture.browser.mark(&first), Mark::Pick);
        fixture
            .browser
            .recorded_marks
            .insert(fixture.browser.files[1].clone(), Mark::Imported);
        fixture.browser.refresh_filter();
        let frame = fixture.frame(vec![]);
        fixture.click(frame.grid.min + egui::vec2(30., 20.));
        fixture.key(egui::Key::ArrowRight, false);
        assert_eq!(fixture.index(), Some(2));
    }
}

#[test]
fn select_all_and_space_toggle_only_visible_eligible_photos_without_changing_the_viewed_photo() {
    for mode in [Organizer::Icons, Organizer::List] {
        let mut fixture = Fixture::new(mode);
        fixture.browser.import_mode = true;
        let imported = fixture.browser.files[1].clone();
        fixture
            .browser
            .recorded_marks
            .insert(imported.clone(), Mark::Imported);
        fixture.browser.refresh_filter();
        fixture.select_first();
        let active = fixture.selected.clone();
        fixture.frame(vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }]);
        assert_eq!(fixture.browser.highlighted.len(), 59);
        for check in [true, false] {
            fixture.key(egui::Key::Space, false);
            assert_eq!(
                fixture
                    .browser
                    .files
                    .iter()
                    .filter(|path| fixture.browser.mark(path) == Mark::Pick)
                    .count(),
                if check { 59 } else { 0 }
            );
            assert_eq!(fixture.selected, active);
            assert_eq!(fixture.browser.mark(&imported), Mark::Imported);
        }
        fixture
            .browser
            .marks
            .insert(fixture.browser.files[0].clone(), Mark::Pick);
        fixture.key(egui::Key::Space, false);
        assert_eq!(
            fixture
                .browser
                .files
                .iter()
                .filter(|path| fixture.browser.mark(path) == Mark::Pick)
                .count(),
            59
        );
    }
}

#[test]
fn checkbox_and_eye_clicks_do_not_select_or_reopen_a_photo_and_delete_unchecks_and_advances() {
    for mode in [Organizer::Icons, Organizer::List] {
        let mut fixture = Fixture::new(mode);
        fixture.browser.import_mode = true;
        let frame = fixture.select_first();
        let first = fixture.selected.clone().unwrap();
        let checkbox = frame.markers[0];
        fixture.click(checkbox);
        assert_eq!(fixture.browser.mark(&first), Mark::Pick);
        assert_eq!(fixture.index(), Some(0));
        fixture.browser.hide_reviewed = false;
        fixture.key(egui::Key::Backspace, false);
        assert_eq!(fixture.browser.mark(&first), Mark::Hidden);
        assert!(fixture.browser.hide_reviewed);
        assert_eq!(fixture.index(), Some(1));
        assert_eq!(fixture.browser.files.len(), 60);
        let frame = fixture.frame(vec![]);
        let frame = fixture.click(frame.eye.unwrap().center());
        assert!(!fixture.browser.hide_reviewed);
        assert!(fixture.focused(&frame));
        let active = fixture.selected.clone();
        // The eye occupies the same caption position as the original checkbox.
        fixture.click(checkbox);
        assert_eq!(fixture.browser.mark(&first), Mark::Unmarked);
        assert_eq!(fixture.selected, active);
        fixture
            .browser
            .recorded_marks
            .insert(first.clone(), Mark::Imported);
        fixture.browser.marks.remove(&first);
        fixture.frame(vec![]);
        fixture.click(checkbox);
        assert_eq!(fixture.browser.mark(&first), Mark::Unmarked);
        assert_eq!(fixture.browser.recorded_marks[&first], Mark::Imported);
    }
}

#[test]
fn command_and_shift_click_highlight_ranges_without_checking_them() {
    for mode in [Organizer::Icons, Organizer::List] {
        let mut fixture = Fixture::new(mode);
        fixture.browser.import_mode = true;
        let frame = fixture.select_first();
        let second = if mode == Organizer::Icons {
            frame.grid.min + egui::vec2(frame.grid.width() / 2. + 30., 20.)
        } else {
            frame.grid.min + egui::vec2(30., 66.)
        };
        fixture.modified_click(second, egui::Modifiers::COMMAND);
        assert_eq!(fixture.browser.highlighted.len(), 2);
        assert_eq!(fixture.index(), Some(1));
        let frame = fixture.frame(vec![]);
        fixture.modified_click(
            frame.grid.min + egui::vec2(30., 20.),
            egui::Modifiers::SHIFT,
        );
        assert_eq!(fixture.browser.highlighted.len(), 2);
        assert!(fixture.browser.marks.is_empty());
    }
}

#[test]
fn hiding_every_visible_photo_clears_the_active_selection() {
    let mut fixture = Fixture::new(Organizer::Icons);
    fixture.browser.import_mode = true;
    fixture.browser.files.truncate(1);
    fixture.select_first();
    fixture.key(egui::Key::Delete, false);
    assert!(fixture.selected.is_none());
    assert!(fixture.browser.clear_selection);
}

#[test]
fn background_import_history_changes_do_not_move_the_viewed_photo() {
    let mut fixture = Fixture::new(Organizer::Icons);
    fixture.browser.import_mode = true;
    fixture.select_first();
    let active = fixture.selected.clone().unwrap();
    fixture
        .browser
        .recorded_marks
        .insert(active.clone(), Mark::Imported);
    fixture.browser.refresh_filter();
    fixture.frame(vec![]);
    assert!(fixture.browser.excluded.contains(&active));
    assert_eq!(fixture.selected.as_ref(), Some(&active));
    assert!(!fixture.browser.clear_selection);
}

#[test]
fn clicking_an_image_gives_the_whole_organizer_persistent_arrow_focus() {
    for mode in [Organizer::Icons, Organizer::List] {
        let mut fixture = Fixture::new(mode);
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
fn workspace_shortcuts_survive_outside_clicks_but_respect_text_entry() {
    for mode in [Organizer::Icons, Organizer::List] {
        let mut fixture = Fixture::new(mode);
        let frame = fixture.select_first();
        let frame = fixture.click(frame.outside.center());
        assert!(!fixture.focused(&frame));
        fixture.key(egui::Key::ArrowRight, false);
        assert_eq!(fixture.index(), Some(1));
        let frame = fixture.select_first();
        fixture.click(frame.field.center());
        fixture.key(egui::Key::ArrowDown, false);
        assert_eq!(fixture.index(), Some(0));
        fixture.select_first();
        let frame = fixture.key(egui::Key::Tab, false);
        assert!(!fixture.focused(&frame));
        fixture.key(egui::Key::ArrowRight, false);
        // Leaving browser focus alone does not suspend workspace commands.
        assert_eq!(fixture.index(), Some(1));
    }
}

#[test]
fn workspace_shortcuts_preempt_buttons_checkboxes_and_sliders_before_the_browser_draws() {
    for mode in [Organizer::Icons, Organizer::List] {
        for control in 0..3 {
            let mut fixture = Fixture::new(mode);
            fixture.browser.import_mode = true;
            let frame = fixture.select_first();
            let response = match control {
                0 => frame.controls.button,
                1 => frame.controls.checkbox,
                _ => frame.controls.slider,
            };
            fixture.click(response.rect.center());
            fixture
                .ctx
                .memory_mut(|memory| memory.request_focus(response.id));
            fixture.frame(vec![]);
            let before = (
                fixture.control_checked,
                fixture.control_actions,
                fixture.number,
            );
            fixture.key(egui::Key::Space, false);
            assert_eq!(fixture.browser.mark(&fixture.browser.files[0]), Mark::Pick);
            fixture.key(egui::Key::ArrowRight, false);
            assert_eq!(fixture.index(), Some(1));
            assert_eq!(
                (
                    fixture.control_checked,
                    fixture.control_actions,
                    fixture.number
                ),
                before
            );
            fixture
                .ctx
                .memory_mut(|memory| memory.request_focus(response.id));
            fixture.frame(vec![egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            }]);
            assert_eq!(fixture.browser.highlighted.len(), 60);
            fixture.key(egui::Key::Space, false);
            assert_eq!(fixture.browser.review_counts(), (60, 0));
            fixture.key(egui::Key::Delete, false);
            assert_eq!(fixture.browser.review_counts(), (0, 60));
            assert_eq!(fixture.selected, None);
            assert_eq!(
                (
                    fixture.control_checked,
                    fixture.control_actions,
                    fixture.number
                ),
                before
            );
        }
    }
}

#[test]
fn numeric_editors_keep_nudges_and_text_shortcuts_and_resume_review_on_enter() {
    for mode in [Organizer::Icons, Organizer::List] {
        let mut fixture = Fixture::new(mode);
        fixture.browser.import_mode = true;
        let frame = fixture.select_first();
        let first = fixture.selected.clone().unwrap();
        fixture.click(frame.controls.number.rect.center());
        let before = fixture.number;
        fixture.key(egui::Key::ArrowUp, false);
        assert!(fixture.number > before);
        assert_eq!(fixture.index(), Some(0));
        fixture.key(egui::Key::Space, false);
        fixture.key(egui::Key::Delete, false);
        assert_eq!(fixture.browser.mark(&first), Mark::Unmarked);
        fixture.frame(vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }]);
        assert_eq!(fixture.browser.highlighted.len(), 1);
        fixture.frame(vec![egui::Event::Text("0.75".into())]);
        fixture.key(egui::Key::Enter, false);
        assert_eq!(fixture.number, 0.75);
        fixture.key(egui::Key::Space, false);
        assert_eq!(fixture.browser.mark(&first), Mark::Pick);
    }
}

#[test]
fn cleared_browser_highlights_fall_back_to_the_current_photo_for_space() {
    let mut fixture = Fixture::new(Organizer::Icons);
    fixture.browser.import_mode = true;
    let frame = fixture.select_first();
    fixture.modified_click(
        frame.grid.min + egui::vec2(30., 20.),
        egui::Modifiers::COMMAND,
    );
    assert!(fixture.browser.highlighted.is_empty());
    assert_eq!(fixture.index(), Some(0));
    fixture.key(egui::Key::Space, false);
    assert_eq!(fixture.browser.mark(&fixture.browser.files[0]), Mark::Pick);
}

#[test]
fn shift_arrow_extends_the_photo_range_after_clicking_the_viewport() {
    for mode in [Organizer::Icons, Organizer::List] {
        let mut fixture = Fixture::new(mode);
        fixture.browser.import_mode = true;
        let frame = fixture.select_first();
        fixture.click(frame.outside.center());
        fixture.frame(vec![egui::Event::Key {
            key: egui::Key::ArrowRight,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::SHIFT,
        }]);
        assert_eq!(fixture.index(), Some(1));
        assert_eq!(fixture.browser.highlighted.len(), 2);
        fixture.key(egui::Key::Space, false);
        assert_eq!(fixture.browser.review_counts(), (2, 0));
    }
}

#[test]
fn open_menus_keep_their_keys_and_workspace_shortcuts_resume_when_closed() {
    let mut fixture = Fixture::new(Organizer::Icons);
    fixture.browser.import_mode = true;
    fixture.select_first();
    fixture
        .ctx
        .memory_mut(|memory| memory.open_popup(egui::Id::new("test menu")));
    fixture.key(egui::Key::Space, false);
    fixture.key(egui::Key::ArrowRight, false);
    fixture.key(egui::Key::Delete, false);
    assert_eq!(fixture.index(), Some(0));
    assert_eq!(fixture.browser.review_counts(), (0, 0));
    fixture.ctx.memory_mut(|memory| memory.close_popup());
    fixture.key(egui::Key::Space, false);
    assert_eq!(fixture.browser.review_counts(), (1, 0));
}

#[test]
fn folder_groups_keep_one_selection_and_keyboard_range_across_directory_boundaries() {
    for mode in [Organizer::Icons, Organizer::List] {
        let mut fixture = Fixture::new(mode);
        fixture.browser.import_mode = true;
        fixture.browser.root = Some("card".into());
        fixture.browser.files = [
            "card/a.x3f",
            "card/DCIM/100SIGMA/a.x3f",
            "card/DCIM/100SIGMA/b.x3f",
            "card/DCIM/101SIGMA/a.x3f",
        ]
        .map(PathBuf::from)
        .to_vec();
        fixture.browser.cache = fixture
            .browser
            .files
            .iter()
            .map(|path| (path.clone(), None))
            .collect();
        fixture.select_first();
        let frame = fixture.key(egui::Key::ArrowDown, false);
        assert_eq!(fixture.index(), Some(1));
        assert!(frame.labels.iter().any(|label| label == "DCIM/100SIGMA"));
        fixture.key(egui::Key::ArrowRight, false);
        let frame = fixture.key(egui::Key::ArrowDown, false);
        assert_eq!(fixture.index(), Some(3));
        assert!(frame.labels.iter().any(|label| label == "DCIM/101SIGMA"));
        fixture.frame(vec![egui::Event::Key {
            key: egui::Key::ArrowLeft,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::SHIFT,
        }]);
        fixture.key(egui::Key::Space, false);
        assert_eq!(fixture.browser.review_counts(), (2, 0));
        fixture.key(egui::Key::Delete, false);
        assert_eq!(fixture.browser.review_counts(), (0, 2));
        assert_eq!(fixture.index(), Some(1));
        let frame = fixture.frame(vec![]);
        assert!(!frame.labels.iter().any(|label| label == "DCIM/101SIGMA"));
    }
}
