//! Starts support reporting before graphics and forwards dependency warnings into the same history.

use foveonic::support::{self, Config, OperationContext};
use serde_json::json;
use std::path::PathBuf;

static STARTUP_FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn startup_failure(message: &str) {
    if !STARTUP_FAILED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        foveonic::support::event(
            foveonic::support::Level::Error,
            "startup.failed",
            message,
            json!({"capture_site":"startup result delivery","origin_details":"See worker failure report or panic record"}),
        );
    }
}
pub fn startup_failed() -> bool {
    STARTUP_FAILED.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn initialize() -> foveonic::support::Application {
    foveonic::support::start_application(Config {
        directory: directory(),
        application: crate::preferences::APP_NAME.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        initial_context: json!({"application_kind":"gui","build":foveonic::support::build_identity(),
            "system":{"status":"pending","os_family":std::env::consts::OS,"architecture":std::env::consts::ARCH},
            "gpu":{"status":"not selected"},"preferences":{"status":"not loaded"}}),
    })
}

pub fn operation_id(component: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!("{component}-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// Logging must not make a valid native filename fail JSON serialization.
pub fn path(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

pub fn directory() -> PathBuf {
    // Automation keeps reports with its isolated, explicitly requested evidence.
    if let Some(path) = std::env::var_os("FOVEONIC_GUI_SMOKE_DIR") {
        return PathBuf::from(path).join("support-logs");
    }
    foveonic::support::default_directory(crate::preferences::APP_ID)
}

pub fn selected_gpu(state: &eframe::egui_wgpu::RenderState) {
    let info = state.adapter.get_info();
    support::update_context(
        "gpu",
        json!({"status":"selected", "name":info.name,
        "vendor":info.vendor,"device":info.device,"device_type":format!("{:?}",info.device_type),
        "backend":format!("{:?}",info.backend),"driver":info.driver,"driver_info":info.driver_info,
        "features":format!("{:?}",state.device.features()), "limits":format!("{:?}",state.device.limits()),
        "shader_compiler":if cfg!(target_os="windows") {"DXC (packaged dynamic libraries)"} else {"backend default"},
        "capacity_source":"See system.gpu_inventory. API allocation limits are separate from physical capacity."}),
    );
    state.device.set_device_lost_callback(|reason, message| {
        if reason==eframe::wgpu::DeviceLostReason::Destroyed {return;}
        support::report_error("gpu.device_lost_callback",&std::io::Error::other(message),OperationContext {
            component:Some("GPU device".into()),phase:Some("device loss callback".into()),
            settings:Some(json!({"reason":format!("{reason:?}"),"recent_stages":foveonic::recent_gpu_stages()})),
            ..Default::default()
        });
    });
}

pub fn shutdown(runtime: foveonic::support::Application, success: bool) {
    runtime.shutdown(success);
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn logging_a_non_utf8_path_keeps_the_report_serializable() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let source = PathBuf::from(OsString::from_vec(b"photo-\xff.x3f".to_vec()));
        let report = json!({"source":path(&source),"operation":"open"});
        assert_eq!(report["source"], "photo-\u{fffd}.x3f");
        assert!(serde_json::to_vec(&report).is_ok());
    }
}
