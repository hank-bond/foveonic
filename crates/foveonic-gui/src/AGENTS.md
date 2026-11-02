---
summary: "GUI modules separate thumbnail browsing, interaction, bounded image jobs, and GPU presentation."
---

# GUI source

Frame updates and widget drawing must not probe the filesystem or wait for worker/GPU completion. Receive background snapshots and queue bounded requests instead. Recompute whole-folder state only when its inputs change. Keep filesystem work outside locks shared with the UI.

Run source decoding and processing on the background image worker. Keep one active image job and one replaceable pending request. Same-photo/root completed older frames may advance feedback without replacing requested settings; reject other-photo/root and out-of-order replies. Deliver images before revision-matched scopes and deferred navigator preparation. Let export jobs finish while editing controls are disabled.

[Import batches](../../../docs/specs/import-workflow.md) have a separate queue of frozen requests and keep interaction available. Reuse the photographic/export paths. Their independent executor shares compiled kernels and yields I/O and GPU submissions to the foreground gate. Keep import review edits in memory and source sidecar writes disabled. Destination history is one compact current-state file, not an audit log or a catalog.

Photographic data stays on the GPU. Small tables and embedded previews may use CPU decoding/readback. Display mapping preserves buffer samples. Preserve the configuration behind the currently displayed artifact while another request is pending.

The native smoke test is opt-in through `FOVEONIC_GUI_SMOKE_DIR`; it captures real egui/wgpu frames and closes the window. Never enable it by default. The explicit [README screenshot task](../../../docs/gui-screenshot.md) uses the same isolated capture path with a checked-in recipe. Keep sidecar access disabled and processing durations out of that documentation capture.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[controls/](./controls/AGENTS.md)** Family panels compose shared editing widgets while the processor defines supported controls.
- **[editor/](./editor/AGENTS.md)** Editor workflow separates scoped settings, application presets and defaults from authored photos and transient comparisons.
- **[noise_calibration/](./noise_calibration/AGENTS.md)** Calibration separates capture storage and availability, background job control, and shared-metadata presentation.

### Source files

- **[actions.rs](./actions.rs)** Applies workspace commands through browser selection and the existing per-photo editing session.
- **[application_settings.rs](./application_settings.rs)** Configures display, thumbnail memory and background disk reuse independently of photographic edits.
- **[batch.rs](./batch.rs)** Runs frozen processing batches, source scans and destination probes off-thread while yielding to interactive rendering.
- **[browser.rs](./browser.rs)** Browses X3F paths with workspace keyboard selection and bounded embedded and rendered previews.
- **[browser_chrome.rs](./browser_chrome.rs)** Draws compact browser view/sort/search menus and independent star-rating controls.
- **[browser_folders.rs](./browser_folders.rs)** Discovers descendant X3Fs and lays out folder groups for virtualized browsing.
- **[browser_items.rs](./browser_items.rs)** Paints aspect-preserving browser images and shared selectable metadata rows with star controls.
- **[browser_loader.rs](./browser_loader.rs)** Prioritizes visible thumbnails over a greedy folder pass on a separate worker from larger previews.
- **[browser_metadata.rs](./browser_metadata.rs)** Loads browser capture metadata and folder ratings off-thread and coalesces independent rating saves.
- **[browser_query.rs](./browser_query.rs)** Defines browser metadata, stable folder-local sorting and shared substring/rating filtering.
- **[browser_review.rs](./browser_review.rs)** Handles browser highlighting, import checkboxes, and shared hidden-photo visibility.
- **[capture.rs](./capture.rs)** Shows capture details without waiting for rendering or publishing an older selection.
- **[chromaticity.rs](./chromaticity.rs)** Draws a CIE 1931 xy guide with fixed visual anchors for the layer mixer.
- **[comparison_view.rs](./comparison_view.rs)** Presents highlighted photos together while the active photo keeps the shared editor and RAW renderer.
- **[controls.rs](./controls.rs)** Selects the camera family's editing panel and shares numerical editing widgets.
- **[display.rs](./display.rs)** Declares the window's display encoding and tags its native Metal layer for host color matching.
- **[display_image.rs](./display_image.rs)** Uploads bounded linear-light photographic previews without clipping them to sRGB.
- **[export.rs](./export.rs)** Shares camera-choice resolution and Full TIFF/DNG exports between interactive and batch workers.
- **[help.rs](./help.rs)** Explains editing controls, inspection stages, scopes, and navigator gestures in photographic language.
- **[help_examples.rs](./help_examples.rs)** Presents embedded photographic comparisons through one image-switching popover template.
- **[icon.rs](./icon.rs)** Loads the window icon while leaving macOS Dock appearances to the app bundle.
- **[import_review.rs](./import_review.rs)** Prepares frozen processing selections and tracks destination-scoped import review and batch status.
- **[information.rs](./information.rs)** Shows selectable capture metadata, the displayed filename, and inclusive visible-image pixel bounds below the viewport.
- **[inspect.rs](./inspect.rs)** Groups viewport preview quality, pipeline views, and display-only Input/Output and Difference comparisons.
- **[main.rs](./main.rs)** Runs a three-pane photographic workspace with current-buffer inspection, separate UI preferences and per-X3F editing recipes.
- **[metadata.rs](./metadata.rs)** Formats available X3F metadata as photographic readouts.
- **[mixer.rs](./mixer.rs)** Edits layer proportions inside a fixed dashed triangle on a CIE xy guide.
- **[navigator.rs](./navigator.rs)** Keeps navigator previews small and lets the main image reach the screen first.
- **[preferences.rs](./preferences.rs)** Persists workspace, display, cache and per-mode processing preferences separately from image edits.
- **[preview_image.rs](./preview_image.rs)** Prepares oriented display thumbnails and larger previews away from the UI thread.
- **[previews.rs](./previews.rs)** Refreshes browser previews from complete photographic frames after foreground work yields.
- **[processing.rs](./processing.rs)** Remembers per-mode output choices and edits them independently of photographic recipes.
- **[progress.rs](./progress.rs)** Shows request-to-image time and discrete completed loading and rendering stages.
- **[scopes.rs](./scopes.rs)** Draws histograms, waveforms, and parades for the selected image buffer.
- **[sections.rs](./sections.rs)** Draws shared disclosure triangles and tool-section headers, accepting section drops only inside the tools pane.
- **[shortcuts.rs](./shortcuts.rs)** Defines workspace actions and routes their keys before widgets, preserving text and modal input.
- **[sidecars.rs](./sidecars.rs)** Keeps each photo's latest edits saved without making rendering wait for disk access.
- **[smart.rs](./smart.rs)** Refines reduced overviews into Full, using a first viewport tile where the recipe supports it.
- **[smoke.rs](./smoke.rs)** Captures opt-in native workspace checks and the recipe-driven README image using isolated real GPU frames.
- **[timing.rs](./timing.rs)** Measures GUI events when requested without changing when the app repaints.
- **[toolbar.rs](./toolbar.rs)** Places source opening, destination and shared processing actions on one padded toolbar row.
- **[viewport.rs](./viewport.rs)** Shares viewport geometry between embedded previews and GPU images, with diagnostic mapping and manual filtering.
- **[widgets.rs](./widgets.rs)** Shares numeric display policy and workspace button, checkbox, and dropdown widgets.
- **[worker.rs](./worker.rs)** Keeps rendering off the UI thread and replaces outdated requests instead of queuing edits.

<!-- INDEX:END -->
