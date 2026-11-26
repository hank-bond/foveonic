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
/// A sandboxed Quick Look extension built from the shared Swift sources and the engine library.
pub struct QuickLookExtension {
    /// Swift module, executable and bundle name.
    pub module: &'static str,
    /// Swift source and principal class.
    provider: &'static str,
    point: &'static str,
    identifier: &'static str,
    display_name: &'static str,
    /// Previews read the file themselves rather than receiving a document.
    data_based: bool,
}
impl QuickLookExtension {
    pub fn bundle(&self) -> String {
        format!("{}.appex", self.module)
    }
}

pub const QUICK_LOOK_EXTENSIONS: [QuickLookExtension; 2] = [
    QuickLookExtension {
        module: "FoveonicPreview",
        provider: "PreviewProvider",
        point: "com.apple.quicklook.preview",
        identifier: "preview",
        display_name: "Foveonic Preview",
        data_based: true,
    },
    QuickLookExtension {
        module: "FoveonicThumbnail",
        provider: "ThumbnailProvider",
        point: "com.apple.quicklook.thumbnail",
        identifier: "thumbnail",
        display_name: "Foveonic Thumbnail",
        data_based: false,
    },
];
/// The X3F type SIGMA Photo Pro also declares; the app exports it as a camera raw image.
const X3F_TYPE: &str = "com.sigmaphoto.photopro-x3f";
/// System libraries the engine's static library needs.
const ENGINE_LINK_ARGUMENTS: &[&str] = &[
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

/// Compile and ad-hoc sign each sandboxed Quick Look extension into `output`.
pub fn compile_quick_look_extensions(
    root: &Path,
    library: &Path,
    output: &Path,
    version: &str,
) -> Result<Vec<PathBuf>> {
    let sources = root.join("crates/foveonic-quicklook/macos");
    QUICK_LOOK_EXTENSIONS
        .iter()
        .map(|kind| {
            let extension = output.join(kind.bundle());
            let contents = extension.join("Contents");
            fs::create_dir_all(contents.join("MacOS"))?;
            let status = Command::new("xcrun")
                .arg("swiftc")
                .args(["-O", "-parse-as-library", "-application-extension"])
                .arg("-target")
                .arg(format!("arm64-apple-macos{MINIMUM_MACOS_VERSION}"))
                .args(["-module-name", kind.module, "-import-objc-header"])
                .arg(sources.join("FoveonicQuickLook.h"))
                .arg(sources.join("FoveonicImage.swift"))
                .arg(sources.join(format!("{}.swift", kind.provider)))
                .arg(library)
                .args(ENGINE_LINK_ARGUMENTS)
                .args([
                    "-framework",
                    "QuickLookUI",
                    "-framework",
                    "QuickLookThumbnailing",
                ])
                // App extensions start in the system's extension runtime, not a Swift main.
                .args(["-Xlinker", "-e", "-Xlinker", "_NSExtensionMain", "-o"])
                .arg(contents.join("MacOS").join(kind.module))
                .status()
                .context("Cannot run xcrun swiftc. Select Xcode 26 or later")?;
            ensure!(
                status.success(),
                "{} compilation failed: {status}",
                kind.display_name
            );
            extension_metadata(kind, version).to_file_xml(contents.join("Info.plist"))?;
            // The sandbox entitlement is required for Quick Look to load the extension.
            let status = Command::new("codesign")
                .args(["--force", "--sign", "-", "--entitlements"])
                .arg(sources.join("QuickLook.entitlements"))
                .arg(&extension)
                .status()
                .context("Cannot run codesign")?;
            ensure!(
                status.success(),
                "{} signing failed: {status}",
                kind.display_name
            );
            Ok(extension)
        })
        .collect()
}

const SPOTLIGHT_IMPORTER: &str = "FoveonicSpotlight";
/// The plug-in factory the Info.plist names and the C source is compiled with.
const SPOTLIGHT_FACTORY: &str = "1E24C19A-0056-4C0D-81D4-4527D64D0C0F";
/// Apple's type ID for Spotlight metadata importers (kMDImporterTypeID).
const SPOTLIGHT_IMPORTER_TYPE: &str = "8B08C4BF-415B-11D8-B3F9-0003936726FC";

/// Compile and ad-hoc sign the Spotlight importer. macOS indexes files through
/// importer plug-ins; Core Spotlight import extensions do not run for files on macOS.
pub fn compile_spotlight_importer(
    root: &Path,
    library: &Path,
    output: &Path,
    version: &str,
) -> Result<PathBuf> {
    let sources = root.join("crates/foveonic-quicklook/macos");
    let importer = output.join(format!("{SPOTLIGHT_IMPORTER}.mdimporter"));
    let contents = importer.join("Contents");
    fs::create_dir_all(contents.join("MacOS"))?;
    let status = Command::new("xcrun")
        .args([
            "clang", "-bundle", "-O2", "-Wall", "-Werror", "-arch", "arm64",
        ])
        .arg(format!("-mmacosx-version-min={MINIMUM_MACOS_VERSION}"))
        .arg(format!(
            "-DFOVEONIC_SPOTLIGHT_FACTORY=\"{SPOTLIGHT_FACTORY}\""
        ))
        .arg("-I")
        .arg(&sources)
        .arg(sources.join("SpotlightImporter.c"))
        .arg(library)
        .args(ENGINE_LINK_ARGUMENTS)
        .args(["-framework", "CoreFoundation", "-framework", "CoreServices"])
        .args(["-Wl,-dead_strip", "-o"])
        .arg(contents.join("MacOS").join(SPOTLIGHT_IMPORTER))
        .status()
        .context("Cannot run xcrun clang. Select Xcode 26 or later")?;
    ensure!(
        status.success(),
        "Spotlight importer compilation failed: {status}"
    );
    importer_metadata(version).to_file_xml(contents.join("Info.plist"))?;
    // The schema declares the importer's custom exposure-compensation attribute.
    let resources = contents.join("Resources");
    fs::create_dir_all(resources.join("en.lproj"))?;
    for file in ["schema.xml", "en.lproj/schema.strings"] {
        fs::copy(sources.join(file), resources.join(file))
            .with_context(|| format!("Cannot copy Spotlight importer {file}"))?;
    }
    let status = Command::new("codesign")
        .args(["--force", "--sign", "-"])
        .arg(&importer)
        .status()
        .context("Cannot run codesign")?;
    ensure!(
        status.success(),
        "Spotlight importer signing failed: {status}"
    );
    Ok(importer)
}

fn importer_metadata(version: &str) -> Value {
    let mut document = strings(&[("CFBundleTypeRole", "MDImporter")]);
    document.insert(
        "LSItemContentTypes".into(),
        Value::Array(vec![X3F_TYPE.into()]),
    );
    let mut factories = Dictionary::new();
    factories.insert(SPOTLIGHT_FACTORY.into(), "FoveonicSpotlightFactory".into());
    let mut types = Dictionary::new();
    types.insert(
        SPOTLIGHT_IMPORTER_TYPE.into(),
        Value::Array(vec![SPOTLIGHT_FACTORY.into()]),
    );
    let mut metadata = strings(&[
        ("CFBundleExecutable", SPOTLIGHT_IMPORTER),
        ("CFBundleIdentifier", &format!("{APP_IDENTIFIER}.spotlight")),
        ("CFBundleName", SPOTLIGHT_IMPORTER),
        ("CFBundlePackageType", "BNDL"),
        ("CFBundleShortVersionString", version),
        ("CFBundleVersion", version),
        ("CFPlugInDynamicRegistration", "NO"),
        ("LSMinimumSystemVersion", MINIMUM_MACOS_VERSION),
    ]);
    metadata.insert(
        "CFBundleDocumentTypes".into(),
        Value::Array(vec![Value::Dictionary(document)]),
    );
    metadata.insert("CFPlugInFactories".into(), Value::Dictionary(factories));
    metadata.insert("CFPlugInTypes".into(), Value::Dictionary(types));
    Value::Dictionary(metadata)
}

fn strings(pairs: &[(&str, &str)]) -> Dictionary {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), Value::String(value.to_string())))
        .collect()
}

fn extension_metadata(kind: &QuickLookExtension, version: &str) -> Value {
    let mut attributes = Dictionary::new();
    attributes.insert(
        "QLSupportedContentTypes".into(),
        Value::Array(vec![X3F_TYPE.into()]),
    );
    if kind.data_based {
        attributes.insert("QLIsDataBasedPreview".into(), Value::Boolean(true));
        attributes.insert("QLSupportsSearchableItems".into(), Value::Boolean(false));
    }
    let mut extension = strings(&[
        ("NSExtensionPointIdentifier", kind.point),
        (
            "NSExtensionPrincipalClass",
            &format!("{}.{}", kind.module, kind.provider),
        ),
    ]);
    extension.insert(
        "NSExtensionAttributes".into(),
        Value::Dictionary(attributes),
    );
    let mut metadata = strings(&[
        ("CFBundleExecutable", kind.module),
        (
            "CFBundleIdentifier",
            &format!("{APP_IDENTIFIER}.{}", kind.identifier),
        ),
        ("CFBundleName", kind.module),
        ("CFBundleDisplayName", kind.display_name),
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
    // A camera raw image, so Finder and Spotlight treat X3F files as photographs.
    declaration.insert(
        "UTTypeConformsTo".into(),
        Value::Array(vec!["public.camera-raw-image".into()]),
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
    extensions: &[PathBuf],
    importer: &Path,
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
    for extension in extensions {
        let name = extension
            .file_name()
            .context("Extension path has no name")?;
        copy_tree(extension, &contents.join("PlugIns").join(name))
            .with_context(|| format!("Cannot copy extension {}", extension.display()))?;
    }
    let name = importer.file_name().context("Importer path has no name")?;
    copy_tree(importer, &contents.join("Library/Spotlight").join(name))
        .with_context(|| format!("Cannot copy Spotlight importer {}", importer.display()))?;
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
    // Exported, not imported: SIGMA Photo Pro exports the type as plain data, and
    // Finder shows capture details only for types that conform to an image type.
    metadata.insert("UTExportedTypeDeclarations".into(), x3f_type_declaration());
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

/// Ad-hoc sign the finished app so macOS accepts it locally; Spotlight will not load
/// an importer from an app with an invalid signature. Nested code keeps its own
/// signature, including the extensions' sandbox entitlement. Releases re-sign
/// with the Developer ID.
pub fn sign_ad_hoc(bundle: &Path) -> Result<()> {
    let status = Command::new("codesign")
        .args(["--force", "--sign", "-"])
        .arg(bundle)
        .status()
        .context("Cannot run codesign")?;
    ensure!(status.success(), "App signing failed: {status}");
    Ok(())
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

    fn fixture(root: &Path) -> (PathBuf, PathBuf, Vec<PathBuf>, PathBuf) {
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
        let extensions = QUICK_LOOK_EXTENSIONS
            .iter()
            .map(|kind| {
                let extension = root.join(kind.bundle());
                fs::create_dir_all(extension.join("Contents/MacOS")).unwrap();
                fs::write(
                    extension.join("Contents/MacOS").join(kind.module),
                    kind.module,
                )
                .unwrap();
                extension
            })
            .collect();
        let importer = root.join(format!("{SPOTLIGHT_IMPORTER}.mdimporter"));
        fs::create_dir_all(importer.join("Contents/MacOS")).unwrap();
        fs::write(
            importer.join("Contents/MacOS").join(SPOTLIGHT_IMPORTER),
            SPOTLIGHT_IMPORTER,
        )
        .unwrap();
        (binary, compiled, extensions, importer)
    }

    #[test]
    fn bundle_preserves_compiler_metadata_permissions_and_replaces_stale_resources() {
        let temp = tempfile::tempdir().unwrap();
        let (binary, compiled, extensions, importer) = fixture(temp.path());
        let bundle = assemble(&binary, &compiled, &extensions, &importer, "0.1.0").unwrap();
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
        let declared = metadata["UTExportedTypeDeclarations"].as_array().unwrap()[0]
            .as_dictionary()
            .unwrap();
        assert_eq!(declared["UTTypeIdentifier"].as_string(), Some(X3F_TYPE));
        assert_eq!(
            declared["UTTypeConformsTo"].as_array().unwrap()[0].as_string(),
            Some("public.camera-raw-image")
        );
        for kind in &QUICK_LOOK_EXTENSIONS {
            assert_eq!(
                fs::read_to_string(
                    contents
                        .join("PlugIns")
                        .join(kind.bundle())
                        .join("Contents/MacOS")
                        .join(kind.module)
                )
                .unwrap(),
                kind.module
            );
        }
        assert_eq!(
            fs::read_to_string(
                contents
                    .join("Library/Spotlight/FoveonicSpotlight.mdimporter/Contents/MacOS")
                    .join(SPOTLIGHT_IMPORTER)
            )
            .unwrap(),
            SPOTLIGHT_IMPORTER
        );
        assert_eq!(fs::read_dir(contents.join("Resources")).unwrap().count(), 3);
        assert_eq!(
            fs::read_to_string(contents.join("Resources/JPEG-XL-LICENSES.txt")).unwrap(),
            include_str!("../../foveonic/licenses/JPEG-XL.txt")
        );
        let stale = contents.join("Resources/old-icon.png");
        fs::write(&stale, "old").unwrap();
        assemble(&binary, &compiled, &extensions, &importer, "0.1.1").unwrap();
        assert!(!stale.exists());
    }

    #[test]
    fn quick_look_extensions_match_the_app_version_and_x3f_type() {
        for (kind, identifier, class, data_based) in [
            (
                &QUICK_LOOK_EXTENSIONS[0],
                "org.foveonic.foveonic.preview",
                "FoveonicPreview.PreviewProvider",
                true,
            ),
            (
                &QUICK_LOOK_EXTENSIONS[1],
                "org.foveonic.foveonic.thumbnail",
                "FoveonicThumbnail.ThumbnailProvider",
                false,
            ),
        ] {
            let metadata = extension_metadata(kind, "0.1.0").into_dictionary().unwrap();
            assert_eq!(metadata["CFBundleIdentifier"].as_string(), Some(identifier));
            assert_eq!(
                metadata["CFBundleShortVersionString"].as_string(),
                Some("0.1.0")
            );
            let extension = metadata["NSExtension"].as_dictionary().unwrap();
            assert_eq!(
                extension["NSExtensionPrincipalClass"].as_string(),
                Some(class)
            );
            let attributes = extension["NSExtensionAttributes"].as_dictionary().unwrap();
            let types = attributes["QLSupportedContentTypes"].as_array().unwrap();
            assert_eq!(types[0].as_string(), Some(X3F_TYPE));
            assert_eq!(attributes.contains_key("QLIsDataBasedPreview"), data_based);
        }
    }

    #[test]
    fn spotlight_importer_registers_its_factory_for_x3f() {
        let metadata = importer_metadata("0.1.0").into_dictionary().unwrap();
        assert_eq!(
            metadata["CFBundleIdentifier"].as_string(),
            Some("org.foveonic.foveonic.spotlight")
        );
        let factories = metadata["CFPlugInFactories"].as_dictionary().unwrap();
        assert_eq!(
            factories[SPOTLIGHT_FACTORY].as_string(),
            Some("FoveonicSpotlightFactory")
        );
        let types = metadata["CFPlugInTypes"].as_dictionary().unwrap()[SPOTLIGHT_IMPORTER_TYPE]
            .as_array()
            .unwrap();
        assert_eq!(types[0].as_string(), Some(SPOTLIGHT_FACTORY));
        let document = metadata["CFBundleDocumentTypes"].as_array().unwrap()[0]
            .as_dictionary()
            .unwrap();
        assert_eq!(
            document["LSItemContentTypes"].as_array().unwrap()[0].as_string(),
            Some(X3F_TYPE)
        );
    }

    #[test]
    fn incomplete_resources_keep_previous_bundle_intact() {
        let temp = tempfile::tempdir().unwrap();
        let (binary, compiled, extensions, importer) = fixture(temp.path());
        let app = assemble(&binary, &compiled, &extensions, &importer, "0.1.0").unwrap();
        fs::remove_file(compiled.join("Assets.car")).unwrap();
        assert!(assemble(&binary, &compiled, &extensions, &importer, "0.1.1").is_err());
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
