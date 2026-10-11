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

The owner selected egui/eframe 0.31.1 with wgpu 24 after inspecting PhotoCraft and approved GUI dependency downloads. The GUI and subsequent processor changes remain uncommitted for review. TIFF batch processing is covered by [`tiff-controls-and-batch.md`](./tiff-controls-and-batch.md); resource ownership by [`render-resources-and-optimization.md`](./render-resources-and-optimization.md).

- The GUI shares eframe's device/queue with the engine. Viewport display uses GPU textures. Native menus and monitor-profile correction remain deferred.
- Browser requests/results are bounded, rows are virtualized, and the thumbnail cache holds at most 256 entries. Seek-based reads decode only the smallest embedded JPEG and read the largest JPEG's dimensions for layout. All six originals contain two JPEG sizes: 221×147 and 4704×3136. Larger-preview decoding remains deferred.
- Selection resolves `WB_DESC`/`CM_DESC` against camera-ordered CAMF lists, including Merrill's `Daylight` → `Sunlight` alias. Missing/unknown choices require manual selection. Launch overrides survive file changes; other camera choices reset per image. WB becomes required at calibration/gain; color mode becomes required at color conversion. Export always renders the final TIFF recipe and refuses existing files.
- One active job, one replaceable pending request, and one retained decoded source bound processing. The worker waits for GPU completion; the UI rejects stale revisions and preserves the current frame's settings/provenance while updating. General intermediate/multi-image caching and comparisons remain deferred.
- Named output roots determine execution and lifetimes independently. The engine can retain black levels/histograms and floating RGB/scalar images; the GUI offers images only. Requesting the black histogram skips mean computation. Required side outputs remain transient unless retained as roots. Retained/imported snapshots keep their owned storage. Scalar storage supports future masks; highlight-mask generation remains pending.
- Presentation materializes a full-resolution RGBA32Float texture alongside the retained working buffer. Manual filtering and working-space conversion provide sRGB SDR viewing. Sensor layers initially map nominal 0–4095 to gray; scalar fields map 0–1 directly. Preview and pipeline share fit/pan/zoom geometry. Navigator dragging preserves the grab offset; double-click restores centered fit and cancels pending 100% zoom. Direct-buffer presentation, compact textures, and dedicated minification filtering are future optimizations.
- Scopes analyze the displayed artifact once per completed image job using a grid of at most 512×512. Only counts are read back. RGB remains linear in working primaries with matching luminance; sensor/scalar ranges and out-of-range diagnostics are explicit. Scope switching, channel selection, and panning reuse the current frame.
- Eframe uses stable app ID `foveonic` and platform application storage. Window/pane geometry, icon/list mode, scope mode, and tool order/collapse persist. Editing settings remain session-only. Missing/corrupt preferences default and duplicate section entries normalize. Opt-in native smoke checks use isolated `app.ron` storage.

## Owner review corrections

Removed view-only exposure, GUI table plotting, and the superseded RGB-gain/linkage path. One `Settings::sensor_gain` scales signed calibrated layers before repair/color conversion, neutral at 1×; nonneutral recipes expose `gain.layers`. CLI callers use `--sensor-gain`.

A pointer fixture reproduced a slider rerender regression caused by replacing the slider response to add double-click sensing. Controls now compare actual before/after samples. Focused numeric fields nudge by 0.1 for floats or 1 for integers, with per-parameter overrides, bounds enforcement, preserved precision/focus, and unchanged mouse-drag sensitivity.

The cleanup pass shares viewport/navigator mapping, isolates native smoke orchestration in `smoke.rs` with named state, exercises actual worker submit/clear methods in its pending-request test, and consolidates historical progress notes. Processing defaults and scheduling policy are unchanged.

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
python3 crates/foveonic-gui/build-macos-app.py --build-only
FOVEONIC_GUI_SMOKE_DIR=.local/gui-review target/release/Foveonic.app/Contents/MacOS/foveonic-gui /Users/work/git/foveonic/references/x3f/blue-parking-sign.x3f
```

## Reference findings

PhotoCraft at `953390a3` declares MIT OR Apache-2.0 and uses egui/eframe with a custom egui_wgpu viewport callback and muda native macOS menus: [`desktop dependencies`](https://github.com/storytold/photocraft/blob/953390a36ef59c6ca04c5850b5b5d9d0dd9e625a/apps/photocraft/Cargo.toml), [`GPU canvas`](https://github.com/storytold/photocraft/blob/953390a36ef59c6ca04c5850b5b5d9d0dd9e625a/crates/ui-egui/src/gpu_canvas.rs). Its processing path uploads a CPU composite. Reference use was limited to architecture research.

The owner suggested [RapidRAW](https://github.com/CyberTimon/RapidRAW). At `6bccda59`, Tauri/React controls overlay a native wgpu surface, passing physical-pixel image/clip bounds and transforms; the processed texture binds directly to display. That path is disabled on Linux/Android in the inspected revision. The README identifies AGPL-3.0. Reference use was limited to architecture research. Sources: [`gpu_processing.rs`](https://github.com/CyberTimon/RapidRAW/blob/6bccda59f80a5e00a575007eed204ec5dc49b34c/src-tauri/src/gpu_processing.rs), [`Editor.tsx`](https://github.com/CyberTimon/RapidRAW/blob/6bccda59f80a5e00a575007eed204ec5dc49b34c/src/components/panel/Editor.tsx).
