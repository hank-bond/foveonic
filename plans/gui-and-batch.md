---
summary: "Expose the working conversion engine through a minimal GUI and batch CLI using shared settings."
---

# Connect GUI and batch processing

Requirements: [`gui-workspace.md`](../docs/specs/gui-workspace.md), [`application-workflows.md`](../docs/specs/application-workflows.md), [`pipeline-inspection.md`](../docs/specs/pipeline-inspection.md), and [`render-resources.md`](../docs/specs/render-resources.md). Build on the processor snapshot committed as `8da3e78`.

## Steps

- [x] Select egui/eframe with wgpu and agree on demand-driven inspection and selective retention.
- [x] Add named output ports with independent dependencies/lifetimes and selective execution.
- [x] Add bounded embedded-JPEG browsing.
- [x] Share the GPU device/queue and provide truthful sensor/RGB/scalar presentation.
- [x] Bound background render work and reject stale replies.
- [x] Initialize recorded WB/mode per image, preserve launch overrides, and keep early inspection independent of downstream settings.
- [x] Share preview/pipeline placement and native-pixel zoom.
- [x] Toggle Fit/snap on double-click in both views; add navigator zoom readout and 100%/200%/400% buttons.
- [x] Anchor main-image double-click to the cursor like scroll zoom, target navigator clicks, and consolidate scale/pan arithmetic.
- [x] Select snap by Command/Ctrl-click, underline its preset, persist it with default 200%, and explain gestures in navigator popovers.
- [x] Preserve fixed magnification and the visible crop's top-left origin when resizing outside Fit mode; migrate smoke callers and check preview/pipeline geometry.
- [x] Build the three-pane organizer/image/tools workspace with reflowing icon/list browsing.
- [x] Add reorderable/collapsible tools, draggable navigator, current-buffer scopes, and photographic metadata.
- [x] Persist UI preferences separately from session-only editing settings.
- [x] Add label/slider/value controls, double-click reset, global Sensor gain, and focused numeric nudging.
- [x] Remove view-only exposure and non-image data from image inspection following owner review.
- [x] Complete the cleanup pass and regression checks before owner review for commit.
- [ ] Review interactive use and a small batch with the owner; record concrete processing or responsiveness gaps.
- [ ] Select interaction checkpoints only if measurements justify them.
- [ ] Expose both output recipes through CLI batch processing; TIFF is implemented, DNG remains pending.

## Current implementation

The owner selected egui/eframe 0.31.1 with wgpu 24 after inspecting PhotoCraft and approved GUI dependency downloads. The initial GUI and processor work is committed; later sections record reviewed refinements and remaining owner acceptance. TIFF batch processing is covered by [`tiff-controls-and-batch.md`](./tiff-controls-and-batch.md); resource ownership by [`render-resources-and-optimization.md`](./render-resources-and-optimization.md).

- The GUI shares eframe's device/queue with the engine. Viewport display uses GPU textures. Native menus and monitor-profile correction remain deferred.
- Browser requests/results are bounded, rows are virtualized, and the thumbnail cache holds at most 256 entries. Seek-based reads decode only the smallest embedded JPEG and read the largest JPEG's dimensions for layout. All six originals contain two JPEG sizes: 221×147 and 4704×3136. Larger-preview decoding remains deferred.
- Selection resolves `WB_DESC`/`CM_DESC` against camera-ordered CAMF lists, including Merrill's `Daylight` → `Sunlight` alias. Missing/unknown choices require manual selection. Launch overrides survive file changes; other camera choices reset per image. WB becomes required at calibration/gain; color mode becomes required at color conversion. Export always renders the final TIFF recipe and refuses existing files.
- One active job, one replaceable pending request, and one retained decoded source bound processing. The worker waits for GPU completion; the UI rejects stale revisions and preserves the current frame's settings/provenance while updating. General intermediate/multi-image caching and comparisons remain deferred.
- Named output roots determine execution and lifetimes independently. The engine can retain black levels/histograms and floating RGB/scalar images; the GUI offers images only. Requesting the black histogram skips mean computation. Required side outputs remain transient unless retained as roots. Retained/imported snapshots keep their owned storage. Scalar storage supports future masks; highlight-mask generation remains pending.
- Presentation materializes a full-resolution RGBA32Float texture alongside the retained working buffer. Manual filtering and working-space conversion provide sRGB SDR viewing. Sensor layers initially map nominal 0–4095 to gray; scalar fields map 0–1 directly. Preview and pipeline share fit/pan/zoom geometry. Navigator dragging preserves the grab offset; double-click in either view toggles automatic Fit and the persisted snap zoom (default 200%). The navigator reports physical-pixel magnification and offers 100%/200%/400% presets below its image. Direct-buffer presentation, compact textures, and dedicated minification filtering are future optimizations.
- Scopes analyze the displayed GPU buffer on the background worker. Switching scope type or display channel reuses the results. The [viewport toggle](./viewport-scopes.md) follows pan and zoom without rendering a new image. Sampling and display rules follow [GUI workspace](../docs/specs/gui-workspace.md).
- Eframe uses stable app ID `foveonic` and platform application storage. Window/pane geometry, icon/list mode, scope mode, and tool order/collapse persist. Editing settings remain session-only. Missing/corrupt preferences default and duplicate section entries normalize. Opt-in native smoke checks use isolated `app.ron` storage.

## Owner review corrections

Removed view-only exposure, GUI table plotting, and the superseded RGB-gain/linkage path. One `Settings::sensor_gain` scales signed calibrated layers before repair/color conversion, neutral at 1×; nonneutral recipes expose `gain.layers`. CLI callers use `--sensor-gain`.

A pointer fixture reproduced a slider rerender regression caused by replacing the slider response to add double-click sensing. Controls now compare actual before/after samples. Focused numeric fields nudge by 0.1 for floats or 1 for integers, with per-parameter overrides, bounds enforcement, preserved precision/focus, and unchanged mouse-drag sensitivity.

The cleanup pass shares viewport/navigator mapping, isolates native smoke orchestration in `smoke.rs` with named state, exercises actual worker submit/clear methods in its pending-request test, and consolidates historical progress notes. Processing defaults and scheduling policy are unchanged.

## Resize correction

Owner review identified image movement during window/pane resizing. Magnification was relative to a freshly computed fit scale, and placement used the changing canvas centre plus an unchanged pan offset. The one-shot 100% command consequently did not survive resizing at native-pixel scale.

The viewport now separates automatic Fit from a fixed UI-points-per-image-pixel scale. Resizing compensates the centre-based pan offset to preserve continuous top-left crop coordinates; fully visible axes stay centred. JPEG and GPU layout share the correction, and smoke callers use the fixed-scale zoom operation. Processing and render scheduling are unchanged.

All eleven viewport tests and the GUI suite pass (31 passed, one GPU fixture ignored). Coverage includes fractional crop origins in both orientations, width/height changes, canvas translation, Fit/letterboxing transitions, pointer wheel/drag, 100% at 1×/2× display density, and matching resized JPEG/GPU bounds. Documentation generation/checks and tool tests, changed-file formatting, and diff checks pass. Workspace tests and GUI Clippy were blocked by concurrent processor work: missing `camera_repair_test.rs` and the unused `repair::Prepared::params` field, respectively. Native interaction review remains pending; processor files were not changed for this correction.

## Zoom interaction correction

The main canvas requested drag-only sensing, so its double-click branch did not receive clicks. Both images now sense clicks and drags and share the Fit/snap toggle. Navigator clicks while fitted leave automatic Fit intact, avoiding a first-click state change before the second click. Scroll, double-click, presets, toolbar 100%, and relative zoom now share one scale-at-anchor operation. Main-image double-click preserves the image point under the cursor, matching scroll zoom. Navigator double-click centres the clicked image location with the existing edge constraints. Presets preserve the centre image point; the readout includes display density and reports the actual percentage in Fit.

Command-click on macOS (Ctrl-click elsewhere) selects a 100%/200%/400% snap target without changing the view. The typed application preference defaults to 200%; the viewport keeps it through image resets and the app loads/saves it with UI preferences. An accent underline marks the target independently of current zoom. Navigator header and preset-button popovers explain the gestures and persistence. Selection also works before an image is open.

All seventeen viewport tests pass, including cursor-point invariance during main-image double-click, clicked navigator locations and edge constraints in both orientations, every snap target in both views, Command/Ctrl selection with unchanged views and a single matching underline, normal-click independence from snap, preset readouts at 1×/2× density, and existing resize/clip/drag checks. Preference tests cover default 200%, loading existing preferences without the new field, and round-trip persistence of all three targets; help tests cover platform modifier descriptions. Density-changing fixtures settle egui's initial layout before locating click targets. The GUI suite passes (48 passed, four opt-in GPU/fixture tests ignored), as do GUI all-target Clippy, documentation generation/checks and tool tests, changed-file formatting, and diff checks. Native visual review remains pending.

## Navigator scroll correction

Owner screenshots showed the navigator image compressing as it scrolled above the window. egui-wgpu clamps callback viewports to screen bounds, but the shader still mapped the full canvas into the shortened viewport. GPU presentation now intersects the callback canvas with the UI/screen clip and recalculates mapping against the unchanged image rectangle. The same path serves the main image; navigator layout, overlay, and interaction geometry remain unchanged.

The regression test covers portrait/landscape images, clipping at each edge, overlay/sample alignment, fully hidden canvases, and 1×/2× display density. GUI tests pass (38 passed, two GPU fixtures ignored); native visual review remains pending.

## Correction controls

- [x] Combine the stage header/enable toggle into Noise and detail and rename its advanced subsection to Advanced noise and detail; keep reconstruction internal and migrate help keys, tests, and user instructions.
- [x] Move ColorDQ gain into the enabled reconstruction controls, retaining its inline A toggle and removing the inactive warning.
- [x] Rename camera-record repair to Bad-pixel repair, with a default-enabled checkbox inline to the left of its label.
- [x] Remove the GUI repair cutoff slider/value/A toggle; use camera selection internally and retain CLI `--repair-badness` for diagnostics.
- [x] Explain camera-based pixel/cluster repair and targeted red-pixel detection separately from Despeckle's search for additional isolated dots.
- [x] Replace old cutoff-control tests with automatic-only repair toggling and reconstruction-local ColorDQ layout checks.

Owner review supersedes the earlier repair cutoff A-toggle design: the cutoff selects recorded defects rather than a photographic correction amount. ColorDQ retains automatic/manual choice with a manual starting gain of 1. Moving its control does not change shared engine/CLI ColorDQ behavior. Noise preset/Detail explanations remain in hover help. GUI tests pass (61 passed, five opt-in tests ignored), along with strict all-target GUI Clippy, documentation generation/checks and tool tests, changed-file formatting, and diff checks. Native visual review remains pending.

## Organizer keyboard navigation

- [x] Add focused Left/Right and row-wise Up/Down selection to icons and lists through the existing selection/render path.
- [x] Keep one stable organizer focus target across virtualized rows and repaints; release it on outside clicks or Tab without intercepting editing-field arrows.
- [x] Scroll keyboard selections into view and cover repeats, edges, short final rows, column reflow, and focus transfer with egui input fixtures.

Arrow navigation was previously absent. Individual tiles now receive pointer clicks without becoming separate keyboard focus targets. Focus is acquired on pointer press so egui installs the arrow-key filter by release, including the first arrow immediately after clicking. Virtualized row heights now match the actual tile/list heights; egui adds item spacing itself. This removes the previous spacing mismatch and makes offscreen selection scrolling accurate. The GUI suite passes (60 passed, five opt-in tests ignored), along with strict all-target GUI Clippy, changed-file formatting, documentation generation/checks and tool tests, and diff checks. Native owner interaction review remains pending.

## Evidence and remaining work

- `.local/gui-cleanup-checks/{fixture}/` contains twelve-view native macOS checks on all six originals after cleanup. Viewport/navigator pixel comparisons match `.local/gui-sensor-gain-checks/` except for one organizer hover tooltip and its shadow overlapping the canvas; processing/display geometry is unchanged.
- Workspace tests, all ignored release GPU/fixture tests, all-target Clippy with warnings denied, documentation/index checks, formatting, and diff checks pass after cleanup.
- `.local/sensor-gain-checks/neutral/` contains six TIFFs byte-identical to the pre-GUI baseline. `.local/sensor-gain-checks/gain-1.3.tiff` verifies migrated CLI export.
- `.local/gui-workspace-restart-checks/` verifies restored organizer/scope/tool preferences, neutral session exposure, and isolated preference storage.
- Unit/GPU fixtures cover output dependencies/retention, signed scalar/sensor gain, scopes, camera defaults, preview geometry, metadata, preferences, and pointer/keyboard interactions. Native checks so far are macOS only.
- Earlier `.local/gui-checks/`, `.local/gui-layout-checks/`, `.local/gui-workspace-final-checks/`, and `.local/gui-review-adjustments/` are historical milestone evidence. Linked-RGB captures and adjusted RGB-gain recipes are superseded.
- Initial prototype runs measured 458–671 ms for initial decode/render/view and 59–167 ms for subsequent inspection without checkpoints. These are local timing observations.

Owner interaction review, cross-platform runtime checks, checkpoint decisions, monitor profiles, non-image diagnostics, comparisons, and DNG remain pending.

## Checks

```sh
cargo test --offline --workspace
FOVEONIC_FIXTURE_DIR=/Users/work/git/foveonic/references/x3f cargo test --offline --release --workspace -- --ignored --test-threads=1
cargo clippy --offline --workspace --all-targets -- -D warnings
cargo run --offline -p docs-index
cargo run --offline -p docs-index -- --check
cargo fmt --all -- --check
git diff --check
cargo xtask bundle --offline
FOVEONIC_GUI_SMOKE_DIR=.local/gui-review target/release/Foveonic.app/Contents/MacOS/foveonic-gui /Users/work/git/foveonic/references/x3f/blue-parking-sign.x3f
```

## Reference findings

PhotoCraft at `953390a3` declares MIT OR Apache-2.0 and uses egui/eframe with a custom egui_wgpu viewport callback and muda native macOS menus: [`desktop dependencies`](https://github.com/storytold/photocraft/blob/953390a36ef59c6ca04c5850b5b5d9d0dd9e625a/apps/photocraft/Cargo.toml), [`GPU canvas`](https://github.com/storytold/photocraft/blob/953390a36ef59c6ca04c5850b5b5d9d0dd9e625a/crates/ui-egui/src/gpu_canvas.rs). Its processing path uploads a CPU composite. Reference use was limited to architecture research.

The owner suggested [RapidRAW](https://github.com/CyberTimon/RapidRAW). At `6bccda59`, Tauri/React controls overlay a native wgpu surface, passing physical-pixel image/clip bounds and transforms; the processed texture binds directly to display. That path is disabled on Linux/Android in the inspected revision. The README identifies AGPL-3.0. Reference use was limited to architecture research. Sources: [`gpu_processing.rs`](https://github.com/CyberTimon/RapidRAW/blob/6bccda59f80a5e00a575007eed204ec5dc49b34c/src-tauri/src/gpu_processing.rs), [`Editor.tsx`](https://github.com/CyberTimon/RapidRAW/blob/6bccda59f80a5e00a575007eed204ec5dc49b34c/src/components/panel/Editor.tsx).
