//! Initializes executable-owned support reporting and collects static context without delaying startup.

use super::{self as support, Config, Guard, Level, OperationContext};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::mpsc, time::Duration};

pub struct Application {
    guard: Guard,
    system: Option<mpsc::Receiver<()>>,
}

pub fn start_application(mut config: Config) -> Application {
    config.initial_context["timezone_offset_seconds"] =
        json!(time::UtcOffset::current_local_offset()
            .ok()
            .map(|offset| offset.whole_seconds()));
    let guard = support::initialize(config);
    if let Err(error) = super::subscriber::install() {
        support::report_error(
            "support.subscriber",
            error.as_ref(),
            OperationContext::default(),
        );
    }
    support::event(
        Level::Info,
        "startup.entry",
        "Application launched",
        json!({"arguments":std::env::args_os().skip(1).map(|argument|argument.to_string_lossy().into_owned()).collect::<Vec<_>>()}),
    );
    let (ready, system) = mpsc::sync_channel(1);
    let system = match std::thread::Builder::new()
        .name("support-system".into())
        .spawn(move || {
            support::update_context("system", super::system::collect());
            let _ = ready.send(());
        }) {
        Ok(_) => Some(system),
        Err(error) => {
            support::report_error("support.system_worker", &error, OperationContext::default());
            None
        }
    };
    Application { guard, system }
}

impl Application {
    pub fn log_directory(&self) -> PathBuf {
        self.guard.log_directory()
    }
    pub fn shutdown(mut self, success: bool) {
        if self
            .system
            .take()
            .is_some_and(|ready| ready.recv_timeout(Duration::from_secs(1)).is_err())
        {
            support::event(
                Level::Warning,
                "support.inventory_incomplete",
                "System inventory did not finish before shutdown",
                json!({"history_incomplete":true}),
            );
        }
        support::event(
            Level::Info,
            "application.exit",
            "Application exiting",
            json!({"success":success}),
        );
        self.guard.shutdown(Duration::from_secs(2));
    }
}

pub fn build_identity() -> Value {
    json!({"version":env!("CARGO_PKG_VERSION"),"revision":env!("FOVEONIC_BUILD_REVISION"),
        "dirty":env!("FOVEONIC_BUILD_DIRTY"),"target":env!("FOVEONIC_BUILD_TARGET"),"profile":env!("FOVEONIC_BUILD_PROFILE")})
}

pub fn default_directory(app: &str) -> PathBuf {
    if let Some(directory) = std::env::var_os("FOVEONIC_SUPPORT_LOG_DIR") {
        return PathBuf::from(directory);
    }
    match std::env::consts::OS {
        "windows" => std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(app)
            .join("Logs"),
        "macos" => std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("Library/Logs")
            .join(app),
        _ => std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(std::env::temp_dir)
                    .join(".local/state")
            })
            .join(app)
            .join("logs"),
    }
}
