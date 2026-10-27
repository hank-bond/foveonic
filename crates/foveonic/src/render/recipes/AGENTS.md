---
summary: "Photographic and DNG-base builders compose shared technical nodes before one executor runs the graph."
---

# Authored recipes

Each family owns a recipe entry point and its preparation modules. The dispatcher selects the family once. Recipes choose processing order and pass numerical parameters to camera-unaware operations. Stop construction as soon as all requested intermediate outputs exist. Keep metadata preparation lazy so an earlier output does not require later calibration or appearance data.

Merrill shares its technical construction between photographic and DNG output. Its monochrome route branches after sensor corrections. Classic owns its signed-code calibration and reconstruction routes. Quattro owns its mixed-resolution calibration and later reconstruction. Share numerical operations where their contracts agree, while keeping camera interpretation beside each recipe.

The shared linear-edit recipe owns sensitivity, HDR, Local Contrast, Universal sharpening and DNG gain baking. Each family supplies its linear boundary, primaries, code scale, geometry and tone context. `BuiltRecipe` declares the working RGB extent explicitly; shared stages must not infer it from the camera family or output-encoding dimensions. The linear recipe owns filter support and checkpoint priorities. Each camera adapter owns regional geometry and crops its filter context before pointwise finishing. Preserve the native combined reference operation when edits are neutral where that is needed for exact output.

Rendering mode and export format are separate choices. Photographic recipes apply appearance to pixels. DNG recipes stop at their established linear technical boundary and prepare the selected appearance as numerical matrices and curves for [shared DNG fitting and packaging](../../dng/AGENTS.md). Keep camera metadata interpretation here.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[classic/](./classic/AGENTS.md)** Classic recipe preparation resolves camera metadata into functional operation parameters.
- **[merrill/](./merrill/AGENTS.md)** Merrill recipes share sensor corrections and technical color before photographic or editable DNG output.
- **[quattro/](./quattro/AGENTS.md)** Quattro preparation owns camera resources and source-derived controls beside its multiresolution recipe.

### Source files

- **[classic.rs](./classic.rs)** Composes equal-resolution photographic and editable DNG recipes from signed-code operations.
- **[finishing.rs](./finishing.rs)** Composes shared photographic edits after each camera's base appearance.
- **[linear_edits.rs](./linear_edits.rs)** Composes pre-curve brightness edits and DNG baking using camera-supplied signal context.
- **[merrill.rs](./merrill.rs)** Selects Merrill photographic and editable DNG recipes over their shared technical stages.
- **[mod.rs](./mod.rs)** Selects authored photographic and DNG-base graphs before shared execution.
- **[quattro.rs](./quattro.rs)** Shares Quattro technical reconstruction between photographic and editable DNG output.

<!-- INDEX:END -->
