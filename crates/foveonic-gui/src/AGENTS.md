---
summary: "GUI modules separate thumbnail browsing, interaction, bounded image jobs, and GPU presentation."
---

# GUI source

Frame updates and widget drawing must not probe the filesystem or wait for worker/GPU completion. Receive background snapshots and queue bounded requests instead. Recompute whole-folder state only when its inputs change. Keep filesystem work outside locks shared with the UI.

Snapshot egui input and response queries before context data or memory write closures. Keep those closures limited to their borrowed data so they cannot reacquire the context lock.

Run requested source loading through the background source loader and processing through the serial render worker. The render coordinator owns desired work per photograph and purpose on its own thread and publishes one replaceable work plan to the serialized executor. The UI publishes immutable view context and checks incoming images against its latest view before application. Same-photo/root completed older frames may advance feedback without replacing requested settings. Reject incompatible and out-of-order replies. Deliver images before image-matched scopes and deferred navigator preparation. Exports remain frozen commands that finish independently of replaceable preview work.

[Import batches](../../../docs/specs/import-workflow.md) have a separate queue of frozen requests and keep interaction available. Reuse the photographic/export paths. Their independent executor shares compiled kernels and yields I/O and GPU submissions to the foreground gate. Keep import review edits in memory and source sidecar writes disabled. Destination history is one compact current-state file, not an audit log or a catalog.

Photographic data stays on the GPU. Small tables and embedded previews may use CPU decoding/readback. Display mapping preserves buffer samples. Preserve the configuration behind the currently displayed artifact while another request is pending.

Keep photograph-tool selection, cancellation, pointer routing and toolbar composition in `photo_tools.rs`. It owns the single selected tool. Tool modules own their gestures, pending replies and options and must not activate or cancel their peers. Use `framing.rs` for the common Crop/Rotate presentation and overlay, `toolbar.rs` for strip placement and grouping, and `widgets.rs` for control geometry and menus. Return edit changes through the drawing result rather than adding tool-specific dirty checks to the app loop.

The native smoke test is opt-in through `FOVEONIC_GUI_SMOKE_DIR`; it captures real egui/wgpu frames and closes the window. Never enable it by default. The explicit [README screenshot task](../../../docs/gui-screenshot.md) uses the same isolated capture path with a checked-in recipe. Keep sidecar access disabled and processing durations out of that documentation capture.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[browser/](./browser/AGENTS.md)** Browser modules separate folder discovery, queries, selection, drawing and bounded preview loading.
- **[comparison_view/](./comparison_view/AGENTS.md)** Comparison layout packs saved photograph aspects while keeping viewport gestures and identity stable.
- **[controls/](./controls/AGENTS.md)** Family panels compose shared editing widgets while the processor defines supported controls.
- **[crop_tool/](./crop_tool/AGENTS.md)** Crop interaction separates pointer geometry and aspect-ratio preferences from the shared image view.
- **[editor/](./editor/AGENTS.md)** Editor workflow separates scoped settings, application presets and defaults from authored photos and transient comparisons.
- **[noise_calibration/](./noise_calibration/AGENTS.md)** Calibration separates capture storage and availability, background job control, and shared-metadata presentation.
- **[rendering/](./rendering/AGENTS.md)** Render coordination separates request meaning, work matching, target scheduling, delivery and serialized execution.
- **[smoke/](./smoke/AGENTS.md)** Native app checks keep scenario expectations separate from shared input, capture and observation support.

### Source files

- **[actions.rs](./actions.rs)** Applies workspace commands through browser selection and the existing per-photo editing session.
- **[activity.rs](./activity.rs)** Presents retained launch logs and copy controls while keeping one acknowledged toolbar status.
- **[application_settings.rs](./application_settings.rs)** Configures display, editing targets, retained memory and background disk reuse independently of photographic edits.
- **[automation.rs](./automation.rs)** Observes real controls for opt-in scripted application checks.
- **[batch.rs](./batch.rs)** Runs frozen processing batches, source scans and destination probes off-thread while yielding to interactive rendering; import-history matching never waits for rendering.
- **[chromaticity.rs](./chromaticity.rs)** Draws a CIE 1931 xy guide with fixed visual anchors for the layer mixer.
- **[comparison_view.rs](./comparison_view.rs)** Fits browser-selected photographs into bounded comparison tiles with explicit viewport editing targets.
- **[controls.rs](./controls.rs)** Selects the camera family's editing panel and shares numerical editing widgets.
- **[crop_tool.rs](./crop_tool.rs)** Owns crop gestures and aspect options within the shared photograph-tool lifecycle.
- **[diagnostics.rs](./diagnostics.rs)** Controls bounded manual and scripted captures through the shared background recorder.
- **[display.rs](./display.rs)** Declares the window's display encoding and tags its native Metal layer for host color matching.
- **[display_image.rs](./display_image.rs)** Uploads bounded linear-light photographic previews without clipping them to sRGB.
- **[export.rs](./export.rs)** Shares Quality photographic and DNG exports between interactive and batch workers.
- **[folder_loader.rs](./folder_loader.rs)** Discovers folder snapshots away from the UI and publishes only the newest request.
- **[framing.rs](./framing.rs)** Shares the rotated editing plane, crop frame, exterior shading and dimensions across framing tools.
- **[help.rs](./help.rs)** Explains editing controls, inspection stages, scopes, and navigator gestures in photographic language.
- **[help_examples.rs](./help_examples.rs)** Presents embedded photographic comparisons through one image-switching popover template.
- **[icon.rs](./icon.rs)** Loads window and startup artwork while leaving macOS Dock appearances to the app bundle.
- **[import_review.rs](./import_review.rs)** Prepares frozen processing selections and tracks destination-scoped import review and batch status.
- **[information.rs](./information.rs)** Shows selectable capture metadata, the displayed filename, and inclusive visible-image pixel bounds below the viewport.
- **[inspect.rs](./inspect.rs)** Groups viewport background, processing quality, pipeline views, and display-only comparisons.
- **[loupe_tool.rs](./loupe_tool.rs)** Owns transient source-anchored loupe gestures, persisted options and independent lens geometry.
- **[loupe_view.rs](./loupe_view.rs)** Presents transient native-detail inspection; its patch is planned by the view planner like a zoomed view.
- **[main.rs](./main.rs)** Runs a three-pane photographic workspace with current-buffer inspection, separate UI preferences and per-X3F editing recipes.
- **[memory.rs](./memory.rs)** Counts shared photographic samples and distinct frontend display pools for the worker memory target.
- **[metadata.rs](./metadata.rs)** Formats available X3F metadata as photographic readouts.
- **[mixer.rs](./mixer.rs)** Edits layer proportions inside a fixed dashed triangle on a CIE xy guide.
- **[multi_edit_controls.rs](./multi_edit_controls.rs)** Applies explicit property edits and draws mixed-value controls for viewport-selected photographs.
- **[multi_edit_runtime.rs](./multi_edit_runtime.rs)** Owns bounded photo settings and images with independent editing selection and render demand.
- **[navigator.rs](./navigator.rs)** Keeps navigator previews small and lets the main image reach the screen first.
- **[photo_tools.rs](./photo_tools.rs)** Owns photograph-tool selection, cancellation and pointer routing, and composes the toolbar.
- **[photo_views.rs](./photo_views.rs)** Retains per-photo viewing geometry and shares Shift viewing changes across editing targets.
- **[preferences.rs](./preferences.rs)** Persists workspace, display, cache and per-mode processing preferences separately from image edits.
- **[preview_image.rs](./preview_image.rs)** Prepares display thumbnails and larger previews from upright images away from the UI thread.
- **[previews.rs](./previews.rs)** Retains edited GPU previews immediately and persists bounded photographic previews after foreground work yields.
- **[processing.rs](./processing.rs)** Remembers per-mode output choices and edits them independently of photographic recipes.
- **[progress.rs](./progress.rs)** Shows request-to-image time and discrete completed loading and rendering stages.
- **[rotate_tool.rs](./rotate_tool.rs)** Owns line and freehand rotation gestures within the shared photograph-tool lifecycle.
- **[scopes.rs](./scopes.rs)** Draws histograms, waveforms, and parades for the selected image buffer.
- **[sections.rs](./sections.rs)** Draws shared disclosure triangles and tool-section headers, accepting section drops only inside the tools pane.
- **[shortcuts.rs](./shortcuts.rs)** Defines workspace actions and routes their keys before widgets, preserving text and modal input.
- **[sidecars.rs](./sidecars.rs)** Keeps each photo's latest edits saved without making rendering wait for disk access.
- **[startup.rs](./startup.rs)** Presents a responsive loading window while one worker prepares the photographic application.
- **[support.rs](./support.rs)** Starts support reporting before graphics and forwards dependency warnings into the same history.
- **[timing.rs](./timing.rs)** Connects GUI tasks, native drawing and existing measurements to the shared Perfetto capture.
- **[toolbar.rs](./toolbar.rs)** Shares padded processing rows and a responsive right-aligned image-tool region.
- **[viewport.rs](./viewport.rs)** Shares viewport geometry between embedded previews and GPU images, with diagnostic mapping and manual filtering.
- **[viewport_selection.rs](./viewport_selection.rs)** Keeps bounded viewport editing targets separate from browser selection and active-photo context.
- **[widgets.rs](./widgets.rs)** Shares numeric display policy, control sizes and workspace widgets.

<!-- INDEX:END -->
