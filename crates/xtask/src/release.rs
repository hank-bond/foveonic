//! Validates release tags and selects one changelog section without generating introductory prose.

use anyhow::{ensure, Context, Result};

pub fn check_tag(tag: &str, version: &str) -> Result<()> {
    ensure!(
        tag == format!("v{version}"),
        "Release tag {tag:?} must match application version v{version}"
    );
    Ok(())
}

pub fn notes(changelog: &str, version: &str) -> Result<String> {
    let heading = format!("## {version}");
    let mut sections = changelog
        .lines()
        .enumerate()
        .filter(|(_, line)| *line == heading);
    let (start, _) = sections
        .next()
        .with_context(|| format!("CHANGELOG.md has no {heading} section"))?;
    ensure!(
        sections.next().is_none(),
        "Duplicate changelog section {heading}"
    );
    let body = changelog
        .lines()
        .skip(start + 1)
        .take_while(|line| !line.starts_with("## "))
        .filter(|line| *line != "<!-- Add your release introduction here. -->")
        .collect::<Vec<_>>()
        .join("\n");
    ensure!(
        body.lines().any(|line| line.starts_with("- ")),
        "Changelog section {heading} has no change entries"
    );
    Ok(body.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_must_match_exact_application_version() {
        assert!(check_tag("v0.1.0", "0.1.0").is_ok());
        for tag in ["0.1.0", "v0.1.1", "v0.1.0-beta", "example-photographs"] {
            assert!(check_tag(tag, "0.1.0").is_err());
        }
    }

    #[test]
    fn notes_preserve_owner_prose_and_only_include_requested_version() {
        let changelog = "---\nsummary: History\n---\n# Changelog\n\n## 0.2.0\n\n- Future work\n\n## 0.1.0\n\nOwner introduction.\n\n### Changes\n\n- Initial features\n\n## 0.0.1\n\n- Older work\n";
        assert_eq!(
            notes(changelog, "0.1.0").unwrap(),
            "Owner introduction.\n\n### Changes\n\n- Initial features"
        );
        assert!(notes(changelog, "0.3.0").is_err());
        assert!(notes("## 0.1.0\n\n", "0.1.0").is_err());
        assert!(notes("## 0.1.0\n- One\n## 0.1.0\n- Two", "0.1.0").is_err());
    }

    #[test]
    fn introduction_placeholder_is_not_published() {
        let changelog = "## 0.1.0\n\n<!-- Add your release introduction here. -->\n\n### Changes\n\n- Features\n";
        assert_eq!(
            notes(changelog, "0.1.0").unwrap(),
            "### Changes\n\n- Features"
        );
    }
}
