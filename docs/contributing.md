---
summary: "Documentation uses concise requirements, evolving progress plans, frontmatter summaries, and generated directory indexes."
kind: reference
read_when: "Read before writing documentation or source summaries."
---

# Contribution conventions

These conventions follow `~/git/foveonic`.

## Place knowledge

- `README.md` introduces the project and its goals to humans.
- `AGENTS.md` gives operating instructions and routes readers to relevant files.
- `docs/specs/` records required behavior agreed with the owner.
- `plans/` tracks steps, progress, checks, discoveries, and corrected assumptions for an implementation attempt.
- Comments beside code explain local intent and constraints. Separate documents capture knowledge that code does not explain.

Create additional document categories when work needs them. Give each fact one home and link instead of duplicating it. Keep documentation lean, plain, and grounded in actual discussions and work.

## Write specifications and plans

Give each specification one subject. State required behavior separately from implementation choices and reasons. Use **must**, **should**, and **may** when they clarify requirements.

Set `status: accepted` when requirements are agreed and contain enough detail for their stated scope. Set `status: draft` when requirements remain unresolved, and identify the open questions. Ask the owner about missing decisions.

Set `implementation: incomplete` until code and evidence satisfy all stated requirements. Set `implementation: conforming` once they do. Acceptance and implementation are separate states.

Link plans to the requirements they implement. Use small action-oriented checklists and update them during work. Move lasting requirements into specifications and keep temporary techniques and findings in the plan. Creating a plan does not authorize implementation.

## Format documents

Every Markdown file except `README.md` starts with frontmatter containing a one-sentence `summary`. Ordinary documentation also has `kind: reference`, `explanation`, `how-to`, or `tutorial`. Plans and `AGENTS.md` omit `kind`.

Specifications include `status` and `implementation`. Active plans omit `status`. Add `read_when` only when the summary does not make the reading trigger clear. Put each field on one line and quote prose values.

Use one top-level title, sentence-case headings, relative repository links, and prose without hard wrapping. README layout is flexible. Avoid unnecessary negative framing and inventories readers can derive from code.

## Maintain indexes and source guidance

Each directory index describes how its files fit together and lists local documents and indexed immediate subdirectories. Add an `AGENTS.md` where navigation or local instructions are useful.

Rust files start with a `//!` responsibility summary. Python tools start with a module docstring summary. Use descriptive names, API documentation for caller obligations, and local comments for nonobvious intent. Preserve attribution for derived code.

The Rust tool generates index blocks between `<!-- INDEX:START -->` and `<!-- INDEX:END -->`. Edit summaries and regenerate rather than editing generated content. Source indexes omit adjacent test files.

Run generation, read-only checks, and tool tests using the commands in [`README.md`](../README.md). Checks cover metadata, titles, relative file links, and stale indexes. Review prose and requirement meaning separately.
