//! Finds application versions and executable artifacts through Cargo's machine-readable output.

use anyhow::{bail, ensure, Context, Result};
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

pub fn build(root: &Path, package: &str, debug: bool, offline: bool) -> Result<PathBuf> {
    let mut command = command(root);
    command
        .args([
            "build",
            "--locked",
            "-p",
            "foveonic-gui",
            "--bin",
            "foveonic-gui",
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
        "GUI build failed: {}",
        output.status
    );
    artifact(&output.stdout, package)
}

fn artifact(report: &[u8], package: &str) -> Result<PathBuf> {
    let mut binary = None;
    for line in report
        .split(|&byte| byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let message: serde_json::Value =
            serde_json::from_slice(line).context("Invalid Cargo compiler message")?;
        if message["reason"] == "compiler-artifact"
            && message["package_id"] == package
            && message["target"]["name"] == "foveonic-gui"
            && message["target"]["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"))
        {
            if let Some(path) = message["executable"].as_str() {
                ensure!(binary.is_none(), "Cargo reported multiple GUI executables");
                binary = Some(PathBuf::from(path));
            }
        }
    }
    match binary {
        Some(binary) => Ok(binary),
        None => bail!("Cargo did not report the GUI executable"),
    }
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
            let report = format!("{{\"reason\":\"build-script-executed\"}}\n{message}\n");
            assert_eq!(
                artifact(report.as_bytes(), "gui-id").unwrap(),
                PathBuf::from(path)
            );
            assert!(artifact(report.as_bytes(), "other-id").is_err());
            assert!(artifact(format!("{message}\n{message}").as_bytes(), "gui-id").is_err());
        }
    }

    #[test]
    fn missing_executable_and_invalid_messages_fail() {
        assert!(artifact(
            b"{\"reason\":\"build-finished\",\"success\":true}\n",
            "gui-id"
        )
        .is_err());
        assert!(artifact(b"invalid JSON", "gui-id").is_err());
    }
}
