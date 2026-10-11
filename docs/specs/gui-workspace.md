---
summary: "The GUI uses a persistent three-pane workspace with a reflowing organizer, current-buffer scopes, and reorderable editor sections."
kind: reference
status: accepted
implementation: conforming
---

# GUI workspace

## Requirements

- The workspace **must** keep an organizer on the left, the selected image in the middle, and tools on the right. The organizer **must** offer icon and compact-list modes; resizing its pane **must** reflow the icon columns.
- Navigator, Scopes, Editing, and Metadata sections **must** be collapsible and reorderable within the right pane. Floating sections and docking in other panes are deferred.
- The navigator **must** show the visible image region and support dragging to pan the main viewport. Double-clicking it **must** restore centered fit, showing the whole image.
- One persistent Scopes section **must** switch between RGB histogram, RGB parade, and combined waveform. Scopes **must** analyze the currently inspected buffer. Signal labels and ranges **must** respect the output's meaning as described in [pipeline inspection](./pipeline-inspection.md).
- Adjustment rows **must** use label, slider, then editable value order. Double-clicking a slider **must** reset its existing processing default. A focused value box **must** support Up/Down nudging, using 0.1 for floats and 1 for integers unless the parameter specifies another unit. Nudges **must** respect bounds, retain text editing/focus, and request a rerender when the value changes. Exposure **must** be offered solely as a processing adjustment.
- Editing **must** offer one global Sensor gain in a Sensor subsection, neutral at 1×. Gain processing follows [processing graphs](./processing-graphs.md).
- Aperture **must** display an integer or one decimal place, such as `f/4` or `f/2.8`. Shutter **must** display a photographic fraction or seconds. Prefer recorded display labels when available.
- Metadata **must** include available camera, lens, focal length, aperture, shutter, ISO, exposure compensation, exposure program, metering, white balance, dimensions, capture date, software, serial, color mode, and color space. Show only fields available in the file.
- A compact information bar **must** sit below the main image viewport, showing photographic metadata, filename, and visible-region dimensions and coordinates. Its layout and remaining coordinate questions follow [image information bar](./image-information-bar.md).
- UI preferences **must** survive application restarts using platform-appropriate application storage. Persist pane/window geometry, organizer mode, section order/collapse state and selected scope as their controls are implemented. Keep the application identity stable.
- UI settings **must** save the last successfully opened organizer folder and reopen it on application startup. An explicit launch path **must** take precedence. Opening a file remembers its containing folder; unavailable saved folders **must** report the error without opening a different folder or blocking use of the app.
- Editing changes are session-only in the current GUI. Per-file sidecars and photographic-settings persistence are deferred; UI-preference storage **must** be limited to workspace choices and geometry.

## Implementation direction

Use eframe's built-in persistence with app ID `foveonic`. Store explicit UI preferences separately from processing `Settings`; eframe/egui handle window and widget layout memory. Missing or unreadable preferences use defaults. Native smoke tests use isolated storage.

Scopes sample an oriented grid of at most 512×512 from the retained current floating buffer, with count-only readback. RGB stays in its working primaries and linear transfer state; the combined RGB waveform includes matching sRGB or Adobe RGB luminance. Sensor/calibrated layers and pre-tone RGB use a nominal 0–4095 range, scalar fields use 0–1, luminance ratios use 0–2, and signed reconstruction detail uses −1000…1000 camera codes, with out-of-range counts shown. Scopes use the buffer's channel values independently of display selection. Inspection offers visual image buffers; node-data diagnostics are deferred.

The owner's Capture One screenshots guide the three-pane arrangement. The local `x3fuse-multiplat` checkout at `7ddebcd` provides visual and behavioral references for navigator, scopes, metadata, and dark neutral styling. Its current branch is `feature/tauri-dng-looks`. These references inform layout and interaction.

## Conformance

Unit and GPU fixtures cover organizer reflow, pointer-driven section collapse/reordering, pane-local drop limits, navigator click/drag/double-click geometry, slider rerender signals, sensor gain/reset/numeric entry, focused nudging/bounds/parameter overrides, metadata formatting, preference restoration, and oriented signed/scalar/sensor/RGB scope values. Native macOS checks capture the workspace and all scope modes on six originals; a second isolated launch restores organizer, section, and scope choices with default session editing settings. Owner interaction review and cross-platform runtime checks remain in the [implementation plan](../../plans/gui-and-batch.md). Last-folder persistence/launch precedence and visible pixel bounds have unit checks. Six-image native captures and a no-path isolated restart verify the information bar and folder restoration in the [editor review plan](../../plans/editor-review-focus.md).
