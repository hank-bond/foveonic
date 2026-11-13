---
summary: "Inventory regional storage, active samples and delivered coverage before a separate geometry consolidation slice."
---

# Regional bounds

This read-only architecture inventory follows [regional preview completion](./regional-preview-completion.md) and the [viewport requirements](../docs/specs/viewport-workflows.md). The owner requested investigation of a consistent distinction between expected output bounds and available input bounds. Consolidation remains a separate future implementation unit. No scheduling or cache-owner changes are proposed.

## Current meanings and owners

| Concept | Current representation and owner | Use |
| --- | --- | --- |
| Composed demand | `Settings.preview_region`, authored Crop and Rotation | GUI requests native half-open bounds in the composed photograph, including crop offsets. GUI overscan extends useful delivered coverage. |
| Camera-oriented demand | `rotation::Regional.camera` | The render boundary maps authored demand to camera-oriented native pixels and adds interpolation support. It retains composed tile extent and sampling mapping separately. |
| Physical sensor support | Family regional adapters' `physical` rectangles | Camera adapters undo camera orientation, expand neighborhoods, preserve algorithmic alignment and extract the required calibrated signals. |
| Working storage | `BuiltRecipe.rgb_extent`, then `Description.extent` | Dimensions of allocated samples at the shared RGB boundary. Merrill commonly retains physical sensor framing. Quattro commonly materializes a camera crop before shared edits. |
| Active sample domain | `BuiltRecipe.rgb_bounds` | Active photographic samples on that local working grid, excluding inactive sensor cells. Adaptive Tone uses it for analysis and local probes. It does not identify the requested displayed tile. |
| Camera output view | `BuiltRecipe.params.crop/source/output`, graph output `View` | Local storage crop, camera orientation and output dimensions. Merrill's regional crop lies inside its supported backing buffer. |
| Delivered view | `RenderedImage.view()`, `params()` and optional `Regional` | Maps the requested composed output onto owned storage. Texture preparation, pickers and scopes share this mapping. |
| Delivered coverage | Artifact settings and GUI `Frame.detail_region()` | Places the delivered tile in native composed coordinates and determines usable coverage. Requested settings and actual artifact settings are distinct when rendering falls back to Whole. |

These are related rectangles in different coordinate systems. Camera orientation, authored quarter turns/fine rotation and working scale each belong to an explicit mapping boundary. An available allocation can contain inactive or padded samples. Calling all available samples “valid” obscures the distinction between active sample domain and neighborhood support sufficient for a requested result.

## Findings

The pattern is needed across Color and Monochrome. Their geometry obligations agree even though their camera algorithms and prerequisites differ. Merrill monochrome must finish column correction and recurrence-based histogram work on the whole frame before extracting regional scalar signals. Quattro/Tiger retain whole-frame calibration and color preparation before regional reconstruction. Classic still uses Whole fallback. A common boundary must preserve these choices.

The present concrete conflation is `linear_edits::alias`: it constructs a camera view and then replaces its crop with `rgb_bounds`. A domain suitable for analysis thereby becomes presentation provenance inherited by later graph outputs. The current Merrill regional finishing overrides restore the requested crop at delivery. A future consolidation should make those meanings explicit at the shared boundary instead of relying on final overrides.

Quattro's final `crop_shared_edits` changes `rgb_extent` and encoding crop/output, but leaves `rgb_bounds` describing the expanded tile. No later shared stage currently consumes those fields, so this is a stale handoff invariant rather than an established numerical failure. Generic `ImageRegion` execution likewise inherits an upstream view without translating it into the extracted storage coordinates. Explicit recipe views or materialized-view handling currently determine the delivered result. These are targeted migration sites.

Shared filter support is partly centralized in `linear_edits::support`, while HDR support and alignment are added separately in Merrill and Quattro adapters. Adaptive Tone also requires the HDR guide lattice. A shared declaration of margin and alignment would remove this policy duplication. Camera reconstruction support, mixed-resolution extraction and crop arithmetic remain family-owned. Sequential filter margins accumulate, parallel branches need enough support for their widest dependency, and alignment preserves phase. The first consolidation need not infer these properties from an arbitrary graph.

Adaptive Tone and Dehaze already build whole-photo analysis branches independently of regional application. Their analysis domains must remain whole-photo domains. Adaptive Tone uses `rgb_bounds`, while Dehaze currently receives `params.crop` on its whole-photo branch, where those normally coincide. Consolidation should make that intention explicit without changing the analysis algorithm. HDR and Adaptive Tone application need local support plus stable guide alignment, independent of requested display coverage.

Graph intermediates need storage geometry and stage-specific provenance. They do not all need a composed delivery rectangle. Public intermediate inspection currently falls back to Whole except `linear.base`, and some sensor outputs intentionally expose the complete physical grid. Delivery is the boundary at which a requested view and actual native coverage must be exact. Display and scopes already consume the same `RenderedImage` sampling parameters and should continue doing so.

## Proposed minimum future unit

- [ ] Agree vocabulary: **storage extent**, **active bounds**, **output view**, **camera region**, **composed region**, and **filter support**. Retain the established [viewport vocabulary](../docs/architecture/concepts.md#viewport-and-image-coordinates).
- [ ] Group the existing shared RGB geometry into a small recipe-boundary value containing storage extent, active bounds and output view. Validate nonempty bounds within storage. Derive encoding parameters at their consumers rather than treating their crop as an analysis domain.
- [ ] Make pointwise aliases preserve output view. Give extraction and materialized crops explicit geometry updates. Migrate Merrill and Quattro callers together, including Color, Monochrome, `linear.base`, whole analysis branches and DNG shared editing.
- [ ] Return the shared edit margin and alignment together. Keep camera halos and physical-grid conversion in family adapters. Preserve current numerical support until comparisons justify a change.
- [ ] Retain composed-demand mapping at the render/delivery boundary. Keep `RenderedImage` authoritative for texture, picker and scopes sampling. Avoid adding authored composition metadata to every graph edge.
- [ ] Validate center and physical-edge tiles, active-area offsets, camera quarter turns, saved crop, authored quarter turns and fine angles. Compare delivered geometry and sampled values to Whole, including Adaptive Tone, HDR, monochrome and combined spatial edits. Check that panning reuses whole-photo analysis.

This unit does not require generic coordinate-space type parameters, automatic graph demand propagation, a new scheduler or regional support for every camera. Start with the existing shared RGB boundary and the extraction points implicated by the inventory. The [reproducible test-input contract](../docs/specs/performance-diagnostics.md#reproducible-test-inputs) governs maintained comparisons.
