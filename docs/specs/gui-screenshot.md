---
summary: "An explicit Cargo task refreshes a lossless WebP README screenshot using the shared local example-photograph cache."
kind: reference
status: accepted
implementation: conforming
---

# GUI screenshot

## Requirements

- `cargo xtask screenshot` **must** refresh `crates/foveonic-gui/assets/gui-screenshot.webp` from a real GUI frame, without the native title bar or window shadow. The README **must** reference this asset without rewriting the owner's prose.
- Capture **must** use a checked-in recipe identifying the example photograph, photographic settings, window dimensions, pane widths, zoom, and pan. Its proportions **must** follow the owner's condensed reference rather than a wide landscape layout. Use the yellow-flowers-and-stroller photograph shown in the owner's example, with the icon organizer, navigator, and waveform scopes visible.
- Capture **must** wait for the processed image, matching scopes, navigator, and requested organizer thumbnails. It **must** isolate UI preferences and ignore local editing sidecars without modifying them. Machine-dependent processing duration **must not** appear in the screenshot.
- Encoding **must** use lossless WebP and preserve the captured pixels. Failed capture or encoding **must not** replace the previous asset. The task **must** close only its own app process and report capture failures or timeouts.
- `cargo xtask examples sync` **must** download missing X3Fs from the supplemental `example-photographs` release into ignored `.local/example-photographs/` storage. A source-directory override **may** select another local cache.
- Sync **must** preserve existing originals and editing sidecars. Existing originals **must not** be replaced or integrity-checked. Newly downloaded files **must** be staged before import.
- Screenshot generation **must** sync the full published X3F collection before capture. `--offline` **must** avoid network access, use the last successfully synced asset inventory, and report missing cached requirements. It cannot discover release assets added since that sync.
- Source downloading **must** have one implementation in the Cargo task. Existing photographic-help tools and documentation **must** migrate to that command rather than retain a Python downloader.
- Ordinary builds and release CI **must not** download originals or regenerate the screenshot. Screenshot generation remains an explicit local operation requiring a working graphical/GPU session. Generated assets remain uncommitted until owner review.

## Validation

Native capture produces a 2650×2194-pixel application-content image from the condensed recipe at 2× display density. Online and offline sync, preservation of originals and sidecars, lossless encoding, and automated checks are recorded in the [implementation plan](../../plans/archived/gui-screenshot.md). The owner approved the condensed capture and changes for commit.
