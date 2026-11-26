//! Finds application versions, executables and the preview library through Cargo's machine-readable output.

use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Deserialize)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub version: String,
}

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
}

fn command(root: &Path) -> Command {
    let mut command = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    command.current_dir(root).stderr(Stdio::inherit());
    command
}

pub fn application(root: &Path, offline: bool) -> Result<Package> {
    let mut command = command(root);
    command.args(["metadata", "--locked", "--no-deps", "--format-version", "1"]);
    if offline {
        command.arg("--offline");
    }
    let output = command.output().context("Cannot run cargo metadata")?;
    ensure!(
        output.status.success(),
        "Cargo metadata failed: {}",
        output.status
    );
    let metadata: Metadata = serde_json::from_slice(&output.stdout)?;
    let core = metadata
        .packages
        .iter()
        .find(|p| p.name == "foveonic")
        .context("Cargo metadata is missing foveonic")?;
    let app = metadata
        .packages
        .iter()
        .find(|p| p.name == "foveonic-gui")
        .context("Cargo metadata is missing foveonic-gui")?;
    ensure!(
        core.version == app.version,
        "Application crate versions differ"
    );
    metadata
        .packages
        .into_iter()
        .find(|p| p.name == "foveonic-gui")
        .context("Cargo metadata is missing foveonic-gui")
}

/// Compiled inputs for one macOS bundle.
pub struct Artifacts {
    pub gui: PathBuf,
    /// Static library linked into the Quick Look preview extension.
    pub preview_library: PathBuf,
}

const PREVIEW_LIBRARY_TARGET: &str = "foveonic_quicklook";

pub fn build(root: &Path, package: &str, debug: bool, offline: bool) -> Result<Artifacts> {
    let mut command = command(root);
    command
        .args([
            "build",
            "--locked",
            "-p",
            "foveonic-gui",
            "--bin",
            "foveonic-gui",
            "-p",
            "foveonic-quicklook",
            "--lib",
            "--message-format=json-render-diagnostics",
        ])
        .env(
            "MACOSX_DEPLOYMENT_TARGET",
            crate::bundle::MINIMUM_MACOS_VERSION,
        );
    if !debug {
        command.arg("--release");
    }
    if offline {
        command.arg("--offline");
    }
    let output = command.output().context("Cannot run cargo build")?;
    ensure!(
        output.status.success(),
        "App build failed: {}",
        output.status
    );
    artifacts(&output.stdout, package)
}

fn artifacts(report: &[u8], package: &str) -> Result<Artifacts> {
    let mut gui = None;
    let mut preview_library = None;
    for line in report
        .split(|&byte| byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let message: serde_json::Value =
            serde_json::from_slice(line).context("Invalid Cargo compiler message")?;
        if message["reason"] != "compiler-artifact" {
            continue;
        }
        let kind = |name: &str| {
            message["target"]["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == name))
        };
        if message["package_id"] == package
            && message["target"]["name"] == "foveonic-gui"
            && kind("bin")
        {
            if let Some(path) = message["executable"].as_str() {
                ensure!(gui.is_none(), "Cargo reported multiple GUI executables");
                gui = Some(PathBuf::from(path));
            }
        } else if message["target"]["name"] == PREVIEW_LIBRARY_TARGET && kind("staticlib") {
            let library = message["filenames"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|name| name.as_str())
                .find(|name| name.ends_with(".a"));
            if let Some(path) = library {
                ensure!(
                    preview_library.is_none(),
                    "Cargo reported multiple preview libraries"
                );
                preview_library = Some(PathBuf::from(path));
            }
        }
    }
    Ok(Artifacts {
        gui: gui.context("Cargo did not report the GUI executable")?,
        preview_library: preview_library.context("Cargo did not report the preview library")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn artifact_uses_reported_path_including_custom_targets_and_fresh_builds() {
        for profile in ["debug", "release"] {
            let path = format!("/custom target/{profile}/foveonic-gui");
            let message = json!({"reason": "compiler-artifact", "package_id": "gui-id",
                "target": {"name": "foveonic-gui", "kind": ["bin"]},
                "executable": path, "fresh": true});
            let library_path = format!("/custom target/{profile}/libfoveonic_quicklook.a");
            let library = json!({"reason": "compiler-artifact", "package_id": "ql-id",
                "target": {"name": "foveonic_quicklook", "kind": ["staticlib", "rlib"]},
                "filenames": [format!("/custom target/{profile}/libfoveonic_quicklook.rlib"), library_path],
                "executable": null, "fresh": true});
            let report =
                format!("{{\"reason\":\"build-script-executed\"}}\n{message}\n{library}\n");
            let built = artifacts(report.as_bytes(), "gui-id").unwrap();
            assert_eq!(built.gui, PathBuf::from(path));
            assert_eq!(built.preview_library, PathBuf::from(library_path));
            assert!(artifacts(report.as_bytes(), "other-id").is_err());
            assert!(artifacts(format!("{message}\n").as_bytes(), "gui-id").is_err());
            assert!(artifacts(
                format!("{message}\n{message}\n{library}").as_bytes(),
                "gui-id"
            )
            .is_err());
        }
    }

    #[test]
    fn missing_executable_and_invalid_messages_fail() {
        assert!(artifacts(
            b"{\"reason\":\"build-finished\",\"success\":true}\n",
            "gui-id"
        )
        .is_err());
        assert!(artifacts(b"invalid JSON", "gui-id").is_err());
    }
}
