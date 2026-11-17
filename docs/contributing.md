---
summary: "Documentation uses concise requirements, evolving progress plans, frontmatter summaries, and generated directory indexes."
kind: reference
read_when: "Read before writing documentation or source summaries."
---

# Contribution conventions

## Place knowledge

- `README.md` introduces the app to end users in friendly, nontechnical language. Advertise capabilities and keep a small Mac build section at the end. Keep implementation details, validation reports and contributor checks in developer documentation.
- `AGENTS.md` gives operating instructions and routes readers to relevant files.
- `docs/specs/` records required behavior agreed with the owner.
- `plans/` tracks steps, progress, checks, discoveries, and corrected assumptions for an implementation attempt.
- Research references in `docs/` catalog evidence and alternatives independently of the current implementation proposal.
- Comments beside code explain local intent and constraints. Separate documents capture knowledge that code does not explain.

Create additional document categories when work needs them. Give each fact one home and link instead of duplicating it. Keep documentation lean, plain, and grounded in actual discussions and work. User-facing copy uses camera names such as **Quattro H**. **Tiger** is an internal architecture name for code and developer documentation.

## Keep documentation portable

Use repository-relative paths or clearly marked placeholders in examples, not personal absolute paths or private checkout locations. Historical evidence references must point to checked-in files or public sources. Summarize findings and limitations instead of citing machine-only captures, logs, scripts, or binaries. Directory conventions and reproducible command examples may use ignored cache and output paths.

## Write specifications and plans

Give each specification one subject. Record durable requirements, not personal setup or temporary investigation details. Summarize investigation findings and limitations in plans without depending on private historical artifacts. State required behavior separately from implementation choices and reasons. Use **must**, **should**, and **may** when they clarify requirements.

Set `status: accepted` when requirements are agreed and contain enough detail for their stated scope. Set `status: draft` when requirements remain unresolved, and identify the open questions. Ask the owner about missing decisions.

Set `implementation: incomplete` until code and evidence satisfy all stated requirements. Set `implementation: conforming` once they do. Acceptance and implementation are separate states.

Link plans to the requirements they implement. Use small action-oriented checklists and update them during work. Move lasting requirements into specifications and keep temporary techniques and findings in the plan. Creating a plan does not authorize implementation.

Keep open-ended research and candidate workflows outside specifications. Use a separate exploration plan for what we might build next. When starting a concrete implementation slice, record its agreed behavior in the relevant specification without turning the wider research catalog into requirements.

## Format documents

Every Markdown file except `README.md` starts with frontmatter containing a one-sentence `summary`. Ordinary documentation also has `kind: reference`, `explanation`, `how-to`, or `tutorial`. Plans and `AGENTS.md` omit `kind`.

Specifications include `status` and `implementation`. Active plans omit `status`. Completed implementation slices move to `plans/archived/` with `status: archived`. Preserve their findings, update links, and remove obsolete continuation instructions. Keep unresolved requirements or acceptance work explicit in active plans. Add `read_when` only when the summary does not make the reading trigger clear. Put each field on one line and quote prose values.

Use one top-level title, sentence-case headings, relative repository links, and prose without hard wrapping. README layout is flexible. Avoid unnecessary negative framing and inventories readers can derive from code.

## Maintain indexes and source guidance

Each directory index describes how its files fit together and lists local documents and indexed immediate subdirectories. Add an `AGENTS.md` where navigation or local instructions are useful.

Rust files start with a `//!` responsibility summary. Python tools start with a module docstring summary. Use descriptive names and keep required attribution for third-party code.

### Write comments

Use simple, friendly language. Explain why a choice matters when the reason is not obvious from the code. Record useful lessons, surprising reader or library behavior, and constraints that a future change could accidentally break. Do not narrate routine steps that the code already makes clear.

Keep API contracts clear, including units, ownership, failure behavior, and caller obligations. Short module summaries still describe each file's responsibility. Use technical terms where precision needs them, and explain unfamiliar terms rather than piling them together.

Use short, plain sentences in prose, including comments and user-facing copy. Do not use semicolons. Split separate thoughts into sentences. Leave code examples and required license text intact.

The Rust tool generates index blocks between `<!-- INDEX:START -->` and `<!-- INDEX:END -->`. Edit summaries and regenerate rather than editing generated content. Source indexes omit adjacent test files.

## Run checks

Run from the repository root. Check tools and prepare photographic inputs using [test environment setup](./testing-coverage.md#test-environment-setup).

Use one test harness thread for workspace and required ignored checks. Selected core numerical and artifact checks share device/kernel setup with fresh executors; initialization, diagnostics and lifecycle checks keep fresh devices. These fixtures do not establish cold-start or latency results. Tests that exercise concurrency still create their own threads. Dedicated parallel diagnostic controls keep their existing scheduling. Debug GUI test workers use the same 8 MiB stack budget as CI; keep that override scoped to the test command. Packaged application checks use their normal stack, and the release smoke verifier removes this override.

```sh
RUST_MIN_STACK=8388608 cargo test --offline --workspace -- --test-threads=1
RUST_MIN_STACK=8388608 cargo test --offline --release -p foveonic --example profile_pipeline -- --test-threads=1
cargo clippy --offline --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo run --offline -p docs-index -- --root "$PWD"
cargo run --offline -p docs-index -- --root "$PWD" --check
.local/test-python/bin/python -m unittest discover -s crates/foveonic-gui/assets -p 'test_*.py'
.local/test-python/bin/python -m unittest discover -s scripts/profiling -p 'test_*.py'
.local/test-python/bin/python -m unittest discover -s scripts -p 'test_*.py'
```

The explicit renderer-example command runs nineteen checks that ordinary workspace tests do not run. Keep it alongside the workspace and required ignored-test commands.

Documentation checks cover metadata, titles, relative file links and stale indexes. Review prose and requirement meaning separately. Artwork tests need Python. Ordinary app builds do not.

Run every required GPU/original-file command from the [ignored Rust capability inventory](./testing-coverage.md#ignored-rust-test-inventory), using its exact disposable fixture sets and environment assignments. The planner reconciles actual compiled test names before emitting serial libtest commands. A blanket workspace `--ignored` selection also includes declared research checks and does not supply their distinct inputs. Missing required inputs and failed checks must remain explicit release blockers.

Camera-specific research checks may need additional original-processing inputs. Their specifications and plans describe those dependencies; the inventory records their classification without counting them as passes. Do not assume access to private originals or captures.
