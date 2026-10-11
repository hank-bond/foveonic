---
summary: "GUI modules separate thumbnail browsing, interaction, bounded image jobs, and GPU presentation."
---

# GUI source

Run source decoding and processing on the background image worker. Keep one active job and one replaceable pending request; reject replies from old revisions. Let export jobs finish while editing controls are disabled.

Photographic data stays on the GPU. Small tables and embedded JPEGs may use CPU decoding/readback. Display mapping preserves buffer samples. Preserve the configuration behind the currently displayed artifact while another request is pending.

The native smoke test is opt-in through `FOVEONIC_GUI_SMOKE_DIR`; it captures real egui/wgpu frames and closes the window. Never enable it by default.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[browser.rs](./browser.rs)** Browses X3F paths and lazily decodes embedded JPEGs through a bounded thumbnail worker.
- **[controls.rs](./controls.rs)** Builds aligned sensor/tone editing rows with reset, focused value nudges, and sample-based rerender signals.
- **[main.rs](./main.rs)** Runs a three-pane Merrill workspace with current-buffer inspection, reorderable tools, and UI-only persistence.
- **[metadata.rs](./metadata.rs)** Formats available X3F metadata as photographic readouts.
- **[preferences.rs](./preferences.rs)** Persists organizer, scope, and tool-section preferences.
- **[scopes.rs](./scopes.rs)** Displays sampled current-buffer histograms and density scopes with explicit signal meaning and range diagnostics.
- **[sections.rs](./sections.rs)** Draws collapse/drag headers and accepts section drops only inside the tools pane.
- **[smoke.rs](./smoke.rs)** Captures opt-in native workspace checks using isolated preferences and real egui/wgpu frames.
- **[viewport.rs](./viewport.rs)** Shares viewport geometry between JPEG previews and GPU images, with diagnostic mapping and manual filtering.
- **[worker.rs](./worker.rs)** Runs one image job at a time, coalesces pending requests, and waits for GPU completion off the UI thread.

<!-- INDEX:END -->
