---
summary: "The egui desktop app browses embedded previews and inspects named outputs of the shared GPU engine."
---

# Desktop GUI

Follow [`gui-and-batch.md`](../../plans/gui-and-batch.md) and [`pipeline-inspection.md`](../../docs/specs/pipeline-inspection.md). Keep photographic processing in `foveonic`; this crate owns interaction, background scheduling, and presentation. Share eframe's GPU device and queue with the processing engine.

Read [`src/AGENTS.md`](./src/AGENTS.md) before implementation.

On macOS 26, use `cargo xtask run -- {X3F_OR_FOLDER}` from the repository root. It builds a local `.app` with native automatic icon appearances, without installing or publishing it. `cargo xtask bundle` builds without launching. Add `--debug` to select the debug profile. Keep the native icon system-owned rather than restoring a runtime Dock override. Bundle tests run in `cargo test --workspace`. Artwork tests are separate under `assets/`. See the [Cargo tasks](../xtask/AGENTS.md) and [release checklist](../../docs/releases.md).

Refresh the README image explicitly with `cargo xtask screenshot`, following the [capture guide](../../docs/gui-screenshot.md). Normal builds do not sync photographs or capture screenshots.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[assets/](./assets/AGENTS.md)** Desktop assets contain layered-X icon artwork and reviewed photographic-help comparisons.
- **[src/](./src/AGENTS.md)** GUI modules separate thumbnail browsing, interaction, bounded image jobs, and GPU presentation.

### Source files

- **[build.rs](./build.rs)** Embeds checked-in photographic help assets and rejects missing rendered images during offline builds.
- **[build_support.rs](./build_support.rs)** Validates comparison manifests and embeds their existing PNGs without downloading or rendering.

<!-- INDEX:END -->
