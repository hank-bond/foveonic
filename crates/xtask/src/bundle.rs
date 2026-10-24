//! Compiles native icon resources, stages complete macOS bundles, and resolves launch input paths.

use anyhow::{ensure, Context, Result};
use plist::{Dictionary, Value};
use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

// Keep executable, icon resources and LaunchServices on the same runtime floor.
pub(crate) const MINIMUM_MACOS_VERSION: &str = "15.0";

pub fn compile_icon(root: &Path, output: &Path) -> Result<()> {
    let status = Command::new("xcrun")
        .args(["actool", "--compile"])
        .arg(output)
        .args([
            "--platform",
            "macosx",
            "--minimum-deployment-target",
            MINIMUM_MACOS_VERSION,
            "--app-icon",
            "Foveonic",
            "--standalone-icon-behavior",
            "all",
            "--output-partial-info-plist",
        ])
        .arg(output.join("partial.plist"))
        .arg(root.join("crates/foveonic-gui/assets/Foveonic.icon"))
        .status()
        .context("Cannot run xcrun actool. Select Xcode 26 or later")?;
    ensure!(status.success(), "Native icon compilation failed: {status}");
    Ok(())
}

pub fn assemble(binary: &Path, compiled: &Path, version: &str) -> Result<PathBuf> {
    let parent = binary
        .parent()
        .context("Executable has no parent directory")?;
    let staging = tempfile::Builder::new()
        .prefix("foveonic-bundle-")
        .tempdir_in(parent)?;
    let staged = staging.path().join("Foveonic.app");
    let contents = staged.join("Contents");
    fs::create_dir_all(contents.join("MacOS"))?;
    fs::create_dir(contents.join("Resources"))?;
    fs::copy(binary, contents.join("MacOS/foveonic-gui"))
        .with_context(|| format!("Cannot copy executable {}", binary.display()))?;
    for name in ["Assets.car", "Foveonic.icns"] {
        fs::copy(compiled.join(name), contents.join("Resources").join(name))
            .with_context(|| format!("Missing compiled icon resource {name}"))?;
    }
    let icon_metadata = Value::from_file(compiled.join("partial.plist"))?
        .into_dictionary()
        .context("Icon compiler plist must be a dictionary")?;
    let mut metadata = Dictionary::new();
    for (key, value) in [
        ("CFBundleExecutable", "foveonic-gui"),
        ("CFBundleIdentifier", "org.foveonic.foveonic"),
        ("CFBundleName", "Foveonic"),
        ("CFBundleDisplayName", "Foveonic"),
        ("CFBundlePackageType", "APPL"),
        ("CFBundleShortVersionString", version),
        ("CFBundleVersion", version),
        ("LSMinimumSystemVersion", MINIMUM_MACOS_VERSION),
    ] {
        metadata.insert(key.into(), Value::String(value.into()));
    }
    for key in [
        "NSHighResolutionCapable",
        "NSSupportsAutomaticGraphicsSwitching",
    ] {
        metadata.insert(key.into(), Value::Boolean(true));
    }
    // Apple's compiler owns icon names, including the automatic appearances.
    metadata.extend(icon_metadata);
    Value::Dictionary(metadata).to_file_xml(contents.join("Info.plist"))?;
    let bundle = parent.join("Foveonic.app");
    if bundle.exists() {
        fs::remove_dir_all(&bundle)?;
    }
    fs::rename(staged, &bundle)?;
    Ok(bundle)
}

pub fn launch_arguments(arguments: &[OsString], cwd: &Path, home: Option<&Path>) -> Vec<OsString> {
    let mut result = arguments.to_vec();
    let mut skip_value = false;
    for argument in &mut result {
        if skip_value {
            skip_value = false;
        } else if argument == "--wb" || argument == "--mode" {
            skip_value = true;
        } else if !argument.as_encoded_bytes().starts_with(b"-") {
            // LaunchServices need not inherit the terminal's working directory.
            let input = Path::new(argument);
            let expanded = match (input.strip_prefix(OsStr::new("~")), home) {
                (Ok(relative), Some(home)) => home.join(relative),
                _ => input.to_path_buf(),
            };
            *argument = cwd.join(expanded).into_os_string();
            break;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(root: &Path) -> (PathBuf, PathBuf) {
        let binary = root.join("foveonic-gui");
        fs::write(&binary, b"test executable").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let compiled = root.join("compiled");
        fs::create_dir(&compiled).unwrap();
        for name in ["Assets.car", "Foveonic.icns"] {
            fs::write(compiled.join(name), name).unwrap();
        }
        let mut metadata = Dictionary::new();
        metadata.insert("CFBundleIconFile".into(), "Foveonic".into());
        metadata.insert("CFBundleIconName".into(), "Foveonic".into());
        Value::Dictionary(metadata)
            .to_file_xml(compiled.join("partial.plist"))
            .unwrap();
        (binary, compiled)
    }

    #[test]
    fn bundle_preserves_compiler_metadata_permissions_and_replaces_stale_resources() {
        let temp = tempfile::tempdir().unwrap();
        let (binary, compiled) = fixture(temp.path());
        let bundle = assemble(&binary, &compiled, "0.1.0").unwrap();
        let contents = bundle.join("Contents");
        let executable = contents.join("MacOS/foveonic-gui");
        assert_eq!(fs::read(&executable).unwrap(), fs::read(&binary).unwrap());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(executable).unwrap().permissions().mode() & 0o777,
                0o755
            );
        }
        let metadata = Value::from_file(contents.join("Info.plist"))
            .unwrap()
            .into_dictionary()
            .unwrap();
        for (key, value) in [
            ("CFBundleIconFile", "Foveonic"),
            ("CFBundleIconName", "Foveonic"),
            ("CFBundleIdentifier", "org.foveonic.foveonic"),
            ("CFBundleExecutable", "foveonic-gui"),
            ("CFBundleShortVersionString", "0.1.0"),
            ("CFBundleVersion", "0.1.0"),
            ("LSMinimumSystemVersion", "15.0"),
        ] {
            assert_eq!(metadata[key].as_string(), Some(value));
        }
        assert!(!metadata.contains_key("NSRequiresAquaSystemAppearance"));
        assert_eq!(fs::read_dir(contents.join("Resources")).unwrap().count(), 2);
        let stale = contents.join("Resources/old-icon.png");
        fs::write(&stale, "old").unwrap();
        assemble(&binary, &compiled, "0.1.1").unwrap();
        assert!(!stale.exists());
    }

    #[test]
    fn incomplete_resources_keep_previous_bundle_intact() {
        let temp = tempfile::tempdir().unwrap();
        let (binary, compiled) = fixture(temp.path());
        let app = assemble(&binary, &compiled, "0.1.0").unwrap();
        fs::remove_file(compiled.join("Assets.car")).unwrap();
        assert!(assemble(&binary, &compiled, "0.1.1").is_err());
        let metadata = Value::from_file(app.join("Contents/Info.plist")).unwrap();
        assert_eq!(
            metadata.as_dictionary().unwrap()["CFBundleVersion"].as_string(),
            Some("0.1.0")
        );
    }

    #[test]
    fn launch_resolves_only_input_and_keeps_camera_overrides() {
        let arguments: Vec<OsString> = [
            "--wb",
            "Shade",
            "--mode",
            "Standard",
            "relative folder/image.X3F",
        ]
        .map(Into::into)
        .into();
        let cwd = std::env::current_dir().unwrap();
        let result = launch_arguments(&arguments, &cwd, Some(&cwd));
        assert_eq!(&result[..4], &arguments[..4]);
        assert_eq!(result[4], cwd.join(&arguments[4]).into_os_string());
        assert!(launch_arguments(&[], &cwd, None).is_empty());
        assert_eq!(
            launch_arguments(&["--help".into()], &cwd, None),
            [OsString::from("--help")]
        );
        assert_eq!(
            launch_arguments(&["~/photo.x3f".into()], &cwd, Some(&cwd)),
            [cwd.join("photo.x3f").into_os_string()]
        );
        assert_eq!(
            launch_arguments(&[cwd.join("photo.x3f").into_os_string()], &cwd, None),
            [cwd.join("photo.x3f").into_os_string()]
        );
    }
}
