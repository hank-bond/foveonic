---
summary: "Profile conversion, establish owned GPU render resources, and optimize decoding and scratch allocation without interactive caching."
---

# Establish render resources and optimize conversion

The owner approved profiling, a small render-resource contract, and optimizations that fit the future GUI. Follow [`processing-graphs.md`](../docs/specs/processing-graphs.md), [`application-workflows.md`](../docs/specs/application-workflows.md), and [`render-resources.md`](../docs/specs/render-resources.md).

## Steps

- [x] Measure source decoding, host preparation, GPU stages, readback, and packaging before changes.
- [x] Separate owned GPU render results from display and export consumers, with explicit image meaning and geometry.
- [x] Optimize the independent decoder work indicated by measurements.
- [x] Reuse execution scratch without aliasing retained images or changing photographic output.
- [x] Compare original-file outputs, timings, memory, and small resource-lifetime fixtures.
- [x] Obtain owner approval to commit the processor work before starting the GUI.

## Scope and constraints

Interactive result caching and invalidation machinery are deferred. Resource ownership and stage boundaries must permit retained immutable results without making them reusable scratch. No GUI toolkit or monitor-color policy is selected in this work. Compact integer uploads, SIMD conversion, batch overlap, and shader fusion depend on measurements and the resulting resource boundary; they are candidates, not prerequisites.

## Findings

- Baseline six-file stage profiles average about 709 ms of serial sensor-plane decoding, 19 ms of conversion/interleaving, and 7 ms of tone preparation. CPU decoding is the first optimization target; metadata and curve SIMD/caching are lower priority.
- Source decoding now uses three scoped workers for independent planes, followed by a contiguous iterator-based interleave. Errors are Send + Sync for workers and future GUI tasks. A 64-bit lookahead window amortizes bit-reader loads, while consumed bits remain bounds-checked. Zero-padding lookahead never authorizes consuming missing bits.
- Repeated cached-window measurements average about 257 ms for all plane workers to finish and 12 ms for interleaving. Separate 32-bit-load and scalar-lookahead trials took about 282 ms and 311 ms for parallel planes respectively. The initial extend-per-pixel interleave took about 55 ms; a bounded triplet iterator removed that regression. No handwritten SIMD was needed.
- Processing and export remain typed graph recipes. The processing recipe returns an owned floating image; the export recipe imports it and adds encoding and TIFF packaging. Per-value descriptions drive pointwise dimensions and storage sizes. Selected graph roots are retained as extra consumers, providing the ownership boundary for future caches without implementing caching policy.
- Three full-resolution floating buffers suffice for the adjusted recipe: the source allocation plus two new scratch allocations, with eight reuses. Imported resources and retained roots never enter the scratch pool. The pool does not persist between renders.
- The [resource API](../docs/render-resources.md) supports externally supplied device/queue, metadata-sharing snapshots, GPU-only floating texture views, and independent export. Calibration remains full-sensor; preview-resolution processing and DNG/WB separation remain later decisions.
- Three interleaved before/after unprofiled six-file trials on the M1 Pro measured 7.03/6.14/6.08 seconds before and 3.16/3.58/3.35 seconds after. Median batch time decreased from 6.14 to 3.35 seconds (about 45%). Median reported peak footprint decreased from 3.05 to 1.66 GiB. Both use the same native-size controls including ColorDQ, repair, and sharpening. Results are local observations, not performance targets.
- All six adjusted outputs, all six neutral outputs, and the Auto/Vivid/+0.5-stop export remain byte-identical. Small fixtures cover retained intermediate roots, scratch reuse, signed texture values, view orientation, export preservation, shared-device construction, and engine ownership. Original-file testing verifies a result after source drop and another render.
- Profiles, variant trials, original comparison outputs, and the baseline executable are in `.local/optimization/`. Timestamp profiling resolves at graph boundaries; use unprofiled runs for throughput/memory comparisons. GPU timestamps measure compute passes, not allocation/transfer overhead.

## Checks

```sh
cargo test --offline --workspace
FOVEONIC_FIXTURE_DIR=/Users/work/git/foveonic/references/x3f cargo test --offline --release -p foveonic -- --ignored --test-threads=1
cargo clippy --offline -p foveonic --all-targets -- -D warnings
cargo run --offline -p docs-index -- --check
cargo fmt --all -- --check
```
