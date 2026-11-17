//! Records the source identity needed to interpret support reports from released builds.

fn main() {
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    emit(&root);
}

fn emit(root: &std::path::Path) {
    use std::process::Command;
    let repository = root.join("../..");
    let run_git = |args: &[&str]| {
        Command::new("git")
            // Status must not refresh the watched index during the build itself.
            .env("GIT_OPTIONAL_LOCKS", "0")
            .args(args)
            .current_dir(&repository)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let top = run_git(&["rev-parse", "--show-toplevel"])
        .and_then(|path| std::fs::canonicalize(path).ok());
    let same_repository = repository
        .canonicalize()
        .ok()
        .is_some_and(|expected| top == Some(expected));
    // An extracted archive may live inside an unrelated checkout.
    let git = |args: &[&str]| same_repository.then(|| run_git(args)).flatten();
    println!("cargo:rerun-if-env-changed=FOVEONIC_BUILD_REVISION");
    let marker = repository.join(".git");
    if marker.is_file() {
        println!("cargo:rerun-if-changed={}", marker.display());
    }
    for name in ["HEAD", "index", "refs/heads", "packed-refs"] {
        if let Some(path) = git(&["rev-parse", "--git-path", name]) {
            let path = std::path::PathBuf::from(path);
            let path = if path.is_absolute() {
                path
            } else {
                repository.join(path)
            };
            // Missing watches make Cargo rerun forever, including in source archives.
            if path.exists() {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }
    for path in [
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
