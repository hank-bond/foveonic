---
summary: "Implemented section recipes, configurable defaults, white-balance bias and isolated Before rendering through the existing editor paths."
status: archived
---

# Add editor workflow controls

Requirements: [editor workflow](../../docs/specs/editor-workflow.md), [editing sidecars](../../docs/specs/editing-sidecars.md), [output strategies](../../docs/specs/output-strategies.md), and [GUI workspace](../../docs/specs/gui-workspace.md).

- [x] Define section-scoped recipe application and persisted preset/default libraries.
- [x] Add section and whole-editor reset/preset actions, with Option/Alt for camera reset.
- [x] Apply user defaults only to missing photo recipes and frozen processing requests.
- [x] Preserve As Shot as an authored camera-relative choice and add neutral WB bias.
- [x] Keep WB coherent through DNG neutral and profile composition.
- [x] Add Y/Shift+Y through the shared shortcut registry.
- [x] Render Before separately from authored frames and preview publication.
- [x] Verify state transfer, sidecar precedence, comparison isolation, numerical WB and DNG behavior.
- [x] Run repository checks and prepare the native review build.

The owner specified reset-to-user-defaults on click and camera-as-shot reset on Option-click. Camera WB is represented by gains and camera tables, not a uniformly available exact Kelvin/Tint pair. Relative Warmth/Tint around As Shot is the initial proposal. The UI must not invent a precise camera temperature.

The browser search follow-up is independent: text now matches only filenames and root-relative paths. Capture metadata remains available for sorting. The clear button uses a larger drawn cross inside its existing bounds.

## Shared defaults and authored camera choices

Section identities, scoped photographic copies and default overlays belong beside shared render settings. GUI code owns visual ordering, named preset menus and preference storage. This lets interactive loading, batch preparation and cache preview validation use the same default recipe rather than implementing separate overlays.

Render replies carry resolved camera choices. The editor preserves the author's unresolved As Shot choices when adopting effective settings. Frames also retain the requested recipe for Smart refinement comparisons. The preview cache resolves unresolved camera choices before validating a rendered preview against a saved recipe.

User defaults also need to participate in preview validation for photographs with no saved recipe. Keep saved sidecar settings authoritative and separate from default policy. A preview fingerprint for unsaved defaults must change with the effective default recipe. Embedded JPEG previews remain reusable. The decoded sensor cache and source identity do not change.

## Presentation and WB checks

Before requests and replies carry a transient rendition identity. The current image remains visible while its baseline renders. Its owned GPU frame is retained and restored with a new presentation revision, allowing monotonic navigator and thumbnail consumers to accept it without another render. Returning to Current resets the preview handoff so a current-image thumbnail that finished during Before can be published again.

WB bias uses a shared cone-response matrix at the linear photographic boundary. The GPU matrix operation preserves declared code units. DNG storage coordinates stay unchanged while the neutral and fitted appearance compose the adjustment. The scalar bake reference starts after WB, and chromatic baking uses the inverse biased profile basis. Zero bias takes the original path.

Original-file checks passed for pure WB sample preservation and WB plus chromatic Color Editor reprojection on Merrill, ordinary Quattro, Quattro H and two Classic cameras. Native comparisons passed camera/default toggles, unchanged recipes, viewport retention, exact GPU texture reuse, excluded Before thumbnails and both reset baselines.

Workspace tests, strict Clippy, formatting, documentation indexes, artwork tests and the native bundle passed. The browser and editor work remain uncommitted for owner review.
