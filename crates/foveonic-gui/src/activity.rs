//! Presents retained launch logs and copy controls while keeping one acknowledged toolbar status.

use crate::widgets;
use eframe::egui;
use foveonic::support::{self, ClearRequest, Handle, Level, Snapshot, VisibleSelection};
use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

const MAX_ENTRIES: usize = 1000;

#[derive(Clone)]
pub enum Event {
    Operation(String),
    Error(String),
    /// The worker already captured the typed failure and its stack.
    ReportedError(String),
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
    support: Option<Handle>,
    snapshot: Arc<Snapshot>,
    refreshed: Option<Instant>,
    clearing: Option<ClearRequest>,
    copying: Option<mpsc::Receiver<String>>,
    copy_cancelled: bool,
    notice: Option<String>,
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
            support: support::handle(),
            snapshot: Arc::new(Snapshot::default()),
            refreshed: None,
            clearing: None,
            copying: None,
            copy_cancelled: false,
            notice: None,
        }
    }
}
impl Log {
    pub fn record(&mut self, event: Event) {
        match event {
            Event::Operation(message) => self.operation(message),
            Event::Error(message) => self.error(message),
            Event::ReportedError(message) => self.error_reported(message),
        }
    }
    pub fn open(&mut self) {
        self.open = true;
        self.latest = None;
        self.unread_errors = false;
        self.refreshed = None;
        if let Some(handle) = &self.support {
            handle.refresh_history();
        }
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
    fn push(&mut self, error: bool, message: String, reported: bool) {
        if !reported {
            support::event(
                if error { Level::Error } else { Level::Info },
                if error { "ui.error" } else { "ui.operation" },
                &message,
                serde_json::json!({"capture_site":"UI message", "typed_origin_available":false}),
            );
        }
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
        self.push(false, message.into(), false);
    }
    pub fn error(&mut self, message: impl Into<String>) {
        self.push(true, message.into(), false);
    }

    pub fn error_reported(&mut self, message: impl Into<String>) {
        self.push(true, message.into(), true);
    }

    /// Persistent failures are recorded on arrival, then again only if they resolve and recur.
    pub fn observe(&mut self, origin: Origin, errors: impl IntoIterator<Item = String>) {
        self.observe_errors(origin, errors, false);
    }
    pub fn observe_reported(&mut self, origin: Origin, errors: impl IntoIterator<Item = String>) {
        self.observe_errors(origin, errors, true);
    }
    fn observe_errors(
        &mut self,
        origin: Origin,
        errors: impl IntoIterator<Item = String>,
        reported: bool,
    ) {
        let current: BTreeSet<_> = errors.into_iter().collect();
        let previous = self.observed.entry(origin).or_default();
        let arrived: Vec<_> = current.difference(previous).cloned().collect();
        *previous = current;
        for error in arrived {
            self.push(true, error, reported);
        }
    }
    pub fn button(&mut self, ui: &mut egui::Ui) {
        if widgets::notice_button(ui, "Log", self.unread_errors)
            .on_hover_text(if self.unread_errors {
                "New errors. Open the log."
            } else {
                "Retained application launches, operations and errors"
            })
            .clicked()
        {
            self.open();
        }
    }
    pub fn show(&mut self, ctx: &egui::Context) {
        if let Some(receiver) = &self.copying {
            match receiver.try_recv() {
                Ok(text) => {
                    if !self.copy_cancelled {
                        ctx.copy_text(text);
                        self.notice = Some("Logs copied".into());
                    }
                    self.copying = None;
                    self.copy_cancelled = false;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.copying = None;
                    self.notice = Some("Could not prepare logs for copying".into());
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(50));
                }
            }
        }
        if let Some(request) = &self.clearing {
            if let Some(result) = request.try_result() {
                self.clearing = None;
                match result {
                    Ok(()) => {
                        self.entries.clear();
                        self.refreshed = None;
                        self.notice=Some("Visible history cleared. Rotated files remain within the retention limit.".into());
                    }
                    Err(error) => self.notice = Some(format!("Could not clear logs: {error}")),
                }
            } else {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
        }
        if !self.open {
            return;
        }
        if self
            .refreshed
            .is_none_or(|at| at.elapsed() >= Duration::from_millis(250))
        {
            if let Some(handle) = &self.support {
                self.snapshot = handle.snapshot_shared();
            }
            self.refreshed = Some(Instant::now());
        }
        ctx.request_repaint_after(Duration::from_millis(250));
        self.unread_errors = false;
        self.latest = None;
        let mut copy = None;
        let mut clear = false;
        let response = egui::Modal::new(egui::Id::new("Activity log")).show(ctx, |ui| {
            ui.set_width((ctx.screen_rect().width() - 80.).clamp(240., 720.));
            ui.heading("Log");
            if let Some(handle) = &self.support {
                let status = handle.status();
                ui.weak(format!("Files: {}", status.directory.display()));
                if let Some(error) = status.persistence_error {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        format!("Logs are not fully saved: {error}"),
                    );
                }
            }
            egui::ScrollArea::vertical()
                .max_height(ctx.screen_rect().height() * 0.65)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for notice in &self.snapshot.incomplete {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!("History incomplete: {notice}"),
                        );
                    }
                    if self.support.is_some() {
                        if self.snapshot.runs.iter().all(|run| run.events.is_empty()) {
                            ui.weak("No visible log entries.");
                        }
                        for run in &self.snapshot.runs {
                            ui.add_space(12.);
                            ui.separator();
                            ui.horizontal_wrapped(|ui| {
                                ui.strong(format!(
                                    "Application launch — {}",
                                    timestamp(run.started_at_ms)
                                ));
                                if ui
                                    .add_enabled(
                                        self.copying.is_none() && self.clearing.is_none(),
                                        egui::Button::new("Copy run logs"),
                                    )
                                    .clicked()
                                {
                                    copy = Some(VisibleSelection::Run(run.launch_id.clone()));
                                }
                            });
                            for notice in &run.incomplete {
                                ui.colored_label(
                                    ui.visuals().warn_fg_color,
                                    format!("History incomplete: {notice}"),
                                );
                            }
                            ui.add_space(4.);
                            for entry in &run.events {
                                ui.horizontal_top(|ui| {
                                    ui.monospace(timestamp(entry.timestamp_ms));
                                    let mut text = egui::RichText::new(&entry.message);
                                    if entry.level == Level::Error {
                                        text = text.color(ui.visuals().error_fg_color);
                                    } else if entry.level == Level::Warning {
                                        text = text.color(ui.visuals().warn_fg_color);
                                    }
                                    ui.add(egui::Label::new(text).wrap().selectable(true));
                                });
                                if entry.level == Level::Error && !entry.fields.is_null() {
                                    ui.push_id((&run.launch_id, entry.sequence), |ui| {
                                        ui.collapsing("Error details", |ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    serde_json::to_string_pretty(&entry.fields)
                                                        .unwrap_or_default(),
                                                )
                                                .wrap()
                                                .selectable(true),
                                            );
                                        });
                                    });
                                }
                            }
                        }
                    } else {
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
                    }
                });
            if let Some(notice) = &self.notice {
                ui.label(notice);
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        self.support.is_some() && self.copying.is_none() && self.clearing.is_none(),
                        egui::Button::new("Copy logs"),
                    )
                    .clicked()
                {
                    copy = Some(VisibleSelection::All);
                }
                if ui
                    .add_enabled(
                        self.support.is_some() && self.clearing.is_none(),
                        egui::Button::new("Clear"),
                    )
                    .on_hover_text("Clear visible history and rotate the current log file")
                    .clicked()
                {
                    clear = true;
                }
                if widgets::button(ui, "Close", false, egui::Vec2::ZERO).clicked() {
                    self.open = false;
                }
            });
        });
        if response.should_close() {
            self.open = false;
        }
        if clear {
            self.begin_clear();
        }
        if let Some(selection) = copy {
            self.begin_copy(ctx, selection);
        }
    }
    fn begin_clear(&mut self) {
        self.copy_cancelled = self.copying.is_some();
        if let Some(handle) = &self.support {
            self.clearing = Some(handle.request_clear());
        }
    }
    fn begin_copy(&mut self, ctx: &egui::Context, selection: VisibleSelection) {
        if self.copying.is_some() || self.clearing.is_some() {
            return;
        }
        let snapshot = self.snapshot.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        let context = ctx.clone();
        match std::thread::Builder::new()
            .name("support-copy".into())
            .spawn(move || {
                let _ = sender.send(snapshot.copy_report(selection));
                context.request_repaint();
            }) {
            Ok(_) => {
                self.copy_cancelled = false;
                self.copying = Some(receiver);
                self.notice = Some("Preparing logs…".into());
            }
            Err(error) => {
                support::report_error("support.copy_worker", &error, Default::default());
                self.notice = Some(format!("Could not copy logs: {error}"));
            }
        }
    }
}

fn timestamp(milliseconds: u64) -> String {
    time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(milliseconds) * 1_000_000)
        .ok()
        .and_then(|time| {
            time.format(&time::format_description::well_known::Rfc3339)
                .ok()
        })
        .unwrap_or_else(|| format!("{milliseconds} ms since Unix epoch"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copy_actions_emit_visible_launch_reports_and_clear_cancels_pending_copy() {
        const CHILD: &str = "FOVEONIC_ACTIVITY_REPORT_TEST";
        let Some(directory) = std::env::var_os(CHILD) else {
            let directory = tempfile::tempdir().unwrap();
            let output=std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact","activity::tests::copy_actions_emit_visible_launch_reports_and_clear_cancels_pending_copy","--nocapture","--test-threads=1"])
                .env(CHILD,directory.path()).output().unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        };
        let config = |gpu: &str| support::Config {
            directory: directory.clone().into(),
            application: "Activity test".into(),
            version: "1".into(),
            initial_context: serde_json::json!({"system":{"os":"Test OS","cpu":"Test CPU","ram_bytes":8_589_934_592u64},"gpu":gpu,"settings":{"exposure":0.7}}),
        };
        let mut first = support::initialize(config("First GPU"));
        support::event(
            Level::Info,
            "test.first",
            "first launch operation",
            serde_json::Value::Null,
        );
        assert_eq!(
            first.shutdown(Duration::from_secs(3)),
            support::ShutdownStatus::Drained
        );
        let mut second = support::initialize(config("Second GPU"));
        let context = egui::Context::default();
        let mut log = Log::default();
        log.operation("second launch operation");
        let failure = support::report_error(
            "test.worker",
            &std::io::Error::from_raw_os_error(5),
            support::OperationContext {
                source: Some("photograph.x3f".into()),
                settings: Some(serde_json::json!({"exposure":0.7})),
                ..Default::default()
            },
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while !support::history()
            .runs
            .iter()
            .flat_map(|run| &run.events)
            .any(|event| {
                event.fields.get("id").and_then(serde_json::Value::as_str)
                    == Some(failure.id.as_str())
            })
        {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        log.snapshot = second.handle().snapshot_shared();
        log.begin_copy(&context, VisibleSelection::All);
        let all = clipboard(&context, &mut log);
        for expected in [
            "first launch operation",
            "second launch operation",
            "Test CPU",
            "ram_bytes",
            "First GPU",
            "Second GPU",
            "photograph.x3f",
            "os_code",
            "backtrace",
            "0.7",
        ] {
            assert!(all.contains(expected), "missing {expected}");
        }
        let current = log
            .snapshot
            .runs
            .iter()
            .find(|run| {
                run.events
                    .iter()
                    .any(|event| event.message == "second launch operation")
            })
            .unwrap()
            .launch_id
            .clone();
        log.begin_copy(&context, VisibleSelection::Run(current));
        let one = clipboard(&context, &mut log);
        assert!(one.contains("Second GPU") && one.contains("second launch operation"));
        assert!(!one.contains("First GPU") && !one.contains("first launch operation"));
        log.begin_copy(&context, VisibleSelection::All);
        log.begin_clear();
        let deadline = Instant::now() + Duration::from_secs(5);
        while log.clearing.is_some() || log.copying.is_some() {
            let output = context.run(Default::default(), |context| log.show(context));
            assert!(
                !output
                    .platform_output
                    .commands
                    .iter()
                    .any(|command| matches!(command, egui::OutputCommand::CopyText(_))),
                "Clear must cancel a pending old-history copy"
            );
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        log.operation("after clear operation");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !support::history()
            .copy_report(VisibleSelection::All)
            .contains("after clear operation")
        {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        log.snapshot = second.handle().snapshot_shared();
        log.begin_copy(&context, VisibleSelection::All);
        let after = clipboard(&context, &mut log);
        assert!(after.contains("after clear operation") && after.contains("Second GPU"));
        assert!(
            !after.contains("first launch operation")
                && !after.contains("second launch operation")
                && !after.contains("First GPU")
        );
        assert_eq!(
            second.shutdown(Duration::from_secs(3)),
            support::ShutdownStatus::Drained
        );
    }

    fn clipboard(context: &egui::Context, log: &mut Log) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let output = context.run(Default::default(), |context| log.show(context));
            if let Some(text) = output
                .platform_output
                .commands
                .into_iter()
                .find_map(|command| match command {
                    egui::OutputCommand::CopyText(text) => Some(text),
                    _ => None,
                })
            {
                return text;
            }
            assert!(
                Instant::now() < deadline,
                "copy action did not emit clipboard output"
            );
            std::thread::yield_now();
        }
    }
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
