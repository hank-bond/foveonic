//! Exposes explicit Cargo commands for native bundles, example syncing, screenshots, and release notes.

mod bundle;
mod cargo;
mod examples;
mod release;
mod screenshot;

use anyhow::{ensure, Context, Result};
use clap::{Args, Parser, Subcommand};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Parser)]
#[command(about = "Build Foveonic apps and explicitly maintain release and documentation assets")]
struct Cli {
    #[command(subcommand)]
    command: Task,
}

#[derive(Subcommand)]
enum Task {
    /// Build a native macOS app without launching or installing it.
    Bundle(Build),
    /// Build and launch the native macOS app. Pass GUI arguments after --.
    Run {
        #[command(flatten)]
        build: Build,
        #[arg(last = true)]
        arguments: Vec<OsString>,
    },
    /// Sync official photographs into ignored local storage.
    Examples {
        #[command(subcommand)]
        command: ExamplesTask,
    },
    /// Refresh the README screenshot using an isolated native GUI session.
    Screenshot(Sources),
    /// Print only the tagged version's changelog section for a draft release.
    ReleaseNotes {
        #[arg(long)]
        tag: String,
    },
}

#[derive(Subcommand)]
enum ExamplesTask {
    /// Download missing X3Fs, or validate the last synced collection offline.
    Sync(Sources),
}

#[derive(Args)]
struct Sources {
    /// Use a different local example cache.
    #[arg(long)]
    source_dir: Option<PathBuf>,
    /// Use cached sources and avoid network access.
    #[arg(long)]
    offline: bool,
}

#[derive(Args)]
struct Build {
    /// Use the debug profile instead of release.
    #[arg(long)]
    debug: bool,
    /// Build without accessing the network.
    #[arg(long)]
    offline: bool,
    /// Require a release build with this tag matching the application version.
    #[arg(long, conflicts_with = "debug")]
    tag: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .context("Cannot locate workspace root")?;
    match cli.command {
        Task::Examples {
            command: ExamplesTask::Sync(sources),
        } => {
            let directory = examples::directory(root, sources.source_dir);
            examples::sync(&directory, sources.offline)?;
        }
        Task::Screenshot(sources) => {
            ensure!(
                cfg!(target_os = "macos"),
                "README screenshots currently require a native macOS graphical/GPU session"
            );
            let directory = examples::directory(root, sources.source_dir);
            examples::sync(&directory, sources.offline)?;
            let source = screenshot::source(root, &directory)?;
            let app = build_app(
                root,
                &Build {
                    debug: false,
                    offline: sources.offline,
                    tag: None,
                },
            )?;
            screenshot::capture(root, &app.join("Contents/MacOS/foveonic-gui"), &source)?;
        }
        Task::ReleaseNotes { tag } => {
            let app = cargo::application(root, false)?;
            release::check_tag(&tag, &app.version)?;
            let changelog = std::fs::read_to_string(root.join("CHANGELOG.md"))?;
            println!("{}", release::notes(&changelog, &app.version)?);
        }
        Task::Bundle(build) => {
            println!("{}", build_app(root, &build)?.display());
        }
        Task::Run { build, arguments } => {
            let app = build_app(root, &build)?;
            println!("{}", app.display());
            let cwd = std::env::current_dir()?;
            let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
            let status = Command::new("open")
                .args(["-n"])
                .arg(app)
                .arg("--args")
                .args(bundle::launch_arguments(&arguments, &cwd, home.as_deref()))
                .status()
                .context("Cannot launch app with macOS LaunchServices")?;
            ensure!(status.success(), "App launch failed: {status}");
        }
    }
    Ok(())
}

fn build_app(root: &Path, build: &Build) -> Result<std::path::PathBuf> {
    ensure!(
        cfg!(target_os = "macos"),
        "App packaging requires macOS and Xcode 26 or later"
    );
    let app = cargo::application(root, build.offline)?;
    if let Some(tag) = &build.tag {
        release::check_tag(tag, &app.version)?;
    }
    let binary = cargo::build(root, &app.id, build.debug, build.offline)?;
    let compiled = tempfile::Builder::new()
        .prefix("foveonic-icons-")
        .tempdir()?;
    bundle::compile_icon(root, compiled.path())?;
    bundle::assemble(&binary, compiled.path(), &app.version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_is_consistent_and_rejects_debug_release_tags() {
        Cli::command().debug_assert();
        assert!(Cli::try_parse_from(["xtask", "bundle", "--debug", "--tag", "v0.1.0"]).is_err());
        assert!(Cli::try_parse_from(["xtask", "release-notes"]).is_err());
        assert!(Cli::try_parse_from(["xtask", "bundle", "--build-only"]).is_err());
    }

    #[test]
    fn example_sync_and_screenshot_share_offline_and_source_directory_options() {
        for args in [
            vec![
                "xtask",
                "examples",
                "sync",
                "--offline",
                "--source-dir",
                "photos with spaces",
            ],
            vec![
                "xtask",
                "screenshot",
                "--offline",
                "--source-dir",
                "photos with spaces",
            ],
        ] {
            let sources = match Cli::try_parse_from(args).unwrap().command {
                Task::Examples {
                    command: ExamplesTask::Sync(sources),
                }
                | Task::Screenshot(sources) => sources,
                _ => panic!("Expected example source options"),
            };
            assert!(sources.offline);
            assert_eq!(
                sources.source_dir,
                Some(PathBuf::from("photos with spaces"))
            );
        }
        let root = Path::new("workspace");
        assert_eq!(
            examples::directory(root, None),
            root.join(".local/example-photographs")
        );
        assert_eq!(
            examples::directory(root, Some("selected".into())),
            PathBuf::from("selected")
        );
    }

    #[test]
    fn run_forwards_arguments_after_separator_without_parsing_gui_options() {
        let cli = Cli::try_parse_from([
            "xtask",
            "run",
            "--debug",
            "--offline",
            "--",
            "folder with spaces/photo.x3f",
            "--wb",
            "Shade",
            "--mode",
            "Standard",
        ])
        .unwrap();
        let Task::Run { build, arguments } = cli.command else {
            panic!("Expected run command");
        };
        assert!(build.debug && build.offline);
        assert_eq!(
            arguments,
            [
                "folder with spaces/photo.x3f",
                "--wb",
                "Shade",
                "--mode",
                "Standard"
            ]
            .map(OsString::from)
        );
    }
}
