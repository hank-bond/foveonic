---
summary: "GUI modules separate thumbnail browsing, interaction, bounded image jobs, and GPU presentation."
---

# GUI source

Run source decoding and processing on the background image worker. Keep one active image job and one replaceable pending request. Same-photo/root completed older frames may advance feedback without replacing requested settings; reject other-photo/root and out-of-order replies. Deliver images before revision-matched scopes and deferred navigator preparation. Let export jobs finish while editing controls are disabled.

Photographic data stays on the GPU. Small tables and embedded previews may use CPU decoding/readback. Display mapping preserves buffer samples. Preserve the configuration behind the currently displayed artifact while another request is pending.

The native smoke test is opt-in through `FOVEONIC_GUI_SMOKE_DIR`; it captures real egui/wgpu frames and closes the window. Never enable it by default. The explicit [README screenshot task](../../../docs/gui-screenshot.md) uses the same isolated capture path with a checked-in recipe. Keep sidecar access disabled and processing durations out of that documentation capture.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[controls/](./controls/AGENTS.md)** Family panels compose shared editing widgets while the processor defines supported controls.

### Source files

- **[advanced.rs](./advanced.rs)** Displays advanced reconstruction controls with automatic ColorDQ and preset-linked parameter overrides.
- **[application_settings.rs](./application_settings.rs)** Configures temporary source and preview disk reuse independently of photographic edits.
- **[browser.rs](./browser.rs)** Browses X3F paths with focused keyboard selection and bounded embedded and rendered previews.
- **[capture.rs](./capture.rs)** Shows capture details without waiting for rendering or publishing an older selection.
- **[controls.rs](./controls.rs)** Selects the camera family's editing panel and shares numerical editing widgets.
- **[help.rs](./help.rs)** Explains editing controls, inspection stages, scopes, and navigator gestures in photographic language.
- **[help_examples.rs](./help_examples.rs)** Presents embedded photographic comparisons through one image-switching popover template.
- **[icon.rs](./icon.rs)** Loads the window icon while leaving macOS Dock appearances to the app bundle.
- **[information.rs](./information.rs)** Shows selectable capture metadata, the displayed filename, and inclusive visible-image pixel bounds below the viewport.
- **[inspect.rs](./inspect.rs)** Groups viewport preview quality, pipeline views, and display-only Input/Output and Difference comparisons.
- **[main.rs](./main.rs)** Runs a three-pane photographic workspace with current-buffer inspection, separate UI preferences and per-X3F editing recipes.
- **[metadata.rs](./metadata.rs)** Formats available X3F metadata as photographic readouts.
- **[mixer.rs](./mixer.rs)** Edits three-channel proportions with a triangular barycentric monochrome mixer.
- **[navigator.rs](./navigator.rs)** Keeps navigator previews small and lets the main image reach the screen first.
- **[preferences.rs](./preferences.rs)** Persists cache, organizer, scope, preview, zoom and tool preferences separately from image edits.
- **[preview_image.rs](./preview_image.rs)** Prepares oriented display thumbnails and larger previews away from the UI thread.
- **[previews.rs](./previews.rs)** Refreshes browser previews from complete photographic frames after foreground work yields.
- **[progress.rs](./progress.rs)** Shows request-to-image time and discrete completed loading and rendering stages.
- **[scopes.rs](./scopes.rs)** Draws histograms, waveforms, and parades for the selected image buffer.
- **[sections.rs](./sections.rs)** Draws shared disclosure triangles and tool-section headers, accepting section drops only inside the tools pane.
- **[sidecars.rs](./sidecars.rs)** Keeps each photo's latest edits saved without making rendering wait for disk access.
- **[smart.rs](./smart.rs)** Refines the first Quattro detail viewport once before completing the whole Full frame.
- **[smoke.rs](./smoke.rs)** Captures opt-in native workspace checks and the recipe-driven README image using isolated real GPU frames.
- **[timing.rs](./timing.rs)** Measures GUI events when requested without changing when the app repaints.
- **[toolbar.rs](./toolbar.rs)** Groups Open/Export actions in nested padded containers outside the editor.
- **[viewport.rs](./viewport.rs)** Shares viewport geometry between embedded previews and GPU images, with diagnostic mapping and manual filtering.
- **[widgets.rs](./widgets.rs)** Shares button, checkbox, and dropdown states across the workspace.
- **[worker.rs](./worker.rs)** Keeps rendering off the UI thread and replaces outdated requests instead of queuing edits.

<!-- INDEX:END -->
