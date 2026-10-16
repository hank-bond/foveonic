//! Syncs missing release photographs into one local cache and validates its last inventory offline.

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const REPOSITORY: &str = "hank-bond/foveonic";
const RELEASE: &str = "example-photographs";
const INVENTORY: &str = ".release-assets.json";

#[derive(Debug, Deserialize, Serialize)]
struct Asset {
    name: String,
    size: u64,
}

pub fn directory(root: &Path, selected: Option<PathBuf>) -> PathBuf {
    selected.unwrap_or_else(|| root.join(".local/example-photographs"))
}

pub fn filename(name: &str) -> bool {
    name.len() > 4
        && name.to_ascii_lowercase().ends_with(".x3f")
        && name
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._".contains(&b))
}

pub fn sync(destination: &Path, offline: bool) -> Result<()> {
    if offline {
        let assets: Vec<Asset> =
            serde_json::from_slice(&fs::read(destination.join(INVENTORY)).context(
                "No cached example inventory. Run cargo xtask examples sync online first",
            )?)?;
        validate(&assets)?;
        require_cached(destination, &assets)?;
        println!(
            "Using {} cached example photographs in {}",
            assets.len(),
            destination.display()
        );
        return Ok(());
    }
    #[derive(Deserialize)]
    struct Release {
        assets: Vec<Asset>,
    }
    let output = Command::new("gh")
        .args([
            "release", "view", RELEASE, "--repo", REPOSITORY, "--json", "assets",
        ])
        .stderr(Stdio::inherit())
        .output()
        .context("Cannot list example photographs. Install and authenticate GitHub CLI (gh)")?;
    ensure!(
        output.status.success(),
        "Cannot read example-photographs release: {}",
        output.status
    );
    let release: Release = serde_json::from_slice(&output.stdout)?;
    let assets = release
        .assets
        .into_iter()
        .filter(|asset| asset.name.to_ascii_lowercase().ends_with(".x3f"))
        .collect::<Vec<_>>();
    import(destination, &assets, |missing, staged| {
        let mut command = Command::new("gh");
        command
            .args([
                "release", "download", RELEASE, "--repo", REPOSITORY, "--dir",
            ])
            .arg(staged);
        for asset in missing {
            command.arg("--pattern").arg(&asset.name);
        }
        let status = command
            .status()
            .context("Cannot download example photographs")?;
        ensure!(status.success(), "Example download failed: {status}");
        Ok(())
    })
}

fn validate(assets: &[Asset]) -> Result<()> {
    ensure!(!assets.is_empty(), "The example inventory contains no X3Fs");
    let mut names = HashSet::new();
    for asset in assets {
        ensure!(
            filename(&asset.name) && names.insert(&asset.name),
            "Unsafe or duplicate example filename: {}",
            asset.name
        );
    }
    Ok(())
}

fn require_cached(destination: &Path, assets: &[Asset]) -> Result<()> {
    let missing = assets
        .iter()
        .filter(|asset| !destination.join(&asset.name).is_file())
        .map(|asset| asset.name.as_str())
        .collect::<Vec<_>>();
    ensure!(
        missing.is_empty(),
        "Missing cached examples: {}. Run cargo xtask examples sync online first",
        missing.join(", ")
    );
    Ok(())
}

fn import(
    destination: &Path,
    assets: &[Asset],
    download: impl FnOnce(&[&Asset], &Path) -> Result<()>,
) -> Result<()> {
    validate(assets)?;
    fs::create_dir_all(destination)?;
    let destination = fs::canonicalize(destination)?;
    let missing = assets
        .iter()
        .filter(|asset| !destination.join(&asset.name).exists())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        let staged = tempfile::Builder::new()
            .prefix(".download-")
            .tempdir_in(&destination)?;
        download(&missing, staged.path())?;
        for asset in &missing {
            let path = staged.path().join(&asset.name);
            ensure!(
                fs::metadata(&path)
                    .with_context(|| format!("Download missing: {}", asset.name))?
                    .len()
                    == asset.size,
                "Incomplete download: {}",
                asset.name
            );
        }
        for asset in &missing {
            // A hard link imports a complete file without overwriting a concurrent sync's original.
            match fs::hard_link(
                staged.path().join(&asset.name),
                destination.join(&asset.name),
            ) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error).context("Cannot import downloaded example"),
            }
        }
    }
    require_cached(&destination, assets)?;
    let mut inventory = tempfile::NamedTempFile::new_in(&destination)?;
    serde_json::to_writer_pretty(inventory.as_file_mut(), assets)?;
    inventory.persist(destination.join(INVENTORY))?;
    println!(
        "Synced {} example photographs ({} downloaded) in {}",
        assets.len(),
        missing.len(),
        destination.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> Vec<Asset> {
        vec![
            Asset {
                name: "sample.x3f".into(),
                size: 3,
            },
            Asset {
                name: "another.x3f".into(),
                size: 4,
            },
        ]
    }

    #[test]
    fn sync_preserves_originals_and_sidecars_and_records_all_offline_requirements() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path();
        fs::write(
            source.join("sample.x3f"),
            "local original, deliberately not the remote size",
        )
        .unwrap();
        let sidecar = source.join("sample.x3f.foveonic.json");
        fs::write(&sidecar, "local editing settings").unwrap();
        import(source, &assets(), |missing, staged| {
            assert_eq!(missing.len(), 1);
            assert_eq!(missing[0].name, "another.x3f");
            fs::write(staged.join("another.x3f"), "new!")?;
            assert!(!source.join("another.x3f").exists());
            Ok(())
        })
        .unwrap();
        assert_eq!(
            fs::read_to_string(source.join("sample.x3f")).unwrap(),
            "local original, deliberately not the remote size"
        );
        assert_eq!(
            fs::read_to_string(sidecar).unwrap(),
            "local editing settings"
        );
        sync(source, true).unwrap();
        import(source, &assets(), |_, _| {
            panic!("Must not redownload existing files")
        })
        .unwrap();
        fs::remove_file(source.join("another.x3f")).unwrap();
        assert!(sync(source, true)
            .unwrap_err()
            .to_string()
            .contains("another.x3f"));
    }

    #[test]
    fn failed_or_partial_downloads_do_not_import_files_or_publish_inventory() {
        for partial in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            assert!(import(temp.path(), &assets(), |_, staged| {
                fs::write(staged.join("sample.x3f"), "raw")?;
                if partial {
                    fs::write(staged.join("another.x3f"), "x")?;
                }
                Ok(())
            })
            .is_err());
            assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn filenames_cannot_escape_the_cache_and_offline_needs_an_inventory() {
        for name in [
            "../photo.x3f",
            "/photo.x3f",
            "x/photo.x3f",
            "*.x3f",
            "photo.json",
            "-photo.x3f",
        ] {
            assert!(!filename(name));
        }
        assert!(filename("yellow-flowers-and-stroller.x3f"));
        let temp = tempfile::tempdir().unwrap();
        assert!(sync(temp.path(), true)
            .unwrap_err()
            .to_string()
            .contains("online first"));
        let duplicate = vec![
            Asset {
                name: "photo.x3f".into(),
                size: 1,
            },
            Asset {
                name: "photo.x3f".into(),
                size: 1,
            },
        ];
        assert!(import(temp.path(), &duplicate, |_, _| unreachable!()).is_err());
    }
}
