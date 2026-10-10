//! Verifies documentation routing and read-only checks through the compiled command-line tool.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use support::FixtureDirectory;

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}

fn write(root: &Path, file: &str, text: &str) {
    let path = root.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

// Keep only the tracked reference metadata and browsing previews. Exclusions
// apply at the repository root, not to similarly named documentation folders.
fn copy_repository(source: &Path, destination: &Path) {
    fn copy(root: &Path, path: &Path, destination: &Path) {
        let relative = path.strip_prefix(root).unwrap();
        if [".git", "target", ".local"]
            .iter()
            .any(|name| relative == Path::new(name))
        {
            return;
        }
        if path.is_dir()
            && matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some(".venv" | "__pycache__")
            )
        {
            return;
        }
        if relative.starts_with("references") && relative != Path::new("references") {
            let preview = relative.parent() == Some(Path::new("references/previews"))
                && relative
                    .extension()
                    .is_some_and(|extension| extension == "webp");
            if relative != Path::new("references/manifest.json")
                && relative != Path::new("references/previews")
                && !preview
            {
                return;
            }
        }
        if path.is_dir() {
            fs::create_dir_all(destination).unwrap();
            for entry in fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                copy(root, &entry.path(), &destination.join(entry.file_name()));
            }
        } else {
            fs::copy(path, destination).unwrap();
        }
    }
    copy(source, source, destination);
}

fn create_repository_fixture() -> FixtureDirectory {
    let fixture = FixtureDirectory::create();
    copy_repository(&repository_root(), fixture.path());
    // Tool checks also exercise documentation categories absent from this repo.
    for directory in ["docs/design", "docs/research", "docs/decisions", "scripts"] {
        write(
            fixture.path(),
            &format!("{directory}/AGENTS.md"),
            "---\nsummary: \"Routes fixture files.\"\n---\n\n# Fixture index\n\n<!-- INDEX:START -->\n<!-- INDEX:END -->\n",
        );
    }
    fixture
}

fn run_index_command(root: &Path, is_check_mode: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_docs-index"));
    command.arg("--root").arg(root);
    if is_check_mode {
        command.arg("--check");
    }
    command.output().unwrap()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn fixtures_keep_manifest_and_previews_but_exclude_full_size_and_build_files() {
    let fixture = FixtureDirectory::create();
    let source = fixture.path().join("source");
    let destination = fixture.path().join("repo");
    for directory in [
        ".git",
        "target",
        ".local",
        ".venv",
        "scripts/__pycache__",
        "references",
        "docs/target",
        "docs/references",
    ] {
        write(&source, &format!("{directory}/fixture.txt"), directory);
    }
    for (file, content) in [
        ("references/manifest.json", "{}\n"),
        ("references/example.tiff", "image fixture"),
        ("references/tiff/example.tiff", "reference fixture"),
        ("references/x3f/example.x3f", "input fixture"),
        ("references/previews/example.webp", "preview fixture"),
        ("references/previews/example.tiff", "excluded image fixture"),
        ("README.md", "# Fixture\n"),
    ] {
        write(&source, file, content);
    }
    copy_repository(&source, &destination);
    for directory in [".git", "target", ".local", ".venv", "scripts/__pycache__"] {
        assert!(!destination.join(directory).exists(), "{directory}");
    }
    for file in [
        "references/example.tiff",
        "references/tiff",
        "references/x3f",
        "references/previews/example.tiff",
        "references/fixture.txt",
    ] {
        assert!(!destination.join(file).exists(), "{file}");
    }
    for (file, content) in [
        ("references/manifest.json", "{}\n"),
        ("references/previews/example.webp", "preview fixture"),
        ("README.md", "# Fixture\n"),
        ("docs/target/fixture.txt", "docs/target"),
        ("docs/references/fixture.txt", "docs/references"),
    ] {
        assert_eq!(fs::read_to_string(destination.join(file)).unwrap(), content);
    }
}

#[test]
fn generation_is_idempotent_and_check_mode_does_not_repair_stale_indexes() {
    let fixture = create_repository_fixture();
    let root = fixture.path();
    assert_success(&run_index_command(root, false));
    assert_success(&run_index_command(root, true));
    assert!(run_index_command(root, false).stdout.is_empty());
    write(root, "docs/design/question.md", "---\nsummary: \"Examines one unresolved question.\"\nkind: explanation\nstatus: exploring\n---\n\n# Question\n");
    let index = root.join("docs/design/AGENTS.md");
    let before = fs::read(&index).unwrap();
    let check_output = run_index_command(root, true);
    assert!(!check_output.status.success());
    assert!(String::from_utf8_lossy(&check_output.stderr).contains("stale index"));
    assert_eq!(fs::read(&index).unwrap(), before);
    assert_success(&run_index_command(root, false));
    assert!(fs::read_to_string(index)
        .unwrap()
        .contains("Examines one unresolved question"));
    assert_success(&run_index_command(root, true));
}

#[test]
fn research_notes_have_a_dedicated_index_without_design_lifecycle_metadata() {
    let fixture = create_repository_fixture();
    let root = fixture.path();
    write(root, "docs/research/highlights.md", "---\nsummary: \"Investigates recoverable highlight structure.\"\nkind: explanation\n---\n\n# Highlight research\n");
    assert_success(&run_index_command(root, false));
    assert!(fs::read_to_string(root.join("docs/AGENTS.md"))
        .unwrap()
        .contains("./research/AGENTS.md"));
    assert!(fs::read_to_string(root.join("docs/research/AGENTS.md"))
        .unwrap()
        .contains("Investigates recoverable highlight structure"));
    assert!(!fs::read_to_string(root.join("docs/design/AGENTS.md"))
        .unwrap()
        .contains("Investigates recoverable highlight structure"));
    assert_success(&run_index_command(root, true));
}

#[test]
fn checks_reject_missing_summaries_malformed_titles_and_broken_links() {
    for (text, expected) in [
        ("# Missing summary\n", "missing frontmatter"),
        ("---\nsummary: \"Examines a question.\"\nkind: explanation\n---\n\n# First\n\n# Second\n", "exactly one H1"),
        ("---\nsummary: \"Examines a question.\"\nkind: explanation\n---\n\n# Question\n\n[Missing](missing.md)\n", "relative link target not found"),
    ] {
        let fixture = create_repository_fixture();
        write(fixture.path(), "docs/design/question.md", text);
        let result = run_index_command(fixture.path(), false);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains(expected), "{}", String::from_utf8_lossy(&result.stderr));
    }
}

#[test]
fn readmes_need_no_frontmatter_or_indexed_layout_but_still_have_links_checked() {
    let fixture = create_repository_fixture();
    let root = fixture.path();
    let public_page = "# Foveonic!\n\n# Another homepage section\n\n[Documentation](AGENTS.md)\n";
    write(root, "README.md", public_page);
    write(
        root,
        "docs/design/README.md",
        "A public introduction without a heading or frontmatter.\n",
    );
    assert_success(&run_index_command(root, false));
    assert_success(&run_index_command(root, true));
    assert_eq!(
        fs::read_to_string(root.join("README.md")).unwrap(),
        public_page
    );
    assert!(!fs::read_to_string(root.join("docs/design/AGENTS.md"))
        .unwrap()
        .contains("./README.md"));
    write(
        root,
        "README.md",
        &format!("{public_page}\n[Missing](missing.md)\n"),
    );
    let result = run_index_command(root, true);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr)
        .contains("README.md: relative link target not found"));
}

#[test]
fn rust_source_owners_are_discovered_when_a_crate_directory_exists() {
    let fixture = create_repository_fixture();
    let root = fixture.path();
    write(root, "crates/example/src/AGENTS.md", "---\nsummary: \"Routes example source files.\"\n---\n\n# Example\n\n<!-- INDEX:START -->\n<!-- INDEX:END -->\n");
    write(
        root,
        "crates/example/src/lib.rs",
        "//! Exposes the example boundary.\n",
    );
    assert_success(&run_index_command(root, false));
    let index = root.join("crates/example/src/AGENTS.md");
    assert!(fs::read_to_string(&index)
        .unwrap()
        .contains("Exposes the example boundary"));
    assert_success(&run_index_command(root, true));
    let before = fs::read(index).unwrap();
    write(
        root,
        "crates/example/src/lib.rs",
        "pub struct MissingSummary;\n",
    );
    let result = run_index_command(root, true);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("missing source summary"));
    assert_eq!(
        fs::read(root.join("crates/example/src/AGENTS.md")).unwrap(),
        before
    );
}

#[test]
fn fields_titles_and_specification_states_are_validated() {
    for (file, text, expected) in [
        ("docs/design/question.md", "---\nsummary: \"Examines a question.\"\nkind: explanation\nstatus: unknown\n---\n\n# Question\n", "unknown documentation status"),
        ("docs/design/question.md", "---\nsummary: \"Examines a question.\"\nkind: unknown\n---\n\n# Question\n", "unknown documentation kind"),
        ("docs/design/question.md", "---\nsummary: \"Examines a question.\"\n---\n\n# Question\n", "indexed documentation missing \"kind\""),
        ("docs/design/question.md", "---\nsummary: \"Examines a question.\"\nkind: explanation\n---\n\n# Question?\n", "H1 title must not end with punctuation"),
        ("docs/design/question.md", "---\r\nsummary: \"Examines a question.\"\r\nkind: explanation\r\n---\r\n\r\n# Question?\r\n", "H1 title must not end with punctuation"),
        ("docs/specs/example.md", "---\nsummary: \"Specifies behavior.\"\nkind: reference\nstatus: accepted\nimplementation: incomplete\n---\n\n# Example\n\n## Open questions\n", "accepted specification has open questions"),
    ] {
        let fixture = create_repository_fixture();
        write(fixture.path(), file, text);
        let result = run_index_command(fixture.path(), false);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains(expected), "{}", String::from_utf8_lossy(&result.stderr));
    }
}

#[test]
fn file_links_decode_paths_and_ignore_urls_anchors_and_images() {
    let fixture = create_repository_fixture();
    let root = fixture.path();
    write(root, "docs/design/space and café.txt", "Linked content.\n");
    write(root, "docs/design/question.md", "---\nsummary: \"Examines a question.\"\nkind: explanation\n---\n\n# Question\n\n```md\n# This example is not a document title.\n```\n\n[File](space%20and%20caf%C3%A9.txt#unchecked-anchor)\n[Anchor](#unchecked-anchor)\n[URL](https://example.invalid/missing)\n![Image](missing.webp)\n");
    assert_success(&run_index_command(root, false));
    assert_success(&run_index_command(root, true));
    let index = root.join("docs/design/AGENTS.md");
    let before = fs::read_to_string(&index).unwrap();
    fs::remove_file(root.join("docs/design/question.md")).unwrap();
    assert_success(&run_index_command(root, false));
    let after = fs::read_to_string(index).unwrap();
    assert!(!after.contains("./question.md"));
    assert_eq!(
        before.split("<!-- INDEX:START -->").next(),
        after.split("<!-- INDEX:START -->").next()
    );
    assert_eq!(
        before.split("<!-- INDEX:END -->").nth(1),
        after.split("<!-- INDEX:END -->").nth(1)
    );
}

#[test]
fn python_tools_are_indexed_but_their_tests_are_not() {
    let fixture = create_repository_fixture();
    let root = fixture.path();
    write(
        root,
        "scripts/example.py",
        "\"\"\"Owns the example tool.\"\"\"\n",
    );
    write(
        root,
        "scripts/test_example.py",
        "\"\"\"Checks the example tool.\"\"\"\n",
    );
    assert_success(&run_index_command(root, false));
    let index = fs::read_to_string(root.join("scripts/AGENTS.md")).unwrap();
    assert!(index.contains("Owns the example tool."));
    assert!(!index.contains("test_example.py"));
    assert_success(&run_index_command(root, true));
}

#[test]
fn private_material_and_language_caches_are_not_documentation_inputs() {
    let fixture = create_repository_fixture();
    let root = fixture.path();
    for directory in [
        ".local",
        "target",
        ".venv",
        "scripts/__pycache__",
        "scripts/.venv",
    ] {
        write(
            root,
            &format!("{directory}/scratch.md"),
            "Unstructured local material.\n",
        );
        write(
            root,
            &format!("{directory}/AGENTS.md"),
            "Not a source owner.\n",
        );
    }
    assert_success(&run_index_command(root, false));
    assert_success(&run_index_command(root, true));
    let index = fs::read_to_string(root.join("scripts/AGENTS.md")).unwrap();
    assert!(!index.contains(".venv/"));
    assert!(!index.contains("__pycache__/"));
}

#[test]
fn decision_indexes_sort_newest_first() {
    let fixture = create_repository_fixture();
    let root = fixture.path();
    for file in ["2026-01-01-older.md", "2026-01-02-newer.md"] {
        write(root, &format!("docs/decisions/{file}"), "---\nsummary: \"Records an agreed choice.\"\nkind: explanation\nstatus: accepted\n---\n\n# Choice\n");
    }
    assert_success(&run_index_command(root, false));
    let index = fs::read_to_string(root.join("docs/decisions/AGENTS.md")).unwrap();
    assert!(
        index.find("./2026-01-02-newer.md").unwrap() < index.find("./2026-01-01-older.md").unwrap()
    );
}
