---
summary: "The egui desktop app browses embedded previews and inspects named outputs of the shared GPU engine."
---

# Desktop GUI

Follow [`gui-and-batch.md`](../../plans/gui-and-batch.md) and [`pipeline-inspection.md`](../../docs/specs/pipeline-inspection.md). Keep photographic processing in `foveonic`; this crate owns interaction, background scheduling, and presentation. Share eframe's GPU device and queue with the processing engine.

Read [`src/AGENTS.md`](./src/AGENTS.md) before implementation.

On macOS 26, use `python3 crates/foveonic-gui/build-macos-app.py -- {X3F_OR_FOLDER}` from the repository root. It builds a local `.app` with native automatic icon appearances, without installing or publishing it. `--build-only` creates the bundle without launching; `--debug` selects the debug profile. Keep the native icon system-owned rather than restoring a runtime Dock override. Bundle tests run with `python3 -m unittest discover -s crates/foveonic-gui -p 'test_*.py'`; artwork tests are separate under `assets/`.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[assets/](./assets/AGENTS.md)** Editable layered-X artwork feeds one native Icon Composer document with glass-free light and dark appearances.
- **[src/](./src/AGENTS.md)** GUI modules separate thumbnail browsing, interaction, bounded image jobs, and GPU presentation.

### Source files

- **[build-macos-app.py](./build-macos-app.py)** Build and launch a local macOS app bundle whose native icon follows system appearance settings.

<!-- INDEX:END -->
