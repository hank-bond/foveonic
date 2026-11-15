---
summary: "Repository instructions cover owner collaboration, implementation scope, and documentation maintenance."
read_when: "Read first when entering the repository."
---

# Working instructions

## Work with the owner

- Treat questions as requests for information. Discuss design choices and obtain approval before implementation.
- Implement agreed decisions. Ask when requirements are unclear or work encounters a blocker.
- Do not assume access to the owner's files outside this repository, other checkouts, or private projects. Ask for required external inputs.
- Keep changes within approved scope. Do not commit, publish, install, or make generated output the default without explicit approval. Work is reviewed before commit.
- At the end of each implementation slice, kick off a review subagent using the largest model accessible in the current harness to review code design, cleanliness and idiomatic Rust. Address its findings before closing the slice. Make each functional change available for owner testing before proceeding to the next slice.
- Keep responses concise. Use plain language and include contrasts only when needed to prevent significant confusion.
- Reserve "Reference" and "reference implementation" for each camera architecture's original processing approach in repository prose, source summaries, and filenames. Label other approaches by their function.

## Implementation approach

- Choose the simplest architecture that meets current needs. Add complexity when actual work demonstrates its value.
- Build the smallest useful end-to-end slice first, then add processing stages incrementally.
- Keep scope grounded in Merrill X3F requirements.
- Defer advanced logging and production hardening to a dedicated later phase.
- Use Rust for core code. Supporting scripts may use Python.
- Follow the [test-input reproducibility contract](./docs/specs/performance-diagnostics.md#reproducible-test-inputs) when adding or changing maintained tests.
- Before a release, run all maintained correctness tests, including GPU/original-file checks and native GUI acceptance suites, using the [test inventory](./docs/testing-coverage.md). Workspace unit tests alone are insufficient. Record skipped, unavailable and failed checks explicitly and discuss unresolved release blockers with the owner.
- Test the actual packaged release app on macOS and an interactive Windows desktop through startup, photographic rendering and the native GUI smoke checks. `--help` exits before graphics initialization and does not establish GUI coverage. Do not increase the tested application's stack to hide a failure.
- Preserve attribution when importing code.
- When replacing a choice, migrate existing callers and documents to the selected approach. Add compatibility paths only when requested.

## Find and maintain documentation

Read the applicable specifications and selected plan before implementation. Follow directory indexes to relevant files and avoid rereading documents already in context.

Follow [`docs/contributing.md`](./docs/contributing.md) when writing comments, documentation, or source summaries. Keep `AGENTS.md` focused on operating instructions and `README.md` concise as the human entry point.

Record agreed requirements in `docs/specs/`. Use small plans to track implementation progress, discoveries, and corrected assumptions. Capture project-specific knowledge from owner discussions and implementation experience.

After changing indexed documents or source summaries, regenerate indexes and run the checks in [`docs/contributing.md`](./docs/contributing.md#run-checks). Keep handwritten guidance outside generated markers.

<!-- INDEX:START -->

<!-- Generated from document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

- **[docs/](./docs/AGENTS.md)** Documentation contains agreed requirements and contribution conventions.
- **[plans/](./plans/AGENTS.md)** Small implementation plans track actionable steps, progress, discoveries, and checks.
- **[crates/](./crates/AGENTS.md)** Rust workspace crates carry their own source instructions and indexes.
- **[scripts/](./scripts/AGENTS.md)** Supporting tools provide reproducible external measurements without becoming part of photographic processing.

<!-- INDEX:END -->
