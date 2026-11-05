//! Retains session operations, tool messages and errors with one acknowledged toolbar status.

use crate::widgets;
use eframe::egui;
use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    time::Instant,
};

const MAX_ENTRIES: usize = 1000;

#[derive(Clone)]
pub enum Event {
    Operation(String),
    Error(String),
}

/// Controls publish events without owning the application log or adding layout rows.
pub fn communicate(ctx: &egui::Context, event: Event) {
    ctx.data_mut(|data| {
        let queue =
            data.get_temp_mut_or_default::<VecDeque<Event>>(egui::Id::new("Activity messages"));
        if queue.len() == MAX_ENTRIES {
            queue.pop_front();
        }
        queue.push_back(event);
    });
    ctx.request_repaint();
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub enum Origin {
    Display,
    Edits,
    Processing,
}

struct Entry {
    seconds: u64,
    error: bool,
    message: String,
}

pub struct Log {
    started: Instant,
    entries: VecDeque<Entry>,
    observed: HashMap<Origin, BTreeSet<String>>,
    unread_errors: bool,
    open: bool,
    latest: Option<String>,
}
impl Default for Log {
    fn default() -> Self {
        Self {
            started: Instant::now(),
            entries: Default::default(),
            observed: Default::default(),
            unread_errors: false,
            open: false,
            latest: None,
        }
    }
}
impl Log {
    pub fn record(&mut self, event: Event) {
        match event {
            Event::Operation(message) => self.operation(message),
            Event::Error(message) => self.error(message),
        }
    }
    pub fn open(&mut self) {
        self.open = true;
        self.latest = None;
        self.unread_errors = false;
    }
    pub fn receive(&mut self, ctx: &egui::Context) {
        let messages = ctx
            .data_mut(|data| {
                data.remove_temp::<VecDeque<Event>>(egui::Id::new("Activity messages"))
            })
            .unwrap_or_default();
        for event in messages {
            self.record(event);
        }
    }
    pub fn status(&self, ui: &mut egui::Ui) {
        crate::toolbar::status(ui, self.latest.as_deref().unwrap_or_default());
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn unread_errors(&self) -> bool {
        self.unread_errors
    }
    fn push(&mut self, error: bool, message: String) {
        self.latest = Some(message.clone());
        if self.entries.len() == MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(Entry {
            seconds: self.started.elapsed().as_secs(),
            error,
            message,
        });
        self.unread_errors |= error;
    }
    pub fn operation(&mut self, message: impl Into<String>) {
        self.push(false, message.into());
    }
    pub fn error(&mut self, message: impl Into<String>) {
        self.push(true, message.into());
    }

    /// Persistent failures are recorded on arrival, then again only if they resolve and recur.
    pub fn observe(&mut self, origin: Origin, errors: impl IntoIterator<Item = String>) {
        let current: BTreeSet<_> = errors.into_iter().collect();
        let previous = self.observed.entry(origin).or_default();
        let arrived: Vec<_> = current.difference(previous).cloned().collect();
        *previous = current;
        for error in arrived {
            self.error(error);
        }
    }
    pub fn button(&mut self, ui: &mut egui::Ui) {
        if widgets::notice_button(ui, "Log", self.unread_errors)
            .on_hover_text(if self.unread_errors {
                "New errors. Open the session log."
            } else {
                "File operations, tool messages and errors for this session"
            })
            .clicked()
        {
            self.open();
        }
    }
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        self.unread_errors = false;
        self.latest = None;
        let response = egui::Modal::new(egui::Id::new("Activity log")).show(ctx, |ui| {
            ui.set_width((ctx.screen_rect().width() - 80.).clamp(240., 720.));
            ui.heading("Log");
            egui::ScrollArea::vertical()
                .max_height(ctx.screen_rect().height() * 0.65)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if self.entries.is_empty() {
                        ui.weak("No operations, messages or errors in this session.");
                    }
                    for entry in &self.entries {
                        ui.horizontal_top(|ui| {
                            ui.monospace(format!(
                                "{:02}:{:02}",
                                entry.seconds / 60,
                                entry.seconds % 60
                            ));
                            let text = egui::RichText::new(&entry.message);
                            let text = if entry.error {
                                text.color(ui.visuals().error_fg_color)
                            } else {
                                text
                            };
                            ui.add(egui::Label::new(text).wrap().selectable(true));
                        });
                        ui.separator();
                    }
                });
            if widgets::button(ui, "Close", false, egui::Vec2::ZERO).clicked() {
                self.open = false;
            }
        });
        if response.should_close() {
            self.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn latest_communication_clears_on_open_without_removing_history_or_reappearing() {
        let ctx = egui::Context::default();
        let mut log = Log::default();
        communicate(&ctx, Event::Operation("Selecting white balance…".into()));
        communicate(&ctx, Event::Error("Choose a neutral area".into()));
        log.receive(&ctx);
        assert_eq!(log.latest.as_deref(), Some("Choose a neutral area"));
        assert!(log.unread_errors());
        log.open();
        assert!(log.latest.is_none());
        assert!(!log.unread_errors());
        assert_eq!(log.entries.len(), 2);
        log.receive(&ctx);
        assert!(log.latest.is_none());
        log.operation("White balance selected.");
        assert_eq!(log.latest.as_deref(), Some("White balance selected."));
        assert!(!log.unread_errors());
    }
    #[test]
    fn acknowledged_failures_stay_read_until_a_new_failure_arrives() {
        let mut log = Log::default();
        log.observe(Origin::Edits, ["Cannot save edits".into()]);
        assert!(log.unread_errors);
        log.open = true;
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |ctx| log.show(ctx));
        assert!(!log.unread_errors);
        log.observe(Origin::Edits, ["Cannot save edits".into()]);
        log.operation("Exported a photograph");
        assert!(!log.unread_errors);
        assert_eq!(log.entries.len(), 2);
        log.observe(Origin::Edits, []);
        log.observe(Origin::Edits, ["Cannot save edits".into()]);
        assert!(log.unread_errors);
        for _ in 0..MAX_ENTRIES {
            log.operation("Completed");
        }
        assert_eq!(log.entries.len(), MAX_ENTRIES);
        assert!(log.unread_errors);
    }
}
