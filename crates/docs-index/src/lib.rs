//! Generates repository indexes and validates document metadata, titles, and relative file links.
//!
//! Imported from Foveonic's documentation tool, adapted from UIX.
//! Discover source owners beneath Rust and tool roots while keeping document layers explicit.

use regex::Regex;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub type Result<T> = std::result::Result<T, String>;
type Frontmatter = BTreeMap<String, String>;

const START: &str = "<!-- INDEX:START -->";
const END: &str = "<!-- INDEX:END -->";
const NOTE: &str = "<!-- Generated from document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->";
const SOURCE_NOTE: &str = "<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->";
const STATUSES: &[&str] = &[
    "accepted",
    "archived",
    "draft",
    "exploring",
    "landed",
    "resolved",
    "stub",
    "superseded",
];
const KINDS: &[&str] = &["explanation", "how-to", "reference", "tutorial"];

static FIELD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([a-z_]+):\s*(.*)$").unwrap());
static DATED_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([0-9]{4}-[0-9]{2}-[0-9]{2})-(.+)$").unwrap());
static FRONTMATTER_BLOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\A---\n(?s:.*?)\n---\n").unwrap());
static CODE_FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)```.*?```").unwrap());
static TITLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^# [^\r\n]+\r?$").unwrap());
static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[^\]]*\]\(([^)]+)\)").unwrap());
static SCHEME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^[a-z][a-z0-9+.-]*:").unwrap());

struct Layer {
    directory: &'static str,
    is_container: bool,
    should_sort_newest_first: bool,
}

// The root is a deliberate entry point, not a directory listing.
// Other containers discover their immediate children.
const LAYERS: &[Layer] = &[
    Layer {
        directory: ".",
        is_container: true,
        should_sort_newest_first: false,
    },
    Layer {
        directory: "docs",
        is_container: true,
        should_sort_newest_first: false,
    },
    Layer {
        directory: "docs/contributing",
        is_container: true,
        should_sort_newest_first: false,
    },
    Layer {
        directory: "docs/decisions",
        is_container: false,
        should_sort_newest_first: true,
    },
    Layer {
        directory: "docs/design",
        is_container: false,
        should_sort_newest_first: false,
    },
    Layer {
        directory: "docs/research",
        is_container: false,
        should_sort_newest_first: false,
    },
    Layer {
        directory: "docs/specs",
        is_container: false,
        should_sort_newest_first: false,
    },
    Layer {
        directory: "docs/architecture",
        is_container: true,
        should_sort_newest_first: false,
    },
    Layer {
        directory: "docs/architecture/conventions",
        is_container: true,
        should_sort_newest_first: false,
    },
    Layer {
        directory: "docs/architecture/conventions/rules",
        is_container: false,
        should_sort_newest_first: false,
    },
    Layer {
        directory: "docs/architecture/conventions/lexicon",
        is_container: false,
        should_sort_newest_first: false,
    },
    Layer {
        directory: "plans",
        is_container: false,
        should_sort_newest_first: false,
    },
];

#[derive(Debug)]
struct Entry {
    label: String,
    link: String,
    date: String,
    fields: Frontmatter,
}

#[derive(Default)]
struct SourceEntries {
    directories: Vec<Entry>,
    documents: Vec<Entry>,
    files: Vec<Entry>,
}

/// Reports updated index paths and diagnostics for stale indexes.
#[derive(Debug, Default)]
pub struct RunReport {
    pub updated_indexes: Vec<String>,
    pub stale_index_messages: Vec<String>,
}

fn read_file_text(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

fn list_directory_paths(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = fs::read_dir(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("{}: {error}", directory.display()))
        })
        .collect::<Result<Vec<_>>>()?;
    paths.sort();
    Ok(paths)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn format_relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn parse_document_filename(filename: &str) -> (String, String) {
    let base = filename.strip_suffix(".md").unwrap_or(filename);
    if let Some(captures) = DATED_NAME.captures(base) {
        (captures[1].to_owned(), captures[2].to_owned())
    } else {
        (String::new(), base.to_owned())
    }
}

fn parse_frontmatter(text: &str, file: &str) -> Result<Frontmatter> {
    if !text.starts_with("---") {
        return Err(format!("{file}: missing frontmatter"));
    }
    let end = text[3..]
        .find("\n---")
        .map(|offset| offset + 3)
        .ok_or_else(|| format!("{file}: unterminated frontmatter"))?;
    let mut fields = Frontmatter::new();
    // The documented format uses one-line values. A full YAML parser would
    // add complexity for syntax these documents do not need.
    for line in text[3..end].lines() {
        let Some(captures) = FIELD.captures(line) else {
            continue;
        };
        let value = captures[2].trim();
        let value = if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
            value[1..value.len() - 1]
                .replace("\\\"", "\"")
                .replace("\\\\", "\\")
        } else if value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'') {
            value[1..value.len() - 1].replace("''", "'")
        } else {
            value.to_owned()
        };
        fields.insert(captures[1].to_owned(), value);
    }
    if frontmatter_value(&fields, "summary").is_empty() {
        return Err(format!("{file}: frontmatter missing \"summary\""));
    }
    Ok(fields)
}

fn frontmatter_value<'a>(fields: &'a Frontmatter, key: &str) -> &'a str {
    fields.get(key).map(String::as_str).unwrap_or_default()
}

fn validate_specification(file: &str, fields: &Frontmatter, text: &str) -> Result<()> {
    if !matches!(frontmatter_value(fields, "status"), "draft" | "accepted") {
        return Err(format!(
            "{file}: specification status must be draft or accepted"
        ));
    }
    if !matches!(
        frontmatter_value(fields, "implementation"),
        "incomplete" | "conforming"
    ) {
        return Err(format!(
            "{file}: specification implementation must be incomplete or conforming"
        ));
    }
    if frontmatter_value(fields, "status") == "accepted"
        && text.lines().any(|line| line == "## Open questions")
    {
        return Err(format!("{file}: accepted specification has open questions"));
    }
    Ok(())
}

fn read_document_index_entry(
    root: &Path,
    path: &Path,
    label: String,
    link: String,
) -> Result<Entry> {
    let fields = parse_frontmatter(&read_file_text(path)?, &format_relative_path(root, path))?;
    Ok(Entry {
        label,
        link,
        date: String::new(),
        fields,
    })
}

fn collect_document_entries(root: &Path, layer: &Layer) -> Result<Vec<Entry>> {
    let directory = root.join(layer.directory);
    let mut entries = Vec::new();
    for path in list_directory_paths(&directory)? {
        let filename = file_name(&path);
        if !filename.ends_with(".md") || matches!(filename.as_str(), "AGENTS.md" | "README.md") {
            continue;
        }
        let file = format_relative_path(root, &path);
        let text = read_file_text(&path)?;
        let fields = parse_frontmatter(&text, &file)?;
        if layer.directory != "plans" && frontmatter_value(&fields, "kind").is_empty() {
            return Err(format!("{file}: indexed documentation missing \"kind\""));
        }
        if layer.directory == "docs/specs" {
            validate_specification(&file, &fields, &text)?;
        }
        let (date, label) = parse_document_filename(&filename);
        entries.push(Entry {
            label,
            link: format!("./{filename}"),
            date,
            fields,
        });
    }
    if layer.should_sort_newest_first {
        entries.sort_by(|a, b| b.date.cmp(&a.date));
    } else {
        entries.sort_by(|a, b| a.label.cmp(&b.label));
    }
    Ok(entries)
}

fn collect_container_entries(root: &Path, layer: &Layer) -> Result<Vec<Entry>> {
    let directory = root.join(layer.directory);
    let paths: Vec<PathBuf> = if layer.directory == "." {
        ["docs", "plans", "crates", "scripts"]
            .iter()
            .map(|child| root.join(child))
            .filter(|path| path.join("AGENTS.md").exists())
            .collect()
    } else {
        list_directory_paths(&directory)?
            .into_iter()
            .filter(|path| {
                if should_skip_directory(path) {
                    return false;
                }
                if path.is_dir() {
                    path.join("AGENTS.md").exists()
                } else {
                    let filename = file_name(path);
                    filename.ends_with(".md")
                        && !matches!(filename.as_str(), "AGENTS.md" | "README.md")
                }
            })
            .collect()
    };
    paths
        .into_iter()
        .map(|path| {
            let filename = file_name(&path);
            if !path.exists() {
                return Err(format!(
                    "{}/AGENTS.md: indexed child not found: {filename}",
                    layer.directory
                ));
            }
            if path.is_dir() {
                read_document_index_entry(
                    root,
                    &path.join("AGENTS.md"),
                    format!("{filename}/"),
                    format!("./{filename}/AGENTS.md"),
                )
            } else {
                let entry = read_document_index_entry(
                    root,
                    &path,
                    parse_document_filename(&filename).1,
                    format!("./{filename}"),
                )?;
                if frontmatter_value(&entry.fields, "kind").is_empty() {
                    return Err(format!(
                        "{}: indexed documentation missing \"kind\"",
                        format_relative_path(root, &path)
                    ));
                }
                Ok(entry)
            }
        })
        .collect()
}

fn render_index_entries(entries: &[Entry], should_include_metadata: bool) -> String {
    if entries.is_empty() {
        return "_(none yet)_".to_owned();
    }
    entries
        .iter()
        .map(|entry| {
            let mut line = format!("- **[{}]({})**", entry.label, entry.link);
            if should_include_metadata {
                let state = ["status", "implementation", "kind"]
                    .iter()
                    .map(|key| frontmatter_value(&entry.fields, key))
                    .filter(|value| !value.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                if !state.is_empty() {
                    line.push_str(&format!(" _({state})._"));
                }
            }
            line.push_str(&format!(" {}", frontmatter_value(&entry.fields, "summary")));
            if should_include_metadata && !frontmatter_value(&entry.fields, "read_when").is_empty()
            {
                line.push_str(&format!(
                    " _{}_",
                    frontmatter_value(&entry.fields, "read_when")
                ));
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

enum SourceLanguage {
    Rust,
    Python,
}

fn source_language(filename: &str) -> Option<SourceLanguage> {
    match Path::new(filename).extension()?.to_str()? {
        "rs" => Some(SourceLanguage::Rust),
        "py" => Some(SourceLanguage::Python),
        _ => None,
    }
}

fn is_test_source(filename: &str) -> bool {
    filename == "tests.rs"
        || filename.ends_with("_test.rs")
        || (filename.starts_with("test_") && filename.ends_with(".py"))
}

fn parse_source_summary(filename: &str, text: &str, file: &str) -> Result<Option<String>> {
    let Some(language) = source_language(filename) else {
        return Ok(None);
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let summary = match language {
        SourceLanguage::Rust => {
            let mut values = Vec::new();
            for line in text.lines() {
                let Some(content) = line.trim().strip_prefix("//!") else {
                    break;
                };
                let content = content.trim();
                if content.is_empty() {
                    break;
                }
                values.push(content);
            }
            values.join(" ")
        }
        SourceLanguage::Python => {
            // A shebang or encoding comment may precede the module docstring.
            // Read the documented triple-quoted format without running Python.
            let mut remaining = text.trim_start();
            while remaining.starts_with('#') {
                remaining = remaining
                    .split_once('\n')
                    .map_or("", |(_, rest)| rest)
                    .trim_start();
            }
            let quote = ["\"\"\"", "'''"]
                .into_iter()
                .find(|quote| remaining.starts_with(quote));
            let quote = quote.ok_or_else(|| format!("{file}: missing source summary"))?;
            let body = &remaining[quote.len()..];
            let end = body
                .find(quote)
                .ok_or_else(|| format!("{file}: unterminated module docstring"))?;
            body[..end]
                .trim()
                .lines()
                .map(str::trim)
                .take_while(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        }
    };
    if summary.is_empty() {
        return Err(format!("{file}: missing source summary"));
    }
    Ok(Some(summary))
}

fn should_skip_directory(path: &Path) -> bool {
    path.is_dir()
        && matches!(
            file_name(path).as_str(),
            ".git" | "target" | ".local" | ".venv" | "__pycache__"
        )
}

fn collect_source_entries(root: &Path, directory: &Path) -> Result<SourceEntries> {
    let mut entries = SourceEntries::default();
    for path in list_directory_paths(directory)? {
        let filename = file_name(&path);
        if matches!(filename.as_str(), "AGENTS.md" | "README.md") || should_skip_directory(&path) {
            continue;
        }
        if path.is_dir() {
            if path.join("AGENTS.md").exists() {
                entries.directories.push(read_document_index_entry(
                    root,
                    &path.join("AGENTS.md"),
                    format!("{filename}/"),
                    format!("./{filename}/AGENTS.md"),
                )?);
            }
        } else if filename.ends_with(".md") {
            entries.documents.push(read_document_index_entry(
                root,
                &path,
                filename.clone(),
                format!("./{filename}"),
            )?);
        } else if source_language(&filename).is_some() && !is_test_source(&filename) {
            let summary = parse_source_summary(
                &filename,
                &read_file_text(&path)?,
                &format_relative_path(root, &path),
            )?
            .unwrap();
            entries.files.push(Entry {
                label: filename.clone(),
                link: format!("./{filename}"),
                date: String::new(),
                fields: [("summary".to_owned(), summary)].into(),
            });
        }
    }
    Ok(entries)
}

fn render_source_index(entries: &SourceEntries) -> String {
    let mut sections = Vec::new();
    for (title, entries, should_include_metadata) in [
        ("Directories", &entries.directories, true),
        ("Local documentation", &entries.documents, true),
        ("Source files", &entries.files, false),
    ] {
        if !entries.is_empty() {
            sections.push(format!(
                "### {title}\n\n{}",
                render_index_entries(entries, should_include_metadata)
            ));
        }
    }
    if sections.is_empty() {
        "_(none yet)_".to_owned()
    } else {
        sections.join("\n\n")
    }
}

fn collect_source_index_directories(root: &Path) -> Result<Vec<PathBuf>> {
    fn visit(directory: &Path, found: &mut Vec<PathBuf>) -> Result<()> {
        if !directory.exists() {
            return Ok(());
        }
        if directory.join("AGENTS.md").exists() {
            found.push(directory.to_owned());
        }
        for path in list_directory_paths(directory)? {
            if path.is_dir() && !should_skip_directory(&path) {
                visit(&path, found)?;
            }
        }
        Ok(())
    }
    let mut directories = Vec::new();
    for source_root in ["crates", "scripts"] {
        visit(&root.join(source_root), &mut directories)?;
    }
    directories.sort();
    Ok(directories)
}

fn collect_markdown_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for path in list_directory_paths(directory)? {
        if should_skip_directory(&path) {
            continue;
        }
        if path.is_dir() {
            collect_markdown_files(&path, files)?;
        } else if path.is_file() && file_name(&path).ends_with(".md") {
            files.push(path);
        }
    }
    Ok(())
}

fn validate_document_frontmatter(root: &Path, paths: &[PathBuf]) -> Result<()> {
    for path in paths {
        if file_name(path) == "README.md" {
            continue;
        }
        let file = format_relative_path(root, path);
        let fields = parse_frontmatter(&read_file_text(path)?, &file)?;
        for (key, allowed) in [("status", STATUSES), ("kind", KINDS)] {
            let value = frontmatter_value(&fields, key);
            if !value.is_empty() && !allowed.contains(&value) {
                return Err(format!("{file}: unknown documentation {key}"));
            }
        }
    }
    Ok(())
}

fn validate_document_titles(root: &Path, paths: &[PathBuf]) -> Result<()> {
    for path in paths {
        let file = format_relative_path(root, path);
        if file_name(path) == "README.md" || file.starts_with("plans/archive/") {
            continue;
        }
        let text = read_file_text(path)?;
        let body = FRONTMATTER_BLOCK.replace(&text, "");
        let body = CODE_FENCE.replace_all(&body, "");
        let titles = TITLE.find_iter(&body).collect::<Vec<_>>();
        if titles.len() != 1 {
            return Err(format!("{file}: expected exactly one H1 title"));
        }
        if titles[0]
            .as_str()
            .trim_end_matches('\r')
            .ends_with(['.', '!', '?'])
        {
            return Err(format!("{file}: H1 title must not end with punctuation"));
        }
    }
    Ok(())
}

fn decode_link_target(target: &str) -> Result<String> {
    let mut bytes = Vec::new();
    let mut input = target.as_bytes().iter().copied();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let high = input.next().and_then(|byte| char::from(byte).to_digit(16));
            let low = input.next().and_then(|byte| char::from(byte).to_digit(16));
            let (Some(high), Some(low)) = (high, low) else {
                return Err(format!(
                    "invalid percent encoding in relative link: {target}"
                ));
            };
            bytes.push((high * 16 + low) as u8);
        } else {
            bytes.push(byte);
        }
    }
    String::from_utf8(bytes).map_err(|_| format!("invalid UTF-8 in relative link: {target}"))
}

fn validate_relative_file_links(root: &Path, paths: &[PathBuf]) -> Result<()> {
    for path in paths {
        let text = read_file_text(path)?;
        for captures in LINK.captures_iter(&text) {
            let start = captures.get(0).unwrap().start();
            // Image targets use different rules from document links. Keep them out
            // of this file-link check rather than treating previews as documents.
            if start > 0 && text.as_bytes()[start - 1] == b'!' {
                continue;
            }
            let target = captures[1].split('#').next().unwrap_or_default();
            if target.is_empty() || SCHEME.is_match(target) {
                continue;
            }
            let destination = path.parent().unwrap().join(decode_link_target(target)?);
            if !destination.exists() {
                return Err(format!(
                    "{}: relative link target not found: {target}",
                    format_relative_path(root, path)
                ));
            }
        }
    }
    Ok(())
}

fn check_or_update_index(
    root: &Path,
    path: &Path,
    body: &str,
    note: &str,
    is_check_mode: bool,
    report: &mut RunReport,
) -> Result<()> {
    let text = read_file_text(path)?;
    let start = text.find(START);
    let end = text.find(END);
    let (Some(start), Some(end)) = (start, end) else {
        return Err(format!("{}: missing {START}/{END} markers", path.display()));
    };
    let next = format!(
        "{}\n\n{note}\n\n{body}\n\n{}",
        &text[..start + START.len()],
        &text[end..]
    );
    if text == next {
        return Ok(());
    }
    let file = format_relative_path(root, path);
    if is_check_mode {
        let kind = if note == SOURCE_NOTE { "source " } else { "" };
        report
            .stale_index_messages
            .push(format!("stale {kind}index: {file}"));
    } else {
        fs::write(path, next).map_err(|error| format!("{}: {error}", path.display()))?;
        report.updated_indexes.push(file);
    }
    Ok(())
}

/// Generates indexes or checks them without writing. Handwritten text outside
/// the index markers is retained. File links are checked after generation so
/// deleting an indexed child does not leave its obsolete link in the way.
pub fn run(root: &Path, is_check_mode: bool) -> Result<RunReport> {
    let mut paths = Vec::new();
    collect_markdown_files(root, &mut paths)?;
    validate_document_frontmatter(root, &paths)?;
    validate_document_titles(root, &paths)?;
    let mut report = RunReport::default();
    for layer in LAYERS {
        let agents = root.join(layer.directory).join("AGENTS.md");
        if !agents.exists() {
            continue;
        }
        let entries = if layer.is_container {
            collect_container_entries(root, layer)?
        } else {
            collect_document_entries(root, layer)?
        };
        check_or_update_index(
            root,
            &agents,
            &render_index_entries(&entries, true),
            NOTE,
            is_check_mode,
            &mut report,
        )?;
    }
    for directory in collect_source_index_directories(root)? {
        let entries = collect_source_entries(root, &directory)?;
        check_or_update_index(
            root,
            &directory.join("AGENTS.md"),
            &render_source_index(&entries),
            SOURCE_NOTE,
            is_check_mode,
            &mut report,
        )?;
    }
    validate_relative_file_links(root, &paths)?;
    Ok(report)
}

#[cfg(test)]
mod tests;
