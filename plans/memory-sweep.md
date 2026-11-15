---
summary: "Reduce peak and retained application memory through measured, behavior-preserving changes."
---

# Memory sweep

Requirements: [photographic output](../docs/specs/output-strategies.md) and [performance diagnostics](../docs/specs/performance-diagnostics.md).

The owner requested an autonomous sweep, prioritizing peak memory and simpler designs. Preserve export samples and editing behavior. Keep retained-resource estimates separate from measured process or GPU peaks.

- [x] Repack JPEG and RGB8 PNG directly from packed GPU output. This avoids a six-byte-per-pixel RGB16 allocation, approximately 92 MiB at 4928 × 3264. Exhaustive RGB8 PNG quantization and existing encoder tests pass. This is an allocation calculation, not a measured process peak.
- [x] Shorten Color Field solver buffer ownership. Native equations consume 48 bytes per active native pixel and coarse equations and solutions consume 96 bytes per active grid cell. The buffers can be released after their last submitted consumer. Driver residency and process-peak effects have not been measured.
- [x] Release the speculative decoded source before publishing selection assets. This shortens its overlap with publication; a native Merrill source holds approximately 184 MiB of sample data. The earlier decode peak is unchanged. The portable cache tests pass; original-file prefetch tests require declared fixtures.
- [x] Rejected mapped export readback after testing. It removed an eight-byte-per-pixel host copy but kept staging alive through encoding. Three release JPEG exports of each variant, alternated, of the example blue photograph produced identical files and showed no clear peak benefit over the same candidate without mapped readback: RSS 774–777 MiB versus 774–781 MiB, physical footprint 1728–1732 MiB versus 1730–1734 MiB. The mapped implementation was removed. These observations cover one image and one format, not every workload.
- [x] Rejected color texture plane reuse after testing. It reduced scalar allocation requests from nine to three but did not reduce process peak in three alternating full-size JPEG exports per variant with active Fine/Coarse controls. RSS was 779–780 MiB without reuse and 780–785 MiB with reuse. Physical footprint was 2873–2874 MiB without reuse and 2874–2875 MiB with reuse. Files were byte-identical. The change was removed. These results cover one image and recipe.
- [x] Rejected monochrome texture plane reuse after testing. Three alternating fresh full-photo renders per variant with active Fine/Coarse controls showed RSS of 779–783 MiB without reuse and 778–780 MiB with reuse. Physical footprint was 2150–2213 MiB without reuse and 2148–2150 MiB with reuse. The earlier path had one higher-footprint outlier. The cached/fresh and DNG boundary check passed, but the peak benefit was not clear enough to retain the extra reuse machinery.
- [x] Release GUI preview source samples after linearization and unrotated floats after rotation. This shortens overlap by 6 bytes per input pixel, plus 12 for rotated previews. GUI rotation and resize tests pass.
- [x] Widen ICC preview input in bounded chunks instead of a full RGB16 copy, avoiding 6 bytes per preview pixel. Preview tests and an exact multi-chunk comparison pass.
- [x] Stream JPEG XL output through a 64 KiB buffer rather than retain the complete encoded file. Lossless multi-chunk round trip, writer failure and existing encoder tests pass.
- [x] Release RGB16 preview samples before floating resize. Review caught an initial extended float lifetime; an inner scope fixes it. Preview tests pass.
- [x] Prepare rendered display previews from their linearized samples without cloning the RGB16 payload needed for persistence. GUI preview tests pass.
- [ ] Investigate decoded-source storage and other GPU transient allocations. Measure representative workloads before claiming process-level savings or accepting complexity.
- [ ] Run applicable checks and independent review for each change. Record limits and remaining opportunities.

## Release export observations

On this Apple Silicon macOS host, compare release builds from main `fa4a09c` and the sweep with mapped readback removed, using the example release's `blue-parking-sign.x3f` (SHA-256 `7a650537a9986bfe348e0da117defbf80da7ceca35f76bc092f220a7cdbcffee`), Sunlight/Standard and default processing. Three fresh CLI processes per variant alternated, with `/usr/bin/time -l` reporting maximum RSS and peak memory footprint. The same output bytes were produced for every matched run. These are process-level observations for one export workload, not native GUI or retained-session measurements. Process startup and filesystem cache were uncontrolled, and timing is not used as acceptance evidence.

- JPEG: main RSS 807–845 MiB, candidate 774–781 MiB. Main footprint 1731–1735 MiB, candidate 1730–1734 MiB.
- Lossless JPEG XL, effort 1: main RSS 1138–1145 MiB, candidate 900–1011 MiB. Main footprint 1731–1738 MiB, candidate 1731–1734 MiB. All outputs were 85,427,843 bytes and byte-identical.

The first JPEG comparison overlapped a separate build and is excluded. Physical footprint did not show a material improvement in these runs despite reduced RSS and allocation traffic. The initial workspace test run overflowed a GUI test thread's default stack. The complete workspace passed with `RUST_MIN_STACK=8388608`; strict Clippy, formatting, documentation index and Python checks also passed before the mapped-readback removal.
