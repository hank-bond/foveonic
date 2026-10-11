---
summary: "Expose the working conversion engine through a minimal GUI and batch CLI using shared settings."
---

# Connect GUI and batch processing

Requirements: [`application-workflows.md`](../docs/specs/application-workflows.md). Build on [`first-conversion.md`](./first-conversion.md).

## Steps

- [ ] Inventory reference controls and agree on the first supported subset and GUI toolkit with the owner.
- [ ] Add a minimal GUI for selecting an X3F, viewing the processed image, changing supported settings, and exporting.
- [ ] Expose the same settings through CLI batch processing for both output recipes.
- [ ] Review interactive use and a small batch. Record processing gaps and concrete responsiveness problems.

## Progress and findings

No GUI toolkit or settings serialization has been selected. TIFF batch processing is implemented in [`tiff-controls-and-batch.md`](./tiff-controls-and-batch.md); DNG batch support depends on its output recipe. Owned GPU results, texture views, and shared-device construction are available from [`render-resources-and-optimization.md`](./render-resources-and-optimization.md). Interactive caching remains deferred by the owner.

The owner suggested [RapidRAW](https://github.com/CyberTimon/RapidRAW) as a GUI reference. Source inspection at commit `6bccda59` found:

- Tauri and React controls use a transparent webview over a native `wgpu` main-window surface. The processed preview texture binds directly to the display pass.
- The UI sends physical-pixel image bounds, clipping bounds, and zoom transforms. Pan and zoom redraw the existing texture.
- The direct-display path is disabled for Linux and Android in that revision.
- The README identifies AGPL-3.0 licensing. Review reuse terms before importing implementation code.

Relevant sources: [`gpu_processing.rs`](https://github.com/CyberTimon/RapidRAW/blob/6bccda59f80a5e00a575007eed204ec5dc49b34c/src-tauri/src/gpu_processing.rs) and [`Editor.tsx`](https://github.com/CyberTimon/RapidRAW/blob/6bccda59f80a5e00a575007eed204ec5dc49b34c/src/components/panel/Editor.tsx).
