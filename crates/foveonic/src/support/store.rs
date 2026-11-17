//! Owns bounded support-log queues, segment retention, visibility and emergency panic evidence.

use super::{Config, Level, Record, RunSnapshot, Snapshot};
use fs2::FileExt;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const RECORD_LIMIT: usize = 64 * 1024;
const SEGMENT_LIMIT: u64 = 4 * 1024 * 1024;
const SEGMENT_RECORDS: usize = 1000;
const NORMAL_COUNT: usize = 512;
const NORMAL_BYTES: usize = 4 * 1024 * 1024;
const ERROR_COUNT: usize = 32;
const ARCHIVES: usize = 3;
const MEMORY_RECORD_BYTES: usize = 16 * 1024 * 1024;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[derive(Clone, Debug)]
pub struct Status {
    pub directory: PathBuf,
    pub persistence_error: Option<String>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownStatus {
    Drained,
    TimedOut,
    Disconnected,
}

struct Shared {
    tx: Sender<Command>,
    ingest: Mutex<Ingest>,
    snapshot: Mutex<Arc<Snapshot>>,
    status: Mutex<Status>,
    start: Instant,
    launch: String,
    sequence: AtomicU64,
}
struct Ingest {
    normal: usize,
    bytes: usize,
    errors: usize,
    dropped: u64,
    closed: bool,
    context_sizes: HashMap<String, usize>,
}
#[derive(Clone)]
pub struct Handle {
    shared: Arc<Shared>,
}
pub struct Guard {
    handle: Handle,
    worker: Option<JoinHandle<()>>,
}
pub struct ClearRequest {
    rx: Receiver<Result<(), String>>,
}
impl ClearRequest {
    pub fn try_result(&self) -> Option<Result<(), String>> {
        match self.rx.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("support writer stopped".into())),
        }
    }
}
enum Command {
    Record(Record, usize, bool),
    Context(Vec<Record>, usize, bool),
    Refresh,
    Clear(Sender<Result<(), String>>),
    Stop(Sender<()>),
}

impl Handle {
    pub fn snapshot(&self) -> Snapshot {
        (*self.snapshot_shared()).clone()
    }
    /// Read the latest immutable view without cloning its records.
    pub fn snapshot_shared(&self) -> Arc<Snapshot> {
        self.shared
            .snapshot
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
    }
    pub fn status(&self) -> Status {
        self.shared
            .status
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|_| Status {
                directory: PathBuf::new(),
                persistence_error: Some("support status unavailable".into()),
            })
    }
    pub fn log_directory(&self) -> PathBuf {
        self.status().directory
    }
    pub fn refresh_history(&self) {
        let _ = self.shared.tx.send(Command::Refresh);
    }
    pub fn request_clear(&self) -> ClearRequest {
        let (tx, rx) = mpsc::channel();
        if let Ok(ingest) = self.shared.ingest.lock() {
            if !ingest.closed {
                let _ = self.shared.tx.send(Command::Clear(tx));
            }
        }
        ClearRequest { rx }
    }
    fn record(
        &self,
        _ingest: &mut Ingest,
        level: Level,
        name: &str,
        message: &str,
        fields: Value,
    ) -> Record {
        let record = Record {
            version: 1,
            launch_id: self.shared.launch.clone(),
            sequence: self.shared.sequence.fetch_add(1, Ordering::Relaxed),
            timestamp_ms: now_ms(),
            elapsed_ms: self
                .shared
                .start
                .elapsed()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
            thread: format!("{:?}", std::thread::current()),
            level,
            event: name.into(),
            message: message.into(),
            fields,
        };
        bounded(record)
    }
    pub fn event(&self, level: Level, name: &str, message: &str, fields: Value) {
        let Ok(mut ingest) = self.shared.ingest.lock() else {
            return;
        };
        if ingest.closed {
            return;
        }
        let record = self.record(&mut ingest, level, name, message, fields);
        let size = serde_json::to_vec(&record).map_or(RECORD_LIMIT, |b| b.len());
        let error = level == Level::Error;
        if (error && ingest.errors >= ERROR_COUNT)
            || (!error
                && (ingest.normal >= NORMAL_COUNT
                    || ingest.bytes.saturating_add(size) > NORMAL_BYTES))
        {
            ingest.dropped += 1;
            return;
        }
        let loss = if ingest.dropped != 0 {
            let dropped = std::mem::take(&mut ingest.dropped);
            let loss = self.record(
                &mut ingest,
                Level::Warning,
                "log.history_incomplete",
                "Events were dropped because the support queue was full",
                json!({"dropped": dropped}),
            );
            Some(loss)
        } else {
            None
        };
        if error {
            ingest.errors += 1;
        } else {
            ingest.normal += 1;
            ingest.bytes += size;
        }
        if self
            .shared
            .tx
            .send(Command::Record(record, size, error))
            .is_err()
        {
            if error {
                ingest.errors -= 1;
            } else {
                ingest.normal -= 1;
                ingest.bytes -= size;
            }
        }
        if let Some(loss) = loss {
            let _ = self.shared.tx.send(Command::Context(vec![loss], 0, false));
        }
    }
    pub fn update_context(&self, section: &str, value: Value) {
        let Ok(mut ingest) = self.shared.ingest.lock() else {
            return;
        };
        if ingest.closed {
            return;
        }
        let Ok(text) = serde_json::to_string(&value) else {
            return;
        };
        if text.len() > NORMAL_BYTES / 4 || section.len() > 1024 {
            ingest.dropped += 1;
            return;
        }
        let previous = ingest.context_sizes.get(section).copied().unwrap_or(0);
        let context_bytes: usize = ingest.context_sizes.values().sum();
        if context_bytes
            .saturating_sub(previous)
            .saturating_add(text.len())
            > NORMAL_BYTES / 4
            || (previous == 0 && ingest.context_sizes.len() >= 128)
        {
            ingest.dropped += 1;
            return;
        }
        let records = context_records(self, &mut ingest, section, &text);
        let count = records.len();
        let size: usize = records
            .iter()
            .map(|r| serde_json::to_vec(r).map_or(RECORD_LIMIT, |v| v.len()))
            .sum();
        if ingest.normal + records.len() > NORMAL_COUNT
            || ingest.bytes.saturating_add(size) > NORMAL_BYTES
        {
            ingest.dropped += records.len() as u64;
            return;
        }
        ingest.normal += records.len();
        ingest.bytes += size;
        ingest.context_sizes.insert(section.into(), text.len());
        if self
            .shared
            .tx
            .send(Command::Context(records, size, true))
            .is_err()
        {
            ingest.normal -= count;
            ingest.bytes -= size;
            if previous == 0 {
                ingest.context_sizes.remove(section);
            } else {
                ingest.context_sizes.insert(section.into(), previous);
            }
        }
    }
}

fn context_records(handle: &Handle, ingest: &mut Ingest, section: &str, text: &str) -> Vec<Record> {
    let chunks = chunks(text, 4 * 1024);
    let count = chunks.len();
    chunks
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            handle.record(
                ingest,
                Level::Info,
                "context.updated",
                "Context updated",
                json!({"section":section,"part":index,"parts":count,"json":text}),
            )
        })
        .collect()
}
impl Guard {
    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }
    pub fn snapshot(&self) -> Snapshot {
        self.handle.snapshot()
    }
    pub fn status(&self) -> Status {
        self.handle.status()
    }
    pub fn log_directory(&self) -> PathBuf {
        self.handle.log_directory()
    }
    pub fn request_clear(&self) -> ClearRequest {
        self.handle.request_clear()
    }
    pub fn shutdown(&mut self, timeout: Duration) -> ShutdownStatus {
        if self.worker.is_none() {
            return ShutdownStatus::Drained;
        }
        let (tx, rx) = mpsc::channel();
        let sent = self
            .handle
            .shared
            .ingest
            .lock()
            .map(|mut ingest| {
                ingest.closed = true;
                if ingest.dropped != 0 {
                    let dropped = std::mem::take(&mut ingest.dropped);
                    let record = self.handle.record(
                        &mut ingest,
                        Level::Warning,
                        "log.history_incomplete",
                        "Events were dropped because the support queue was full",
                        json!({"dropped":dropped}),
                    );
                    let _ = self
                        .handle
                        .shared
                        .tx
                        .send(Command::Context(vec![record], 0, false));
                }
                self.handle.shared.tx.send(Command::Stop(tx)).is_ok()
            })
            .unwrap_or(false);
        if !sent {
            self.worker.take();
            return ShutdownStatus::Disconnected;
        }
        match rx.recv_timeout(timeout) {
            Ok(()) => {
                if let Some(worker) = self.worker.take() {
                    let _ = worker.join();
                }
                ShutdownStatus::Drained
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.worker.take();
                ShutdownStatus::TimedOut
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.worker.take();
                ShutdownStatus::Disconnected
            }
        }
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.shutdown(Duration::from_millis(200));
    }
}

fn chunks(text: &str, max: usize) -> Vec<&str> {
    if text.is_empty() {
        return vec![""];
    }
    let mut out = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = (start + max).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        out.push(&text[start..end]);
        start = end;
    }
    out
}
fn bounded(mut record: Record) -> Record {
    let size = serde_json::to_vec(&record).map_or(usize::MAX, |b| b.len());
    if size >= RECORD_LIMIT {
        if (record.fields.get("id").is_some_and(Value::is_string)
            && record.fields.get("causes").is_some_and(Value::is_array))
            || (record
                .fields
                .get("failure_id")
                .is_some_and(Value::is_string)
                && record.fields.get("operation").is_some_and(Value::is_object))
        {
            bound_failure(&mut record, size);
            if record_size(&record) < RECORD_LIMIT {
                return record;
            }
        }
        let mut budget = 32 * 1024;
        trim_value(&mut record.fields, &mut budget, 0);
        if let Value::Object(fields) = &mut record.fields {
            fields.insert("truncated".into(), Value::Bool(true));
            fields.insert("original_bytes".into(), json!(size));
        } else {
            record.fields = json!({"truncated":true,"original_bytes":size,"value":record.fields});
        }
        record.message = chunks(&record.message, 8 * 1024)
            .first()
            .copied()
            .unwrap_or("")
            .into();
        record.event = chunks(&record.event, 1024)
            .first()
            .copied()
            .unwrap_or("")
            .into();
        if serde_json::to_vec(&record).map_or(true, |v| v.len() >= RECORD_LIMIT) {
            record.fields = json!({"truncated":true,"original_bytes":size});
        }
    }
    record
}

/// Keep correlation, paths and numeric causes before spending space on error prose.
fn bound_failure(record: &mut Record, original_bytes: usize) {
    record.fields["truncated"] = json!(true);
    record.fields["original_bytes"] = json!(original_bytes);
    record.fields["truncated_fields"] = json!(["backtrace", "message", "cause messages"]);
    let mut allowance = 16 * 1024;
    loop {
        shorten(&mut record.message, allowance.min(4 * 1024));
        for (key, limit) in [
            ("backtrace", allowance),
            ("message", allowance.min(4 * 1024)),
        ] {
            if let Some(Value::String(text)) = record.fields.get_mut(key) {
                shorten(text, limit);
            }
        }
        if let Some(causes) = record
            .fields
            .get_mut("causes")
            .and_then(Value::as_array_mut)
        {
            for cause in causes {
                if let Some(Value::String(message)) = cause.get_mut("message") {
                    shorten(message, allowance.min(256));
                }
            }
        }
        if record_size(record) < RECORD_LIMIT {
            return;
        }
        if allowance == 0 {
            break;
        }
        allowance /= 2;
    }
    if let Some(operation) = record
        .fields
        .get_mut("operation")
        .and_then(Value::as_object_mut)
    {
        if let Some(settings) = operation.get_mut("settings") {
            let bytes = serde_json::to_vec(settings).map_or(0, |value| value.len());
            *settings = json!({"truncated":true,"original_bytes":bytes,"reason":"frozen settings exceed the failure record limit"});
        }
        // Native paths can themselves exceed a bounded record when escaped.
        for value in operation.values_mut() {
            if let Value::String(text) = value {
                shorten(text, 2048);
            }
        }
    }
    for key in ["caller", "thread", "boundary"] {
        if let Some(Value::String(text)) = record.fields.get_mut(key) {
            shorten(text, 2048);
        }
    }
    record.fields["truncated_fields"] = json!([
        "backtrace",
        "message",
        "cause messages",
        "operation context"
    ]);
}

fn shorten(text: &mut String, limit: usize) {
    if text.len() > limit {
        let mut end = limit;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str(" [truncated]");
    }
}

fn record_size(record: &Record) -> usize {
    serde_json::to_vec(record).map_or(RECORD_LIMIT, |bytes| bytes.len() + 1)
}

fn trim_value(value: &mut Value, budget: &mut usize, depth: usize) {
    if depth >= 16 || *budget == 0 {
        *value = Value::String("[truncated]".into());
        return;
    }
    match value {
        Value::String(text) => {
            let limit = (*budget).min(16 * 1024);
            if text.len() > limit {
                let mut end = limit;
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                text.truncate(end);
                text.push_str(" [truncated]");
            }
            *budget = budget.saturating_sub(text.len());
        }
        Value::Array(items) => {
            items.truncate(128);
            for item in items {
                trim_value(item, budget, depth + 1);
            }
        }
        Value::Object(items) => {
            while items.len() > 128 {
                if let Some(key) = items.keys().next_back().cloned() {
                    items.remove(&key);
                }
            }
            for (key, item) in items {
                *budget = budget.saturating_sub(key.len());
                trim_value(item, budget, depth + 1);
            }
        }
        _ => *budget = budget.saturating_sub(32),
    }
}

pub(super) fn start(config: Config) -> Guard {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let launch = format!(
        "{}-{}-{}",
        now_ms(),
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let (tx, rx) = mpsc::channel();
    let shared = Arc::new(Shared {
        tx,
        ingest: Mutex::new(Ingest {
            normal: 0,
            bytes: 0,
            errors: 0,
            dropped: 0,
            closed: false,
            context_sizes: HashMap::new(),
        }),
        snapshot: Mutex::new(Arc::new(Snapshot::default())),
        status: Mutex::new(Status {
            directory: config.directory.clone(),
            persistence_error: None,
        }),
        start: Instant::now(),
        launch: launch.clone(),
        sequence: AtomicU64::new(1),
    });
    let handle = Handle {
        shared: shared.clone(),
    };
    let mut writer = Writer::new(shared.clone(), &config);
    writer.open_initial();
    if let Ok(mut ingest) = shared.ingest.lock() {
        let first = handle.record(
            &mut ingest,
            Level::Info,
            "launch.started",
            "Application launch started",
            json!({"application":config.application,"version":config.version}),
        );
        writer.accept(first);
        if let Ok(context) = serde_json::to_string(&config.initial_context) {
            if context.len() <= NORMAL_BYTES / 4 {
                let records = context_records(&handle, &mut ingest, "launch", &context);
                ingest.context_sizes.insert("launch".into(), context.len());
                writer.contexts.insert("launch".into(), records.clone());
                for record in records {
                    writer.accept(record);
                }
            } else {
                let record = handle.record(
                    &mut ingest,
                    Level::Warning,
                    "log.history_incomplete",
                    "Initial context exceeded its bound",
                    Value::Null,
                );
                writer.accept(record);
            }
        }
    }
    let worker = thread::Builder::new()
        .name("support-log".into())
        .spawn(move || writer.run(rx));
    let worker = match worker {
        Ok(worker) => Some(worker),
        Err(error) => {
            set_error(&shared, error);
            None
        }
    };
    Guard {
        handle: handle.clone(),
        worker,
    }
}

fn set_error(shared: &Shared, error: impl std::fmt::Display) {
    if let Ok(mut status) = shared.status.lock() {
        status.persistence_error = Some(error.to_string());
    }
}

struct Segment {
    file: File,
    _lock: File,
    bytes: u64,
    records: usize,
}
struct Writer {
    shared: Arc<Shared>,
    dir: PathBuf,
    segment: Option<Segment>,
    index: u64,
    records: Vec<Record>,
    records_bytes: usize,
    cutoffs: BTreeMap<String, u64>,
    headers: Vec<Record>,
    contexts: HashMap<String, Vec<Record>>,
    incomplete: Vec<String>,
    rotation_retry: Option<Instant>,
    pending: Vec<Record>,
    pending_bytes: usize,
    pending_dropped: u64,
}
impl Writer {
    fn new(shared: Arc<Shared>, config: &Config) -> Self {
        Self {
            shared,
            dir: config.directory.clone(),
            segment: None,
            index: 0,
            records: Vec::new(),
            records_bytes: 0,
            cutoffs: BTreeMap::new(),
            headers: Vec::new(),
            contexts: HashMap::new(),
            incomplete: Vec::new(),
            rotation_retry: None,
            pending: Vec::new(),
            pending_bytes: 0,
            pending_dropped: 0,
        }
    }
    fn open_initial(&mut self) {
        if let Err(error) = fs::create_dir_all(&self.dir)
            .and_then(|_| self.refresh())
            .and_then(|_| self.rotate())
        {
            if error.kind() == io::ErrorKind::WouldBlock {
                self.rotation_retry = Some(Instant::now() + Duration::from_secs(1));
                self.incomplete
                    .push("Initial support segment is waiting for another process".into());
            } else {
                set_error(&self.shared, error);
            }
        }
    }
    fn run(mut self, rx: Receiver<Command>) {
        loop {
            let command = match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(command) => command,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    self.retry_rotation();
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            match command {
                Command::Record(record, size, error) => {
                    if let Ok(mut ingest) = self.shared.ingest.lock() {
                        if error {
                            ingest.errors = ingest.errors.saturating_sub(1);
                        } else {
                            ingest.normal = ingest.normal.saturating_sub(1);
                            ingest.bytes = ingest.bytes.saturating_sub(size);
                        }
                        if ingest.dropped != 0 {
                            let dropped = std::mem::take(&mut ingest.dropped);
                            let handle = Handle {
                                shared: self.shared.clone(),
                            };
                            let notice = handle.record(
                                &mut ingest,
                                Level::Warning,
                                "log.history_incomplete",
                                "Events were dropped because the support queue was full",
                                json!({"dropped":dropped}),
                            );
                            let _ = self
                                .shared
                                .tx
                                .send(Command::Context(vec![notice], 0, false));
                        }
                    }
                    self.accept(record);
                }
                Command::Context(records, size, counted) => {
                    if counted {
                        if let Ok(mut ingest) = self.shared.ingest.lock() {
                            ingest.normal = ingest.normal.saturating_sub(records.len());
                            ingest.bytes = ingest.bytes.saturating_sub(size);
                            if ingest.dropped != 0 {
                                let dropped = std::mem::take(&mut ingest.dropped);
                                let handle = Handle {
                                    shared: self.shared.clone(),
                                };
                                let notice = handle.record(
                                    &mut ingest,
                                    Level::Warning,
                                    "log.history_incomplete",
                                    "Events were dropped because the support queue was full",
                                    json!({"dropped":dropped}),
                                );
                                let _ =
                                    self.shared
                                        .tx
                                        .send(Command::Context(vec![notice], 0, false));
                            }
                        }
                    }
                    if records
                        .first()
                        .is_some_and(|r| r.event == "context.updated")
                    {
                        if let Some(section) =
                            records[0].fields.get("section").and_then(Value::as_str)
                        {
                            self.contexts.insert(section.into(), records.clone());
                        }
                    }
                    for record in records {
                        self.accept(record);
                    }
                }
                Command::Refresh => {
                    if let Err(error) = self.refresh() {
                        set_error(&self.shared, error);
                    }
                }
                Command::Clear(reply) => {
                    if self.segment.is_none() && self.rotation_retry.is_some() {
                        self.rotation_retry = Some(Instant::now());
                        self.retry_rotation();
                    }
                    let result = if !self.pending.is_empty() {
                        Err("Support events are still waiting for the log file".into())
                    } else {
                        match self.clear() {
                            Ok(()) => Ok(()),
                            Err(error) => {
                                if error.kind() != io::ErrorKind::WouldBlock {
                                    set_error(&self.shared, &error);
                                }
                                Err(error.to_string())
                            }
                        }
                    };
                    let _ = reply.send(result);
                }
                Command::Stop(reply) => {
                    if self.rotation_retry.is_some() {
                        self.rotation_retry = Some(Instant::now());
                        self.retry_rotation();
                    }
                    if !self.pending.is_empty() {
                        set_error(
                            &self.shared,
                            "Buffered support events could not be persisted before shutdown",
                        );
                    }
                    if let Some(segment) = &mut self.segment {
                        if let Err(error) = segment.file.flush() {
                            set_error(&self.shared, error);
                        }
                    }
                    self.segment.take();
                    match maintenance(&self.dir).and_then(|lock| {
                        let result = prune(&self.dir);
                        drop(lock);
                        result
                    }) {
                        Ok(()) => {
                            let _ = self.refresh();
                        }
                        Err(error) => set_error(&self.shared, error),
                    }
                    let _ = reply.send(());
                    break;
                }
            }
            self.retry_rotation();
        }
    }
    fn accept(&mut self, record: Record) {
        if record.event == "launch.started" {
            self.headers = vec![record.clone()];
        }
        if self.segment.is_none() && self.rotation_retry.is_some() {
            self.pending_bytes += record_size(&record);
            self.pending.push(record.clone());
            while self.pending.len() > 4000 || self.pending_bytes > MEMORY_RECORD_BYTES {
                let index = self
                    .pending
                    .iter()
                    .position(|event| event.level != Level::Error)
                    .unwrap_or(0);
                let removed = self.pending.remove(index);
                self.pending_bytes = self.pending_bytes.saturating_sub(record_size(&removed));
                self.pending_dropped += 1;
                if !self.incomplete.iter().any(|notice| {
                    notice == "Support events were dropped while waiting for the log file"
                }) {
                    self.incomplete
                        .push("Support events were dropped while waiting for the log file".into());
                }
            }
        } else if let Err(error) = self.write_record(&record) {
            set_error(&self.shared, error);
            if !self
                .incomplete
                .iter()
                .any(|notice| notice == "Support logs could not be persisted")
            {
                self.incomplete
                    .push("Support logs could not be persisted".into());
            }
            self.segment.take();
        }
        self.records.push(record);
        self.records_bytes += record_size(self.records.last().expect("record just inserted"));
        self.bound_records();
        self.publish();
    }
    fn retry_rotation(&mut self) {
        if self.rotation_retry.is_none_or(|at| Instant::now() < at) {
            return;
        }
        match self.rotate() {
            Ok(()) => {
                self.rotation_retry = None;
                let pending = self.pending.clone();
                for record in pending {
                    if let Err(error) = self.write_record(&record) {
                        set_error(&self.shared, error);
                        self.incomplete
                            .push("Buffered support events could not be persisted".into());
                        self.segment.take();
                        self.records.extend(self.pending.iter().cloned());
                        self.records_bytes = self.records.iter().map(record_size).sum();
                        self.bound_records();
                        self.publish();
                        return;
                    }
                }
                self.pending.clear();
                self.pending_bytes = 0;
                if self.pending_dropped != 0 {
                    let dropped = std::mem::take(&mut self.pending_dropped);
                    let notice = Record {
                        version: 1,
                        launch_id: self.shared.launch.clone(),
                        sequence: self.shared.sequence.fetch_add(1, Ordering::Relaxed),
                        timestamp_ms: now_ms(),
                        elapsed_ms: self
                            .shared
                            .start
                            .elapsed()
                            .as_millis()
                            .min(u128::from(u64::MAX)) as u64,
                        thread: format!("{:?}", thread::current()),
                        level: Level::Warning,
                        event: "log.history_incomplete".into(),
                        message: "Support events were dropped while waiting for the log file"
                            .into(),
                        fields: json!({"dropped":dropped}),
                    };
                    self.accept(notice);
                }
                if let Err(error) = self.refresh() {
                    set_error(&self.shared, error);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                self.rotation_retry = Some(Instant::now() + Duration::from_secs(1))
            }
            Err(error) => {
                self.rotation_retry = None;
                set_error(&self.shared, error);
                self.incomplete
                    .push("Support logs could not be persisted".into());
            }
        }
    }
    fn write_record(&mut self, record: &Record) -> io::Result<()> {
        let mut bytes = serde_json::to_vec(record)?;
        bytes.push(b'\n');
        if bytes.len() > RECORD_LIMIT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "support record exceeds limit",
            ));
        }
        if self.segment.as_ref().is_some_and(|s| {
            s.records >= SEGMENT_RECORDS || s.bytes + bytes.len() as u64 > SEGMENT_LIMIT
        }) && self.rotation_retry.is_none_or(|at| Instant::now() >= at)
        {
            match self.rotate() {
                Ok(()) => self.rotation_retry = None,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    self.rotation_retry = Some(Instant::now() + Duration::from_secs(1));
                    if !self
                        .incomplete
                        .iter()
                        .any(|notice| notice == "Segment rotation was delayed by another process")
                    {
                        self.incomplete
                            .push("Segment rotation was delayed by another process".into());
                    }
                }
                Err(error) => return Err(error),
            }
        }
        let segment = self
            .segment
            .as_mut()
            .ok_or_else(|| io::Error::other("support log unavailable"))?;
        segment.file.write_all(&bytes)?;
        segment.file.flush()?;
        segment.bytes += bytes.len() as u64;
        segment.records += 1;
        Ok(())
    }
    fn rotate(&mut self) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let maintenance = maintenance(&self.dir)?;
        let (lock, file) = loop {
            let stem = format!("{}-{:06}", self.shared.launch, self.index);
            self.index += 1;
            let path = self.dir.join(format!("{stem}.jsonl"));
            let lock_path = self.dir.join(format!("{stem}.lock"));
            let lock = OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&lock_path)?;
            if let Err(error) = lock.try_lock_exclusive() {
                let _ = fs::remove_file(lock_path);
                return Err(error);
            }
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => break (lock, file),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    drop(lock);
                    let _ = fs::remove_file(lock_path);
                    continue;
                }
                Err(error) => {
                    drop(lock);
                    let _ = fs::remove_file(lock_path);
                    return Err(error);
                }
            }
        };
        self.segment = Some(Segment {
            file,
            _lock: lock,
            bytes: 0,
            records: 0,
        });
        prune(&self.dir)?;
        drop(maintenance);
        let mut headers = self.headers.clone();
        for records in self.contexts.values() {
            headers.extend(records.iter().cloned());
        }
        headers.sort_by_key(|r| r.sequence);
        for record in headers {
            let mut bytes = serde_json::to_vec(&record)?;
            bytes.push(b'\n');
            let segment = self.segment.as_mut().expect("new segment");
            if segment.bytes + bytes.len() as u64 > SEGMENT_LIMIT {
                return Err(io::Error::other("support context exceeds segment limit"));
            }
            segment.file.write_all(&bytes)?;
            segment.bytes += bytes.len() as u64;
            segment.records += 1;
        }
        if let Some(segment) = &mut self.segment {
            segment.file.flush()?;
        }
        self.refresh()?;
        Ok(())
    }
    fn refresh(&mut self) -> io::Result<()> {
        let (records, cutoffs, incomplete) = scan(&self.dir)?;
        let mut merged = records;
        if !self.pending.is_empty()
            || self
                .shared
                .status
                .lock()
                .map_or(true, |status| status.persistence_error.is_some())
        {
            let mut seen: HashSet<_> = merged
                .iter()
                .map(|r| (r.launch_id.clone(), r.sequence))
                .collect();
            for record in &self.records {
                if seen.insert((record.launch_id.clone(), record.sequence)) {
                    merged.push(record.clone());
                }
            }
        }
        merged.sort_by_key(|record| record.timestamp_ms);
        self.records = merged;
        self.records_bytes = self.records.iter().map(record_size).sum();
        self.cutoffs = cutoffs;
        self.incomplete = incomplete;
        self.bound_records();
        self.publish();
        Ok(())
    }
    fn bound_records(&mut self) {
        while self.records.len() > 4000 || self.records_bytes > MEMORY_RECORD_BYTES {
            if let Some(launch) = self
                .records
                .iter()
                .find(|record| record.launch_id != self.shared.launch)
                .map(|record| record.launch_id.clone())
            {
                self.records.retain(|record| record.launch_id != launch);
                self.records_bytes = self.records.iter().map(record_size).sum();
            } else {
                let removed = self.records.remove(0);
                self.records_bytes = self.records_bytes.saturating_sub(record_size(&removed));
            }
            if !self
                .incomplete
                .iter()
                .any(|notice| notice == "In-memory history was truncated")
            {
                self.incomplete
                    .push("In-memory history was truncated".into());
            }
        }
    }
    fn publish(&self) {
        self.publish_with(self.incomplete.clone());
    }
    fn publish_with(&self, incomplete: Vec<String>) {
        let mut records = self.headers.clone();
        for context in self.contexts.values() {
            records.extend(context.iter().cloned());
        }
        records.extend(self.records.iter().cloned());
        let result = make_snapshot(&records, &self.cutoffs, incomplete, &self.shared.launch);
        if let Ok(mut snapshot) = self.shared.snapshot.lock() {
            *snapshot = Arc::new(result);
        }
    }
    fn clear(&mut self) -> io::Result<()> {
        let lock = maintenance(&self.dir)?;
        let (disk, mut cutoffs, _) = scan_unlocked(&self.dir)?;
        for record in disk.iter().chain(&self.records) {
            cutoffs
                .entry(record.launch_id.clone())
                .and_modify(|n| *n = (*n).max(record.sequence))
                .or_insert(record.sequence);
        }
        let retained: HashSet<_> = disk
            .iter()
            .map(|record| record.launch_id.as_str())
            .collect();
        cutoffs.retain(|launch, _| {
            retained.contains(launch.as_str()) || launch == &self.shared.launch
        });
        let mut temp = tempfile::NamedTempFile::new_in(&self.dir)?;
        serde_json::to_writer(&mut temp, &cutoffs)?;
        temp.flush()?;
        temp.persist(self.dir.join("cutoffs.json"))
            .map_err(|e| e.error)?;
        self.cutoffs = cutoffs;
        drop(lock);
        self.rotate()?;
        self.rotation_retry = None;
        self.publish();
        Ok(())
    }
}

fn maintenance(dir: &Path) -> io::Result<File> {
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("maintenance.lock"))?;
    let deadline = Instant::now() + Duration::from_millis(100);
    loop {
        match lock.try_lock_exclusive() {
            Ok(()) => return Ok(lock),
            Err(error) => {
                let raw = error.raw_os_error();
                let contended = error.kind() == io::ErrorKind::WouldBlock
                    || (raw.is_some() && raw == fs2::lock_contended_error().raw_os_error());
                if !contended {
                    return Err(error);
                }
                if Instant::now() >= deadline {
                    return Err(io::Error::new(io::ErrorKind::WouldBlock, error));
                }
                thread::sleep(Duration::from_millis(2));
            }
        }
    }
}
fn prune(dir: &Path) -> io::Result<()> {
    let mut candidates = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "jsonl") {
            continue;
        }
        let lock_path = path.with_extension("lock");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        if lock.try_lock_exclusive().is_ok() {
            candidates.push((
                entry.metadata()?.modified().unwrap_or(UNIX_EPOCH),
                path,
                lock_path,
                lock,
            ));
        }
    }
    candidates.sort_by_key(|e| e.0);
    let excess = candidates.len().saturating_sub(ARCHIVES);
    if excess != 0 {
        fs::write(
            dir.join("history-pruned"),
            b"Earlier support segments were pruned.\n",
        )?;
    }
    for (_, path, lock_path, lock) in candidates.into_iter().take(excess) {
        fs::remove_file(path)?;
        drop(lock);
        let _ = fs::remove_file(lock_path);
    }
    let paths: Vec<_> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    for panic in paths
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "panic"))
    {
        let Some(launch) = panic.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let prefix = format!("{launch}-");
        if !paths.iter().any(|p| {
            p.extension().is_some_and(|e| e == "jsonl")
                && p.file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.starts_with(&prefix))
        }) {
            let _ = fs::remove_file(panic);
        }
    }
    Ok(())
}
type Scan = (Vec<Record>, BTreeMap<String, u64>, Vec<String>);

fn scan(dir: &Path) -> io::Result<Scan> {
    scan_unlocked(dir)
}
fn scan_unlocked(dir: &Path) -> io::Result<Scan> {
    let cutoffs = match fs::read(dir.join("cutoffs.json")) {
        Ok(data) => serde_json::from_slice(&data).map_err(io::Error::other)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => BTreeMap::new(),
        Err(e) => return Err(e),
    };
    let mut records = Vec::new();
    let mut incomplete = Vec::new();
    if dir.join("history-pruned").exists() {
        incomplete.push("Older retained segments were pruned".into());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path
            .extension()
            .is_none_or(|ext| ext != "jsonl" && ext != "panic")
        {
            continue;
        }
        if path.extension().is_some_and(|ext| ext == "jsonl")
            && fs::metadata(&path).is_ok_and(|metadata| metadata.len() > SEGMENT_LIMIT)
        {
            incomplete.push(format!(
                "{} exceeded its normal segment size",
                path.display()
            ));
        }
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(e) => {
                incomplete.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        let mut reader = BufReader::new(file);
        loop {
            let mut line = Vec::new();
            match reader
                .by_ref()
                .take((RECORD_LIMIT + 1) as u64)
                .read_until(b'\n', &mut line)
            {
                Ok(0) => break,
                Ok(_) if line.len() <= RECORD_LIMIT && line.last() == Some(&b'\n') => {
                    match serde_json::from_slice::<Record>(&line) {
                        Ok(record) => records.push(record),
                        Err(_) => incomplete.push(format!("invalid record in {}", path.display())),
                    }
                }
                Ok(_) => {
                    incomplete.push(format!(
                        "incomplete or oversized record in {}",
                        path.display()
                    ));
                    break;
                }
                Err(e) => {
                    incomplete.push(format!("{}: {e}", path.display()));
                    break;
                }
            }
        }
    }
    Ok((records, cutoffs, incomplete))
}
fn make_snapshot(
    records: &[Record],
    cutoffs: &BTreeMap<String, u64>,
    incomplete: Vec<String>,
    current_launch: &str,
) -> Snapshot {
    let mut ordered = records.to_vec();
    ordered.sort_by(|a, b| {
        a.launch_id
            .cmp(&b.launch_id)
            .then(a.sequence.cmp(&b.sequence))
    });
    ordered.dedup_by(|a, b| a.launch_id == b.launch_id && a.sequence == b.sequence);
    let mut runs: BTreeMap<String, RunSnapshot> = BTreeMap::new();
    let mut context_parts: HashMap<(String, String), (usize, Vec<String>)> = HashMap::new();
    for record in ordered {
        let run = runs
            .entry(record.launch_id.clone())
            .or_insert_with(|| RunSnapshot {
                launch_id: record.launch_id.clone(),
                started_at_ms: record.timestamp_ms,
                context: json!({}),
                ..Default::default()
            });
        run.started_at_ms = run.started_at_ms.min(record.timestamp_ms);
        if record.event == "context.updated" {
            let Some(section) = record.fields.get("section").and_then(Value::as_str) else {
                continue;
            };
            let part = record
                .fields
                .get("part")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            let total = record
                .fields
                .get("parts")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            if total == 0 || total > 10000 || part >= total {
                continue;
            }
            let key = (record.launch_id.clone(), section.to_owned());
            if part == 0 {
                context_parts.insert(key.clone(), (total, Vec::new()));
            }
            if let Some((expected, chunks)) = context_parts.get_mut(&key) {
                if *expected == total && chunks.len() == part {
                    chunks.push(
                        record
                            .fields
                            .get("json")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                    );
                    if chunks.len() == total {
                        if let Ok(value) = serde_json::from_str(&chunks.concat()) {
                            run.context[section] = value;
                        }
                    }
                }
            }
            continue;
        }
        if record.sequence > *cutoffs.get(&record.launch_id).unwrap_or(&0) {
            if record.event == "log.history_incomplete" {
                run.incomplete.push(record.message.clone());
            }
            if record.fields.get("truncated").and_then(Value::as_bool) == Some(true) {
                run.incomplete.push("A record was truncated".into());
            }
            run.events.push(record);
        }
    }
    Snapshot {
        runs: runs
            .into_values()
            .filter(|run| !run.events.is_empty() || run.launch_id == current_launch)
            .collect(),
        incomplete,
    }
}

pub(super) fn install_panic_hook(handle: &Handle) {
    let dir = &handle
        .shared
        .status
        .lock()
        .expect("support status")
        .directory
        .clone();
    let launch = &handle.shared.launch;
    let path = dir.join(format!("{launch}.panic"));
    let Ok(file) = OpenOptions::new().write(true).create_new(true).open(path) else {
        return;
    };
    let fired = AtomicBool::new(false);
    let shared = handle.shared.clone();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if !fired.swap(true, Ordering::Relaxed) {
            let trace = std::backtrace::Backtrace::force_capture();
            let record = bounded(Record {
                version: 1,
                launch_id: shared.launch.clone(),
                sequence: shared.sequence.fetch_add(1, Ordering::Relaxed),
                timestamp_ms: now_ms(),
                elapsed_ms: shared.start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                thread: format!("{:?}", std::thread::current()),
                level: Level::Error,
                event: "panic".into(),
                message: info.to_string(),
                fields: json!({"thread":format!("{:?}",std::thread::current()), "capture":"panic hook", "backtrace":format!("{trace:#}")}),
            });
            if let Ok(mut bytes) = serde_json::to_vec(&record) {
                bytes.push(b'\n');
                let _ = (&file).write_all(&bytes);
            }
        }
        previous(info);
    }));
}

#[cfg(test)]
mod queue_tests {
    use super::*;
    #[test]
    fn outer_failure_context_uses_error_reserve() {
        const CHILD: &str = "FOVEONIC_SUPPORT_RESERVE_CHILD";
        if let Some(dir) = std::env::var_os(CHILD) {
            let mut guard = super::super::initialize(Config {
                directory: dir.into(),
                application: "test".into(),
                version: "1".into(),
                initial_context: Value::Null,
            });
            let handle = guard.handle();
            handle.shared.ingest.lock().unwrap().normal = NORMAL_COUNT;
            let source: Box<dyn std::error::Error + Send + Sync> =
                Box::new(io::Error::from_raw_os_error(13));
            let wrapped = super::super::reported_error(
                "write",
                source,
                super::super::OperationContext::default(),
            );
            let report = super::super::report_error(
                "export",
                wrapped.as_ref(),
                super::super::OperationContext {
                    settings: Some(json!({"frozen":"export-settings"})),
                    ..Default::default()
                },
            );
            handle.shared.ingest.lock().unwrap().normal = 0;
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let snapshot = handle.snapshot();
                let events: Vec<_> = snapshot.runs.iter().flat_map(|run| &run.events).collect();
                if events.iter().any(|event| {
                    event.event == "operation.failed" && event.fields["id"] == report.id
                }) && events.iter().any(|event| {
                    event.event == "operation.context"
                        && event.fields["failure_id"] == report.id
                        && event.fields["operation"]["settings"]["frozen"] == "export-settings"
                }) {
                    break;
                }
                assert!(Instant::now() < deadline);
                thread::yield_now();
            }
            assert_eq!(
                guard.shutdown(Duration::from_secs(3)),
                ShutdownStatus::Drained
            );
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "support::store::queue_tests::outer_failure_context_uses_error_reserve",
            ])
            .env(CHILD, dir.path())
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn startup_contention_retries_and_persists_buffered_events() {
        let dir = tempfile::tempdir().unwrap();
        let lock = maintenance(dir.path()).unwrap();
        let mut guard = start(Config {
            directory: dir.path().into(),
            application: "test".into(),
            version: "1".into(),
            initial_context: json!({"startup":"initial"}),
        });
        let handle = guard.handle();
        handle.event(
            Level::Error,
            "startup.failed",
            "preserved error",
            json!({"cause":"device"}),
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        while !handle
            .snapshot()
            .copy_report(super::super::VisibleSelection::All)
            .contains("preserved error")
        {
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        let clear = handle.request_clear();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(result) = clear.try_result() {
                assert!(result.is_err());
                break;
            }
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        assert!(handle
            .snapshot()
            .copy_report(super::super::VisibleSelection::All)
            .contains("preserved error"));
        drop(lock);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let (records, _, _) = scan(dir.path()).unwrap();
            if records
                .iter()
                .any(|record| record.event == "startup.failed")
            {
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        let (records, _, _) = scan(dir.path()).unwrap();
        assert!(records
            .iter()
            .any(|record| record.event == "launch.started"));
        assert!(records
            .iter()
            .any(|record| record.event == "context.updated"));
        assert!(handle.status().persistence_error.is_none());
        assert_eq!(
            guard.shutdown(Duration::from_secs(3)),
            ShutdownStatus::Drained
        );
    }

    #[test]
    fn overflow_is_reported_when_capacity_returns() {
        let dir = tempfile::tempdir().unwrap();
        let mut guard = start(Config {
            directory: dir.path().into(),
            application: "test".into(),
            version: "1".into(),
            initial_context: Value::Null,
        });
        let h = guard.handle();
        while h.shared.ingest.lock().unwrap().normal != 0 {
            thread::yield_now();
        }
        h.shared.ingest.lock().unwrap().normal = NORMAL_COUNT;
        h.event(Level::Info, "dropped", "overflow", Value::Null);
        assert_eq!(h.shared.ingest.lock().unwrap().dropped, 1);
        h.shared.ingest.lock().unwrap().normal = 0;
        h.event(Level::Info, "accepted", "resume", Value::Null);
        let end = Instant::now() + Duration::from_secs(3);
        while !h
            .snapshot()
            .copy_report(super::super::VisibleSelection::All)
            .contains("queue was full")
        {
            assert!(Instant::now() < end);
            thread::yield_now();
        }
        assert_eq!(
            guard.shutdown(Duration::from_secs(3)),
            ShutdownStatus::Drained
        );
    }

    #[test]
    fn rotation_retries_after_maintenance_contention() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            directory: dir.path().into(),
            application: "test".into(),
            version: "1".into(),
            initial_context: Value::Null,
        };
        let mut guard = start(config.clone());
        let handle = guard.handle();
        let mut writer = Writer::new(handle.shared.clone(), &config);
        writer.index = 100;
        writer.open_initial();
        writer.segment.as_mut().unwrap().records = SEGMENT_RECORDS;
        let lock = maintenance(dir.path()).unwrap();
        let mut ingest = handle.shared.ingest.lock().unwrap();
        let first = handle.record(
            &mut ingest,
            Level::Info,
            "contended",
            "still written",
            Value::Null,
        );
        drop(ingest);
        writer.accept(first);
        assert!(writer.segment.is_some());
        assert!(writer.rotation_retry.is_some());
        assert!(writer
            .incomplete
            .iter()
            .any(|notice| notice.contains("rotation was delayed")));
        drop(lock);
        writer.rotation_retry = Some(Instant::now());
        writer.retry_rotation();
        assert!(writer.rotation_retry.is_none());
        let mut ingest = handle.shared.ingest.lock().unwrap();
        let second = handle.record(
            &mut ingest,
            Level::Info,
            "after",
            "rotation resumed",
            Value::Null,
        );
        drop(ingest);
        writer.accept(second);
        assert!(writer.rotation_retry.is_none());
        let (records, _, _) = scan(dir.path()).unwrap();
        assert!(records.iter().any(|record| record.event == "contended"));
        assert!(records.iter().any(|record| record.event == "after"));
        writer.segment.take();
        assert_eq!(
            guard.shutdown(Duration::from_secs(3)),
            ShutdownStatus::Drained
        );
    }
}

#[cfg(test)]
mod fallback_tests {
    use super::*;
    #[test]
    fn in_memory_cap_keeps_launch_context() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-a-directory");
        fs::write(&path, b"x").unwrap();
        let config = Config {
            directory: path,
            application: "test".into(),
            version: "1".into(),
            initial_context: json!({"gpu":"pending"}),
        };
        let mut guard = start(config.clone());
        let h = guard.handle();
        let mut writer = Writer::new(h.shared.clone(), &config);
        let mut ingest = h.shared.ingest.lock().unwrap();
        let header = h.record(
            &mut ingest,
            Level::Info,
            "launch.started",
            "started",
            Value::Null,
        );
        let context = context_records(
            &h,
            &mut ingest,
            "settings",
            &json!({"exposure":0.25}).to_string(),
        );
        writer.headers.push(header);
        writer.contexts.insert("settings".into(), context);
        for _ in 0..4000 {
            writer
                .records
                .push(h.record(&mut ingest, Level::Info, "work", "working", Value::Null));
        }
        let last = h.record(&mut ingest, Level::Info, "last", "latest", Value::Null);
        drop(ingest);
        writer.accept(last);
        let snapshot = h.snapshot();
        let run = snapshot
            .runs
            .iter()
            .find(|run| run.events.iter().any(|event| event.event == "last"))
            .unwrap();
        assert_eq!(run.context["settings"]["exposure"], 0.25);
        assert!(run
            .events
            .iter()
            .any(|event| event.event == "launch.started"));
        assert_eq!(
            guard.shutdown(Duration::from_secs(3)),
            ShutdownStatus::Drained
        );
    }

    #[test]
    fn in_memory_cap_evicts_whole_historical_runs() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            directory: dir.path().into(),
            application: "test".into(),
            version: "1".into(),
            initial_context: Value::Null,
        };
        let mut guard = start(config.clone());
        let handle = guard.handle();
        let mut writer = Writer::new(handle.shared.clone(), &config);
        let mut ingest = handle.shared.ingest.lock().unwrap();
        for (launch, count) in [("old", 2), ("retained", 3998)] {
            let mut header = handle.record(
                &mut ingest,
                Level::Info,
                "launch.started",
                "start",
                Value::Null,
            );
            header.launch_id = launch.into();
            writer.records.push(header);
            let mut context = context_records(
                &handle,
                &mut ingest,
                "settings",
                &json!({"owner":launch}).to_string(),
            )
            .remove(0);
            context.launch_id = launch.into();
            writer.records.push(context);
            for _ in 0..count {
                let mut record =
                    handle.record(&mut ingest, Level::Info, "work", "visible", Value::Null);
                record.launch_id = launch.into();
                writer.records.push(record);
            }
        }
        drop(ingest);
        writer.records_bytes = writer.records.iter().map(record_size).sum();
        writer.bound_records();
        writer.publish();
        let snapshot = handle.snapshot();
        assert!(!snapshot.runs.iter().any(|run| run.launch_id == "old"));
        let retained = snapshot
            .runs
            .iter()
            .find(|run| run.launch_id == "retained")
            .unwrap();
        assert_eq!(retained.context["settings"]["owner"], "retained");
        assert!(retained.events.iter().any(|event| event.event == "work"));
        assert_eq!(
            guard.shutdown(Duration::from_secs(3)),
            ShutdownStatus::Drained
        );
    }
}
