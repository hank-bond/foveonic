---
summary: "Shared pre-curve edits and DNG baking cover Classic, Merrill and Quattro with per-output section choices."
status: archived
---

# Share linear photographic edits

Requirements: [photographic finishing](../../docs/specs/photographic-finishing.md), [tonal ranges](../../docs/specs/tonal-ranges.md), [output strategies](../../docs/specs/output-strategies.md).

The owner approved extending the reviewed Merrill linear stage to Classic and Quattro, including Tiger. Camera adapters provide signal scale, primaries, geometry and tone context. Sensitivity, HDR, Local Contrast and Universal sharpening share numerical operations and bake into DNG. Camera appearance remains metadata. Preserve neutral/reference rendering and remove redundant format-support badges. All numeric slider readouts must use at most three significant digits without changing stored precision.

- [x] Extract shared linear recipe and clean independent color guide.
- [x] Adapt Classic signed response and Quattro code response at their pre-curve boundaries.
- [x] Share DNG gain baking and include Universal sharpening.
- [x] Migrate inspection, UI, labels and documentation. Remove replaced paths.
- [x] Verify originals, bypasses, cache reuse, regional/reduced rendering and export. Build review app.

Universal sharpening moves before camera tone by owner approval. Its appearance needs review. The owner also approved per-output section switches, remembered independently for each Browse/Import and DNG/TIFF combination. Freeze these switches with a batch and preserve full authored sidecars. Required calibration and reconstruction stay active. No new defaults or commits are authorized.

## Findings and checks

The shared stage uses camera-supplied primaries, code white, actual working-grid scale and lazy tone-pivot preparation. Classic retains its combined signed reference operation when edits are neutral. Quattro adapts its integral pre-response codes and keeps mandatory reconstruction. DNG transfers the same combined scalar gain onto each camera’s established technical base. Native full-frame exports ignore transient preview regions and sizes.

Quattro monochrome repeated three channel-mapping operations under one cache name, invalidating unchanged downstream work. Stable recipe scopes now distinguish those boundaries. Within the existing session budget, admission favors the expensive sensitivity inputs and HDR/Local Contrast boundary over cheaper sharpening/tone outputs. Cached Local Contrast edits reuse reconstruction and the HDR mask.

Linear buffer descriptions carry their nominal white through aliases, imported inputs and materialized textures. Viewport/navigator mapping and scopes use those units. This prevents Quattro’s pre-curve inspection outputs from being displayed at the former fixed 4095-code scale. Numeric formatting lives in the shared widget policy and applies to every slider/readout. Built-in full-precision hints are suppressed while domain help remains available.

Original checks pass in Color and Monochrome on Merrill, Classic, ordinary Quattro and Quattro H. They cover matching baked gains at ordinary and extreme Local Contrast with Universal sharpening, section bypasses, native DNG framing, finite signed/headroom data and cached/fresh equality. Regional/Smart checks pass on Merrill, Quattro and Quattro H with all shared edits active. Pixel crops match Full and continuation reuses sensor work. A synthetic GPU fixture checks equal scope counts at 4095-, 4096- and 16384-code white.

The Quattro H section fixture initially timed out after queuing unfinished full-frame jobs and retaining earlier large buffers. Waiting for each DNG result and releasing prior outputs passes all exclusions. This checks completed work and keeps the test’s peak residency representative of export. UI/settings checks cover four independent preference combinations, default inclusion, frozen section choices and unchanged authored sidecar serialization. Originals and sidecars remain untouched.

The cleanup keeps pre-curve support and cache admission in the linear recipe, and Quattro viewport expansion/cropping in its camera adapter. An explicit working RGB extent removes camera-family decisions from shared finishing. DNG technical settings omit all shared photographic edits; the authored recipe is their single baking source. Resolving the active tone method before preparing DNG metadata also prevents saved inactive Reference Shadows/Highlights from becoming active during export. HDR inspection uses the same input alias on every family. Retired post-curve test adapters are removed, and numerical HDR coverage checks all three declared code whites.

Photographic review of the new Classic/Quattro placement and pre-curve Universal sharpening remains with the owner. Reference stays the default.

Workspace tests, strict Clippy, formatting, documentation indexes, artwork tests and whitespace checks pass. The optimized macOS review bundle builds. No installation, commit or release was performed.
