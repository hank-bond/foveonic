//! Captures typed failure context, causes and boundary backtraces.

use serde::Serialize;
use serde_json::Value;
use std::{
    backtrace::Backtrace,
    error::Error,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Debug, Default, Serialize)]
pub struct OperationContext {
    pub operation_id: Option<String>,
    pub parent_id: Option<String>,
    pub component: Option<String>,
    pub phase: Option<String>,
    pub source: Option<String>,
    pub destination: Option<String>,
    pub revision: Option<String>,
    pub purpose: Option<String>,
    pub settings: Option<Value>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FailureCause {
    pub message: String,
    pub io_kind: Option<String>,
    pub os_code: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct FailureReport {
    pub id: String,
    pub message: String,
    pub causes: Vec<FailureCause>,
    pub boundary: String,
    pub caller: String,
    pub thread: String,
    pub backtrace_status: String,
    pub backtrace: String,
    pub operation: OperationContext,
}

#[derive(Debug)]
pub struct ReportedError {
    pub report: std::sync::Arc<FailureReport>,
    pub(super) source: Box<dyn Error + Send + Sync>,
}
impl std::fmt::Display for ReportedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.source.fmt(f)
    }
}
impl Error for ReportedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

pub(super) fn capture(
    boundary: &str,
    error: &(dyn Error + 'static),
    operation: OperationContext,
    caller: &std::panic::Location<'_>,
) -> FailureReport {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut causes = Vec::new();
    let mut next = Some(error);
    while let Some(cause) = next {
        if causes.len() == 32 {
            causes.push(FailureCause {
                message: "Further causes omitted (chain limit)".into(),
                io_kind: None,
                os_code: None,
            });
            break;
        }
        let io = cause.downcast_ref::<std::io::Error>();
        causes.push(FailureCause {
            message: cause.to_string(),
            io_kind: io.map(|e| format!("{:?}", e.kind())),
            os_code: io.and_then(std::io::Error::raw_os_error),
        });
        next = cause.source();
    }
    let backtrace = Backtrace::force_capture();
    FailureReport {
        id: format!(
            "{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ),
        message: error.to_string(),
        causes,
        boundary: boundary.into(),
        caller: format!("{}:{}:{}", caller.file(), caller.line(), caller.column()),
        thread: format!("{:?}", std::thread::current()),
        backtrace_status: format!("{:?}", backtrace.status()),
        backtrace: format!("{backtrace:#}"),
        operation,
    }
}
