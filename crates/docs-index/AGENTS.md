---
summary: "The Rust documentation tool generates directory indexes and checks document fields, titles, and relative links."
---

# Documentation tool

Imported from the reference implementation at commit `d4cfa2d`. Root routing includes indexed crates and skips absent directories. Read the [source index](./src/AGENTS.md) for implementation guidance and [CLI tests](./tests/cli.rs) for repository-fixture checks. Use the [contributor checks](../../docs/contributing.md#run-checks). Tests pass `--root {PATH}` to operate on isolated repository copies.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[src/](./src/AGENTS.md)** Index generation and validation share a library, with a small command-line entry point.

<!-- INDEX:END -->
