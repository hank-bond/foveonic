//! Runs documentation index generation or read-only validation for a repository.

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "usage: docs-index [--check] [--root PATH]";

fn parse_command_options() -> Result<(PathBuf, bool), String> {
    let mut root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the tool lives in crates/docs-index")
        .to_owned();
    let mut is_check_mode = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--check") => is_check_mode = true,
            Some("--root") => {
                root = args.next().map(PathBuf::from).ok_or(USAGE)?;
            }
            _ => return Err(USAGE.to_owned()),
        }
    }
    let root = root
        .canonicalize()
        .map_err(|error| format!("{}: {error}", root.display()))?;
    Ok((root, is_check_mode))
}

fn main() -> ExitCode {
    let result = parse_command_options()
        .and_then(|(root, is_check_mode)| docs_index::run(&root, is_check_mode));
    match result {
        Ok(report) => {
            for path in report.updated_indexes {
                println!("updated {path}");
            }
            if report.stale_index_messages.is_empty() {
                ExitCode::SUCCESS
            } else {
                for message in report.stale_index_messages {
                    eprintln!("{message}");
                }
                eprintln!("Run \"cargo run -p docs-index\" to regenerate.");
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
