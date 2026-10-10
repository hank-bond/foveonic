//! Checks summary parsing, specification states, source discovery, and index rendering.

use super::*;
#[path = "../tests/support/mod.rs"]
mod support;
use support::FixtureDirectory;

#[test]
fn source_files_expose_their_first_summary_paragraph() {
    let summary = |name, text| parse_source_summary(name, text, name);
    assert_eq!(
        summary("image.rs", "//! Owns an image description.\n")
            .unwrap()
            .as_deref(),
        Some("Owns an image description.")
    );
    assert_eq!(
        summary("tool.py", "\"\"\"Checks documentation.\"\"\"\n")
            .unwrap()
            .as_deref(),
        Some("Checks documentation.")
    );
    assert_eq!(
        summary(
            "image.rs",
            "//! Owns an image\n//! description.\n//!\n//! Elaboration.\n"
        )
        .unwrap()
        .as_deref(),
        Some("Owns an image description.")
    );
    for text in ["\n//! Too late.\n", "pub struct Image;\n", "//!\n"] {
        assert!(summary("image.rs", text)
            .unwrap_err()
            .contains("missing source summary"));
    }
    assert_eq!(summary("Cargo.toml", "[package]\n").unwrap(), None);
    for text in ["import os\n", "\"\"\"\"\"\"\n", "\"\"\"Unterminated\n"] {
        assert!(summary("tool.py", text).is_err());
    }
    assert_eq!(
        summary("tool.py", "#!/usr/bin/env python3\n# Encoding comment.\n\"\"\"\nChecks tool\nbehavior.\n\nFurther context.\n\"\"\"\n")
            .unwrap().as_deref(),
        Some("Checks tool behavior.")
    );
    assert_eq!(
        summary("tool.py", "'''Checks tool behavior.'''\n")
            .unwrap()
            .as_deref(),
        Some("Checks tool behavior.")
    );
    assert_eq!(
        summary("image.rs", "\u{feff}//! A BOM-prefixed summary.\r\n")
            .unwrap()
            .as_deref(),
        Some("A BOM-prefixed summary.")
    );
}

#[test]
fn specifications_distinguish_acceptance_from_implementation() {
    let fields = |status: &str, implementation: &str| {
        [
            ("status".to_owned(), status.to_owned()),
            ("implementation".to_owned(), implementation.to_owned()),
        ]
        .into()
    };
    let text = "# Example\n";
    assert!(
        validate_specification("example.md", &fields("", "incomplete"), text)
            .unwrap_err()
            .contains("status must be draft or accepted")
    );
    assert!(
        validate_specification("example.md", &fields("draft", ""), text)
            .unwrap_err()
            .contains("implementation must be incomplete or conforming")
    );
    let questions = "# Example\n\n## Open questions\n";
    assert!(
        validate_specification("example.md", &fields("draft", "incomplete"), questions).is_ok()
    );
    assert!(
        validate_specification("example.md", &fields("accepted", "incomplete"), questions)
            .unwrap_err()
            .contains("accepted specification has open questions")
    );
    assert!(validate_specification("example.md", &fields("accepted", "conforming"), text).is_ok());
}

#[test]
fn source_indexes_route_production_files_documents_and_immediate_owners() {
    let fixture = FixtureDirectory::create();
    let owner = fixture.path().join("owner");
    fs::create_dir_all(owner.join("child")).unwrap();
    for (file, content) in [
        ("image.rs", "//! Owns image descriptions.\n"),
        ("image_test.rs", "//! Checks image descriptions.\n"),
        ("tool.py", "\"\"\"Owns tool behavior.\"\"\"\n"),
        ("test_tool.py", "\"\"\"Checks tool behavior.\"\"\"\n"),
        ("README.md", "# Public entry\n"),
        ("Cargo.toml", "[package]\n"),
        (
            "guide.md",
            "---\nsummary: \"Explains cross-file usage.\"\nkind: explanation\n---\n\n# Guide\n",
        ),
        (
            "child/AGENTS.md",
            "---\nsummary: \"Owns the child responsibility.\"\n---\n\n# Child\n",
        ),
    ] {
        fs::write(owner.join(file), content).unwrap();
    }
    let entries = collect_source_entries(fixture.path(), &owner).unwrap();
    let labels = |entries: &[Entry]| {
        entries
            .iter()
            .map(|entry| entry.label.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(labels(&entries.files), ["image.rs", "tool.py"]);
    assert_eq!(labels(&entries.documents), ["guide.md"]);
    assert_eq!(labels(&entries.directories), ["child/"]);
    let rendered = render_source_index(&entries);
    assert!(rendered.contains("Owns image descriptions"));
    assert!(!rendered.contains("image_test"));
    assert!(!rendered.contains("test_tool.py"));
}

#[test]
fn frontmatter_decodes_quoted_scalar_values() {
    let text = r#"---
summary: "A \"quoted\" summary with a \\ separator."
read_when: 'Read when it''s needed.'
kind: reference
---

# Example
"#;
    let fields = parse_frontmatter(text, "example.md").unwrap();
    assert_eq!(
        frontmatter_value(&fields, "summary"),
        "A \"quoted\" summary with a \\ separator."
    );
    assert_eq!(
        frontmatter_value(&fields, "read_when"),
        "Read when it's needed."
    );
    for text in [
        "# Missing\n",
        "---\nsummary: missing terminator\n",
        "---\nkind: reference\n---\n",
    ] {
        assert!(parse_frontmatter(text, "example.md").is_err());
    }
}

#[test]
fn rendered_entries_include_state_and_read_when() {
    let fields = [
        ("summary", "Explains one choice."),
        ("status", "draft"),
        ("implementation", "incomplete"),
        ("kind", "reference"),
        ("read_when", "Read before choosing."),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), value.to_owned()))
    .collect();
    let entry = Entry {
        label: "choice".to_owned(),
        link: "./choice.md".to_owned(),
        date: String::new(),
        fields,
    };
    assert_eq!(render_index_entries(&[entry], true), "- **[choice](./choice.md)** _(draft, incomplete, reference)._ Explains one choice. _Read before choosing._");
    assert_eq!(render_index_entries(&[], true), "_(none yet)_");
}

#[test]
fn dated_names_and_percent_encoded_links_keep_their_meanings() {
    assert_eq!(
        parse_document_filename("2026-05-31-choice.md"),
        ("2026-05-31".to_owned(), "choice".to_owned())
    );
    assert_eq!(
        parse_document_filename("choice.md"),
        (String::new(), "choice".to_owned())
    );
    assert_eq!(
        decode_link_target("space%20and%20caf%C3%A9.md").unwrap(),
        "space and café.md"
    );
    assert!(decode_link_target("bad%2").is_err());
    assert!(decode_link_target("bad%FF").is_err());
}

#[test]
fn source_names_sort_consistently_without_locale_rules() {
    let fixture = FixtureDirectory::create();
    let filenames = [
        "repair_setup.rs",
        "repair.rs",
        "reconstruction_setup.rs",
        "reconstruction_global.rs",
        "reconstruction.rs",
    ];
    for filename in filenames {
        fs::write(
            fixture.path().join(filename),
            "//! Describes one responsibility.\n",
        )
        .unwrap();
    }
    let entries = collect_source_entries(fixture.path(), fixture.path()).unwrap();
    let names = entries
        .files
        .iter()
        .map(|entry| entry.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "reconstruction.rs",
            "reconstruction_global.rs",
            "reconstruction_setup.rs",
            "repair.rs",
            "repair_setup.rs",
        ]
    );
}
