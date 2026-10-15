//! Validates comparison manifests and embeds their existing PNGs without downloading or rendering.

use serde::Deserialize;
use std::{collections::HashSet, fs, path::Path};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    id: String,
    title: String,
    description: String,
    note: String,
    source: String,
    crop: [u32; 4],
    image_size: [u32; 2],
    base_args: Vec<String>,
    variants: Vec<Variant>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Variant {
    id: String,
    label: String,
    caption: String,
    image: String,
    args: Vec<String>,
}

fn filename(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-._".contains(&c))
}

fn require(condition: bool, message: impl Into<String>) -> Result<(), String> {
    condition.then_some(()).ok_or_else(|| message.into())
}

pub fn generate(root: &Path, mut watch: impl FnMut(&Path)) -> Result<String, String> {
    watch(root);
    let mut directories = fs::read_dir(root)
        .map_err(|e| format!("{}: {e}", root.display()))?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    directories.retain(|path| path.is_dir());
    directories.sort();
    require(
        !directories.is_empty(),
        "No photographic help comparisons found",
    )?;
    let mut result = String::from("const COMPARISONS: &[Comparison] = &[\n");
    let mut ids = HashSet::new();
    for directory in directories {
        watch(&directory);
        let path = directory.join("comparison.json");
        watch(&path);
        let manifest: Manifest = serde_json::from_str(
            &fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?,
        )
        .map_err(|e| format!("{}: {e}", path.display()))?;
        let id = &manifest.id;
        require(
            filename(id) && directory.file_name().and_then(|n| n.to_str()) == Some(id),
            format!("Comparison ID must match its directory: {}", path.display()),
        )?;
        require(
            ids.insert(id.clone()),
            format!("Duplicate comparison ID: {id}"),
        )?;
        require(
            [&manifest.title, &manifest.description, &manifest.note]
                .iter()
                .all(|v| !v.trim().is_empty()),
            format!("{id}: title, description, and note must not be empty"),
        )?;
        require(
            filename(&manifest.source) && manifest.source.ends_with(".x3f"),
            format!("{id}: source must be an official X3F filename"),
        )?;
        let [x0, y0, x1, y1] = manifest.crop;
        require(
            x1 > x0 && y1 > y0 && manifest.image_size.iter().all(|v| *v > 0),
            format!("{id}: crop and image size must be positive"),
        )?;
        require(
            !manifest.base_args.is_empty() && manifest.variants.len() >= 2,
            format!("{id}: shared recipe and at least two variants are required"),
        )?;
        result.push_str(&format!(
            "Comparison {{ id: {id:?}, title: {:?}, source_name: {:?}, description: {:?}, note: {:?}, image_size: {:?}, variants: &[\n",
            manifest.title, manifest.source.strip_suffix(".x3f").unwrap(), manifest.description, manifest.note, manifest.image_size,
        ));
        let mut variant_ids = HashSet::new();
        let mut images = HashSet::new();
        for variant in manifest.variants {
            require(
                filename(&variant.id)
                    && variant_ids.insert(variant.id.clone())
                    && filename(&variant.image)
                    && variant.image.ends_with(".png")
                    && images.insert(variant.image.clone()),
                format!("{id}: variant IDs and PNG filenames must be safe and distinct"),
            )?;
            require(
                !variant.label.trim().is_empty() && !variant.caption.trim().is_empty(),
                format!("{id}: each variant needs a label and caption"),
            )?;
            // Parsing the full recipe catches malformed argument types even though
            // normal builds never execute it.
            let _ = variant.args;
            let image = directory.join(&variant.image);
            watch(&image);
            let bytes = fs::read(&image).map_err(|e| format!(
                "Missing rendered help image {}: {e}. Run the explicit help-example render step.", image.display()))?;
            require(
                bytes.len() >= 24
                    && &bytes[..8] == b"\x89PNG\r\n\x1a\n"
                    && &bytes[12..16] == b"IHDR",
                format!("{}: expected a PNG image", image.display()),
            )?;
            let dimensions = [
                u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
                u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
            ];
            require(
                dimensions == manifest.image_size,
                format!("{}: PNG dimensions must match image_size", image.display()),
            )?;
            result.push_str(&format!(
                "Variant {{ id: {:?}, label: {:?}, caption: {:?}, png: include_bytes!({:?}) }},\n",
                variant.id,
                variant.label,
                variant.caption,
                image.to_str().ok_or("Non-UTF-8 image path")?,
            ));
        }
        result.push_str("] },\n");
    }
    result.push_str("];\n");
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_new_variant_and_unsafe_paths_fail_without_sources_or_renderer() {
        let root = std::env::temp_dir().join(format!("foveonic-help-{}", std::process::id()));
        let directory = root.join("example");
        fs::create_dir_all(&directory).unwrap();
        let mut manifest = json!({
            "id": "example", "title": "Example", "description": "Explanation", "note": "Fixed recipe",
            "source": "example.x3f", "crop": [0, 0, 2, 2], "image_size": [1024, 1024],
            "base_args": ["--wb", "Sunlight"],
            "variants": [
                {"id": "off", "label": "Off", "caption": "Before", "image": "off.png", "args": []},
                {"id": "on", "label": "On", "caption": "After", "image": "on.png", "args": []}
            ]
        });
        let path = directory.join("comparison.json");
        fs::write(&path, manifest.to_string()).unwrap();
        assert!(generate(&root, |_| {})
            .unwrap_err()
            .contains("Missing rendered help image"));
        for name in ["off.png", "on.png"] {
            fs::write(directory.join(name), include_bytes!("assets/icon.png")).unwrap();
        }
        let generated = generate(&root, |_| {}).unwrap();
        assert!(generated.contains("include_bytes!"));
        assert!(generated.contains("Before"));
        assert!(generated.contains("source_name: \"example\""));
        manifest["variants"][1]["image"] = json!("../on.png");
        fs::write(&path, manifest.to_string()).unwrap();
        assert!(generate(&root, |_| {})
            .unwrap_err()
            .contains("safe and distinct"));
        manifest["variants"][1]["image"] = json!("missing.png");
        fs::write(&path, manifest.to_string()).unwrap();
        assert!(generate(&root, |_| {}).unwrap_err().contains("missing.png"));
        manifest["variants"][1]["image"] = json!("on.png");
        manifest["image_size"] = json!([400, 360]);
        fs::write(&path, manifest.to_string()).unwrap();
        assert!(generate(&root, |_| {}).unwrap_err().contains("dimensions"));
        fs::remove_dir_all(root).unwrap();
    }
}
