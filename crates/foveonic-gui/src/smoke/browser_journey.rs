//! Exercises browser commands and search against independently declared staged paths.

use super::{journey::ExposureJourney, App, Smoke};
use crate::{browser::Mark, shortcuts::BrowserAction};
use eframe::egui;
use serde_json::{json, Value};
use std::path::PathBuf;

#[derive(Clone, Copy)]
enum Phase {
    Start,
    Marked,
    Limited,
    Searched,
    Cleared,
    Sorted,
    RestoredOrder,
    Navigated,
    RestoredSelection,
    Done,
}

pub(super) struct BrowserJourney {
    pub(super) model: ExposureJourney,
    phase: Phase,
    expected: Vec<PathBuf>,
    observations: Vec<Value>,
}

impl BrowserJourney {
    pub(super) fn new(exposures: Vec<f32>, paths: Vec<PathBuf>) -> Self {
        Self {
            model: ExposureJourney::new(exposures),
            phase: Phase::Start,
            expected: paths,
            observations: Vec::new(),
        }
    }

    pub(super) fn advance(
        &mut self,
        app: &mut App,
        ctx: &egui::Context,
        smoke: &mut Smoke,
    ) -> Result<bool, String> {
        let paths = &self.expected;
        match self.phase {
            Phase::Start => {
                if paths.len() < 2 || app.edit_selection.paths() != paths {
                    return Err("browser journey needs all declared photographs selected".into());
                }
                if self
                    .model
                    .exposures()
                    .iter()
                    .any(|v| !v.is_finite() || *v + 0.125 > 4.)
                {
                    return Err("browser journey needs manual exposures accepting +0.125".into());
                }
                if visible(app) != *paths {
                    return Err("initial browser order differs from declared staged paths".into());
                }
                if paths.iter().any(|p| {
                    app.browser.mark(p) != Mark::Unmarked || app.browser.rating(p).stars() != 0
                }) {
                    return Err("browser fixture must begin unmarked and unrated".into());
                }
                app.browser.hide_reviewed = false;
                self.model.adjust_exposure(0.125);
                super::scenarios::apply_exposure(app, 0.125);
                app.browser.set_actions(vec![
                    (BrowserAction::Rate(3), egui::Modifiers::NONE),
                    (BrowserAction::TogglePick, egui::Modifiers::NONE),
                ]);
                self.phase = Phase::Marked;
            }
            Phase::Marked => {
                if app.view_busy()
                    || paths.iter().any(|p| {
                        app.browser.rating(p).stars() != 3 || app.browser.mark(p) != Mark::Pick
                    })
                {
                    return Ok(false);
                }
                self.observations
                    .push(json!({"step":"rate-and-pick", "paths":paths, "rating":3}));
                app.preferences.multi_edit_limit = 2;
                self.phase = Phase::Limited;
            }
            Phase::Limited => {
                if app.edit_selection.paths() != &paths[..2] {
                    return Ok(false);
                }
                if app.browser.processing_selection(app.selected.as_deref()) != *paths {
                    return Err("viewport limit changed browser selection".into());
                }
                self.observations.push(
                    json!({"step":"selection-limit", "browser":paths, "viewport":&paths[..2]}),
                );
                let name = paths[0]
                    .file_name()
                    .ok_or("source filename missing")?
                    .to_string_lossy()
                    .into_owned();
                ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("browser search")));
                smoke.direct_events.push_back(vec![egui::Event::Text(name)]);
                self.phase = Phase::Searched;
            }
            Phase::Searched => {
                if !smoke.direct_events.is_empty() || visible(app) != paths[..1] {
                    return Ok(false);
                }
                self.observations
                    .push(json!({"step":"search", "visible":&paths[..1]}));
                let key = |key, modifiers| egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                };
                ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("browser search")));
                smoke.direct_events.push_back(vec![
                    key(egui::Key::A, egui::Modifiers::COMMAND),
                    key(egui::Key::Backspace, egui::Modifiers::NONE),
                ]);
                self.phase = Phase::Cleared;
            }
            Phase::Cleared => {
                if !smoke.direct_events.is_empty() || visible(app) != *paths {
                    return Ok(false);
                }
                self.observations
                    .push(json!({"step":"clear-search", "visible":paths}));
                app.preferences.browser_sort = crate::browser::Sort {
                    field: crate::browser::Field::Name,
                    reverse: true,
                };
                self.phase = Phase::Sorted;
            }
            Phase::Sorted => {
                let reverse: Vec<_> = paths.iter().rev().cloned().collect();
                if visible(app) != reverse {
                    return Ok(false);
                }
                self.observations
                    .push(json!({"step":"reverse-name", "visible":reverse}));
                app.preferences.browser_sort = crate::browser::Sort::default();
                self.phase = Phase::RestoredOrder;
            }
            Phase::RestoredOrder => {
                if visible(app) != *paths {
                    return Ok(false);
                }
                app.preferences.organizer = crate::preferences::Organizer::List;
                app.browser.set_actions(vec![(
                    BrowserAction::Arrow(egui::Key::ArrowDown),
                    egui::Modifiers::NONE,
                )]);
                self.phase = Phase::Navigated;
            }
            Phase::Navigated => {
                if app.selected.as_ref() != Some(&paths[1]) {
                    return Ok(false);
                }
                self.observations
                    .push(json!({"step":"navigate-down", "selected":paths[1]}));
                app.preferences.multi_edit_limit = paths.len();
                app.browser
                    .set_actions(vec![(BrowserAction::SelectAll, egui::Modifiers::COMMAND)]);
                self.phase = Phase::RestoredSelection;
            }
            Phase::RestoredSelection => {
                if app.edit_selection.paths() != paths || app.view_busy() {
                    return Ok(false);
                }
                let mut actual = Vec::new();
                for path in paths {
                    let settings = app
                        .multi_edit
                        .settings(path)
                        .ok_or("missing retained settings")?;
                    actual.push(settings.exposure);
                }
                self.model.check_exposures(&actual)?;
                if paths.iter().any(|p| {
                    app.browser.rating(p).stars() != 3 || app.browser.mark(p) != Mark::Pick
                }) {
                    return Err("ratings or picks did not survive filtering and sorting".into());
                }
                self.observations
                    .push(json!({"step":"restored", "exposures":actual}));
                self.phase = Phase::Done;
                return Ok(true);
            }
            Phase::Done => return Ok(true),
        }
        ctx.request_repaint();
        Ok(false)
    }
    pub(super) fn evidence(&self) -> Value {
        json!({"expected":self.model, "observations":self.observations})
    }
}

fn visible(app: &App) -> Vec<PathBuf> {
    app.browser
        .files
        .iter()
        .filter(|path| !app.browser.excluded.contains(*path))
        .cloned()
        .collect()
}
