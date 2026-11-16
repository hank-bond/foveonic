//! Records the source identity needed to interpret support reports from released builds.

fn main() {
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    emit(&root);
}

fn emit(root: &std::path::Path) {
    use std::process::Command;
    let repository = root.join("../..");
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(&repository)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    println!("cargo:rerun-if-env-changed=FOVEONIC_BUILD_REVISION");
    for path in [
        ".git/HEAD",
        ".git/index",
        ".git/refs/heads",
        "crates/foveonic/src",
        "crates/foveonic-gui/src",
        "Cargo.lock",
    ] {
        println!("cargo:rerun-if-changed={}", repository.join(path).display());
    }
    let revision = std::env::var("FOVEONIC_BUILD_REVISION")
        .ok()
        .or_else(|| git(&["rev-parse", "HEAD"]))
        .unwrap_or_else(|| "unknown".into());
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"])
        .map(|status| (!status.is_empty()).to_string())
        .unwrap_or_else(|| "unknown".into());
    for (key, value) in [
        ("REVISION", revision),
        ("DIRTY", dirty),
        ("TARGET", std::env::var("TARGET").unwrap()),
        ("PROFILE", std::env::var("PROFILE").unwrap()),
    ] {
        println!("cargo:rustc-env=FOVEONIC_BUILD_{key}={value}");
    }
}
