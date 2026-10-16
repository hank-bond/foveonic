---
summary: "Explicitly refresh the README's lossless WebP screenshot from a fixed native GUI capture recipe and shared example-photo cache."
kind: how-to
---

# Refresh the GUI screenshot

Run from the repository root on macOS with Xcode and an available graphical/GPU session:

```sh
cargo xtask screenshot
```

This syncs the full supplemental example-photo collection into `.local/example-photographs/`, builds the native app, opens an isolated capture session, and updates [`gui-screenshot.webp`](../crates/foveonic-gui/assets/gui-screenshot.webp). The session closes automatically. Review the image and diff before committing it. The README references that stable asset path.

Capture uses [`gui-screenshot.json`](../crates/foveonic-gui/assets/gui-screenshot.json) for the source photograph, photographic settings, window dimensions and pane widths in points, zoom, and pan. The 1325×1097-point window follows the owner's condensed reference. It shows the icon organizer, navigator, and viewport waveform in a dark workspace. Capture waits for processing, matching scopes, navigator preparation, and organizer thumbnails. The image contains application contents without a native title bar, window shadow, or processing-duration readout. Native display density determines the captured pixel dimensions.

Local editing sidecars are neither loaded nor changed, and UI preferences live in temporary storage. The command clears inherited profiling/smoke modes, encodes lossless WebP outside the GUI binary, verifies the decoded pixels against the capture, and only then replaces the existing asset. A failed capture or a two-minute timeout leaves the previous asset untouched.

## Work offline or use another cache

```sh
cargo xtask examples sync
cargo xtask screenshot --offline
```

Offline mode uses the last successfully synced release inventory and reports missing originals. Run an online sync once to create that inventory. It cannot discover photographs added to the release since the previous sync. Rust dependencies must also be cached for an offline build.

Both commands accept `--source-dir PATH`:

```sh
cargo xtask examples sync --source-dir path/to/examples
cargo xtask screenshot --offline --source-dir path/to/examples
```

The sync command skips existing originals without checking or replacing them, preserves local sidecars, and imports new downloads only after staging them. See [photographic-help maintenance](help-examples.md) for rendering comparisons from the same cache. Ordinary builds and release CI do not download sources or refresh this screenshot.
