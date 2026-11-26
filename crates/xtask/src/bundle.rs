//! Compiles native icon resources and the Quick Look extension, stages complete macOS bundles, and resolves launch input paths.

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

const APP_IDENTIFIER: &str = "org.foveonic.foveonic";
const PREVIEW_EXTENSION: &str = "FoveonicPreview.appex";
const PREVIEW_MODULE: &str = "FoveonicPreview";
/// Declared by SIGMA Photo Pro. Importing it lets Quick Look route X3F files without that app.
const X3F_TYPE: &str = "com.sigmaphoto.photopro-x3f";
/// System libraries the engine's static library needs, beyond what Swift links itself.
const PREVIEW_LINK_ARGUMENTS: &[&str] = &[
    "-framework",
    "QuickLookUI",
    "-framework",
    "Metal",
    "-framework",
    "QuartzCore",
    "-framework",
    "IOKit",
    "-framework",
    "CoreGraphics",
    "-framework",
    "AppKit",
    "-lc++",
];

/// Compile and ad-hoc sign the sandboxed space-bar preview extension into `output`.
pub fn compile_preview_extension(
    root: &Path,
    library: &Path,
    output: &Path,
    version: &str,
) -> Result<PathBuf> {
    let sources = root.join("crates/foveonic-quicklook/macos");
    let extension = output.join(PREVIEW_EXTENSION);
    let contents = extension.join("Contents");
    fs::create_dir_all(contents.join("MacOS"))?;
    let status = Command::new("xcrun")
        .arg("swiftc")
        .args(["-O", "-parse-as-library", "-application-extension"])
        .arg("-target")
        .arg(format!("arm64-apple-macos{MINIMUM_MACOS_VERSION}"))
        .args(["-module-name", PREVIEW_MODULE, "-import-objc-header"])
        .arg(sources.join("FoveonicQuickLook.h"))
        .arg(sources.join("PreviewProvider.swift"))
        .arg(library)
        .args(PREVIEW_LINK_ARGUMENTS)
        // App extensions start in the system's extension runtime, not a Swift main.
        .args(["-Xlinker", "-e", "-Xlinker", "_NSExtensionMain", "-o"])
        .arg(contents.join("MacOS").join(PREVIEW_MODULE))
        .status()
        .context("Cannot run xcrun swiftc. Select Xcode 26 or later")?;
    ensure!(
        status.success(),
        "Preview extension compilation failed: {status}"
    );
    preview_metadata(version).to_file_xml(contents.join("Info.plist"))?;
    // The sandbox entitlement is required for Quick Look to load the extension.
    let status = Command::new("codesign")
        .args(["--force", "--sign", "-", "--entitlements"])
        .arg(sources.join("Preview.entitlements"))
        .arg(&extension)
        .status()
        .context("Cannot run codesign")?;
    ensure!(
        status.success(),
        "Preview extension signing failed: {status}"
    );
    Ok(extension)
}

fn strings(pairs: &[(&str, &str)]) -> Dictionary {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), Value::String(value.to_string())))
        .collect()
}

fn preview_metadata(version: &str) -> Value {
    let mut attributes = Dictionary::new();
    attributes.insert("QLIsDataBasedPreview".into(), Value::Boolean(true));
    attributes.insert("QLSupportsSearchableItems".into(), Value::Boolean(false));
    attributes.insert(
        "QLSupportedContentTypes".into(),
        Value::Array(vec![X3F_TYPE.into()]),
    );
    let mut extension = strings(&[
        ("NSExtensionPointIdentifier", "com.apple.quicklook.preview"),
        (
            "NSExtensionPrincipalClass",
            &format!("{PREVIEW_MODULE}.PreviewProvider"),
        ),
    ]);
    extension.insert(
        "NSExtensionAttributes".into(),
        Value::Dictionary(attributes),
    );
    let mut metadata = strings(&[
        ("CFBundleExecutable", PREVIEW_MODULE),
        ("CFBundleIdentifier", &format!("{APP_IDENTIFIER}.preview")),
        ("CFBundleName", PREVIEW_MODULE),
        ("CFBundleDisplayName", "Foveonic Preview"),
        ("CFBundlePackageType", "XPC!"),
        ("CFBundleShortVersionString", version),
        ("CFBundleVersion", version),
        ("LSMinimumSystemVersion", MINIMUM_MACOS_VERSION),
    ]);
    metadata.insert("NSExtension".into(), Value::Dictionary(extension));
    Value::Dictionary(metadata)
}

fn x3f_type_declaration() -> Value {
    let mut tags = Dictionary::new();
    tags.insert(
        "public.filename-extension".into(),
        Value::Array(vec!["x3f".into()]),
    );
    let mut declaration = strings(&[
        ("UTTypeIdentifier", X3F_TYPE),
        ("UTTypeDescription", "SIGMA X3F raw image"),
    ]);
    declaration.insert(
        "UTTypeConformsTo".into(),
        Value::Array(vec!["public.data".into()]),
    );
    declaration.insert("UTTypeTagSpecification".into(), Value::Dictionary(tags));
    Value::Array(vec![Value::Dictionary(declaration)])
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

pub fn assemble(
    binary: &Path,
    compiled: &Path,
    extension: &Path,
    version: &str,
) -> Result<PathBuf> {
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
    fs::write(
        contents.join("Resources/JPEG-XL-LICENSES.txt"),
        include_str!("../../foveonic/licenses/JPEG-XL.txt"),
    )?;
    fs::copy(binary, contents.join("MacOS/foveonic-gui"))
        .with_context(|| format!("Cannot copy executable {}", binary.display()))?;
    for name in ["Assets.car", "Foveonic.icns"] {
        fs::copy(compiled.join(name), contents.join("Resources").join(name))
            .with_context(|| format!("Missing compiled icon resource {name}"))?;
    }
    copy_tree(extension, &contents.join("PlugIns").join(PREVIEW_EXTENSION))
        .with_context(|| format!("Cannot copy preview extension {}", extension.display()))?;
    let icon_metadata = Value::from_file(compiled.join("partial.plist"))?
        .into_dictionary()
        .context("Icon compiler plist must be a dictionary")?;
    let mut metadata = strings(&[
        ("CFBundleExecutable", "foveonic-gui"),
        ("CFBundleIdentifier", APP_IDENTIFIER),
        ("CFBundleName", "Foveonic"),
        ("CFBundleDisplayName", "Foveonic"),
        ("CFBundlePackageType", "APPL"),
        ("CFBundleShortVersionString", version),
        ("CFBundleVersion", version),
        ("LSMinimumSystemVersion", MINIMUM_MACOS_VERSION),
    ]);
    for key in [
        "NSHighResolutionCapable",
        "NSSupportsAutomaticGraphicsSwitching",
    ] {
        metadata.insert(key.into(), Value::Boolean(true));
    }
    metadata.insert("UTImportedTypeDeclarations".into(), x3f_type_declaration());
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

    fn fixture(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
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
        let extension = root.join(PREVIEW_EXTENSION);
        fs::create_dir_all(extension.join("Contents/MacOS")).unwrap();
        fs::write(
            extension.join("Contents/MacOS").join(PREVIEW_MODULE),
            b"extension",
        )
        .unwrap();
        (binary, compiled, extension)
    }

    #[test]
    fn bundle_preserves_compiler_metadata_permissions_and_replaces_stale_resources() {
        let temp = tempfile::tempdir().unwrap();
        let (binary, compiled, extension) = fixture(temp.path());
        let bundle = assemble(&binary, &compiled, &extension, "0.1.0").unwrap();
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
        let imported = metadata["UTImportedTypeDeclarations"].as_array().unwrap()[0]
            .as_dictionary()
            .unwrap();
        assert_eq!(imported["UTTypeIdentifier"].as_string(), Some(X3F_TYPE));
        assert_eq!(
            fs::read(
                contents
                    .join("PlugIns")
                    .join(PREVIEW_EXTENSION)
                    .join("Contents/MacOS")
                    .join(PREVIEW_MODULE)
            )
            .unwrap(),
            b"extension"
        );
        assert_eq!(fs::read_dir(contents.join("Resources")).unwrap().count(), 3);
        assert_eq!(
            fs::read_to_string(contents.join("Resources/JPEG-XL-LICENSES.txt")).unwrap(),
            include_str!("../../foveonic/licenses/JPEG-XL.txt")
        );
        let stale = contents.join("Resources/old-icon.png");
        fs::write(&stale, "old").unwrap();
        assemble(&binary, &compiled, &extension, "0.1.1").unwrap();
        assert!(!stale.exists());
    }

    #[test]
    fn preview_extension_matches_the_app_version_and_x3f_type() {
        let metadata = preview_metadata("0.1.0").into_dictionary().unwrap();
        assert_eq!(
            metadata["CFBundleIdentifier"].as_string(),
            Some("org.foveonic.foveonic.preview")
        );
        assert_eq!(
            metadata["CFBundleShortVersionString"].as_string(),
            Some("0.1.0")
        );
        let extension = metadata["NSExtension"].as_dictionary().unwrap();
        assert_eq!(
            extension["NSExtensionPrincipalClass"].as_string(),
            Some("FoveonicPreview.PreviewProvider")
        );
        let types = extension["NSExtensionAttributes"].as_dictionary().unwrap()
            ["QLSupportedContentTypes"]
            .as_array()
            .unwrap();
        assert_eq!(types[0].as_string(), Some(X3F_TYPE));
    }

    #[test]
    fn incomplete_resources_keep_previous_bundle_intact() {
        let temp = tempfile::tempdir().unwrap();
        let (binary, compiled, extension) = fixture(temp.path());
        let app = assemble(&binary, &compiled, &extension, "0.1.0").unwrap();
        fs::remove_file(compiled.join("Assets.car")).unwrap();
        assert!(assemble(&binary, &compiled, &extension, "0.1.1").is_err());
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
