---
summary: "Clarify the Classic photographic/output boundary, consolidate metadata preparation, and retire obsolete profile machinery."
status: archived
---

# Classic experiment cleanup

Implements the owner-approved cleanup following [shared Classic finishing](classic-finishing.md) and [Classic rendering](../../docs/specs/classic-rendering.md).

## Work

- [x] Move shared RGB spaces, colorants and transfer functions out of sensor calibration and finishing nodes.
- [x] Name the signed photographic response independently of destination conversion and encoding.
- [x] Consolidate Classic metadata defaults and highlight preparation while preserving lazy inspection boundaries.
- [x] Remove the unused legacy profile asset and preparation tool after preserving captured outputs and profile identities.
- [x] Exercise adjusted exposure, tone, saturation, gains, detail and signed table boundaries on the GPU.
- [x] Audit operation ports and exact cache keys without introducing a caching framework.
- [x] Verify unchanged full-image output and run repository checks.

## Resulting organization

The shared `color_space` module owns RGB space definitions, metadata preference lookup, transfer functions and output colorants. The existing public `image::ColorSpace` and `render::TiffColorSpace` paths remain stable. Internal callers use the shared definition directly.

The Classic recipe composes typed operations. Staged metadata preparation selects supported request branches, chroma and reconstruction settings, root/detail parameters and photographic defaults. Camera highlight preparation lives beside Classic preparation. Appearance preparation still occurs only after the relevant inspection stop, so early inspection does not require later color metadata.

`SignedPhotographic` applies the signed root-domain detail, tone and saturation contract. It produces normalized linear ROMM for shared output consumers. It has two inputs: RGB and detail. The former retinex input was a dummy connection backed by an always-neutral factor table. Removing that input and table preserves arithmetic while reducing each prepared table upload and exact key by 256 KiB.

Prepared response fields are private and own their exact cache-key construction. The six retained tables occupy 1.5 MiB. Exact keys intentionally retain their bytes, preserving collision-free parameter equality. Reusing prepared tables or keys is deferred until measured interactive preparation latency demonstrates a need. No hashing scheme or generalized cache layer was added.

The obsolete profile transform and its native-library generator are removed. The captured fixture, profile identities and numerical comparison findings remain in the [fixture documentation](../../crates/foveonic/tests/data/README.md) and finishing plan. The runtime uses no legacy color-management library.

## Validation

Independent table fingerprints retain the recovered-equation checks. Additional scalar/GPU comparisons cover signed wrapping, root joins and plateaus, signed detail, non-neutral gains and three adjusted control combinations. Saturation checks distinguish equal-channel photographic weighting from the floating finishing node's luminance weighting and distinguish saturation before versus after tone. These tests verify execution and ordering, rather than claiming additional native camera coverage.

Full-size SD15, DP2 and DP1s TIFFs remain byte-identical to the pre-cleanup shared-finishing outputs. The retained Merrill rendering also remains byte-identical to main. Named inspection and session tests preserve the earlier reconstruction and profile-difference bounds. Workspace tests, targeted GPU tests, Clippy, formatting, documentation indexes and diff checks validate the final cleanup. Changes remain uncommitted for owner review.

## Interface and numeric-contract follow-up

Named graph output lookup replaces positional output offsets in Classic and the corresponding Merrill recipe callers. Lookup is local to a producer, so repeated inspection names across stages cannot select an unrelated output. Existing typed-port validation still checks connections.

Ordered signed defect repair and dynamic signed despeckling now have separate graph variants and prepared types. Defect records belong only to defect repair. Despeckling owns its geometry and boundary policy. Their GPU arithmetic and named inspection outputs remain unchanged.

Camera preparation now lives beside the Classic recipe under `render/recipes/classic/`, including highlight preparation and tests. Shared operations remain under `nodes/`. This keeps family knowledge in recipe construction and preparation.

The numeric-contract pass names the main signal, table and fixed-point scales and documents curve/window constants. Recovered literals such as the cosine support and cutoff retain their original precision instead of being replaced with mathematical constants. The fixed reference basis matrix is identified as a reference coefficient set whose exact standard color-space identity remains unestablished. Comments distinguish known numerical roles from unknown design intent.

## Megapixel-normalized performance baseline

A release build on Apple M1 Pro measured three warm fresh renders per family, with Sunlight/Standard, reconstruction, repair, highlight recovery, despeckle and unit sharpening enabled. These runs rebuild the complete recipe from decoded samples rather than reusing an edit-session result. Decode, TIFF export and a separate synchronized GPU diagnostic were measured independently.

| Source | Physical processed MP | Median completed processing | Time per processed MP |
| --- | ---: | ---: | ---: |
| SD15, ISO 50 | 4.816896 | 748.874 ms | 155.468 ms |
| Merrill, ISO 100 | 16.084992 | 1046.985 ms | 65.091 ms |

Classic took 2.39 times as long per processed megapixel in this pair. Warm samples were 747.506–803.478 ms and 1033.983–1242.103 ms respectively. The denominator includes processed sensor margins, not just cropped output or advertised resolution. Different photographs, ISO and family algorithms prevent treating this as a universal scaling or matched-quality result.

The separate Classic GPU diagnostic totaled 706.452 ms. Highlight streaming took 312.173 ms, flare statistics 218.996 ms and ordered despeckle 62.872 ms. The first two account for about 75 percent of diagnostic GPU time. Rust color preparation took 0.006334 ms and calibration preparation 0.390667 ms in that diagnostic. Small host matrix calculations prepare coefficients and tables. Per-pixel matrix application remains in WGSL.

In-memory decode wall time was 95.931 ms for Classic and 307.240 ms for Merrill. Median TIFF export was 27.153 ms and 73.438 ms respectively. Decode describes one file opening with uncontrolled filesystem cache state. GPU initialization and shader setup are outside the measured processing calls. Redundant timing TIFFs were deleted after successful export and validation while numerical reports were retained.
