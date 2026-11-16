---
summary: "Versioned performance observations preserve matched workloads, hardware and accepted output identities."
---

# Performance baselines

Fresh-processing reports use the shared [fresh-processing comparison schema](../../../docs/profiling.md#fresh-processing-matrix-comparisons). Preserve recorded reports. Add a separately reviewed checkpoint when changing accepted output or release observations.

The initial `v0.8.0/apple-m1-pro/fresh-processing.json` records the owner-accepted `8c0da85` renderer across 144 cases declared by the tracked test-input manifest. It measures fresh graphs after decoding, with one excluded warmup and five measured iterations. The report includes actual settings, output hashes and geometry. Timings are observations rather than approved latency limits. Retained edits, native-detail transitions and application delivery require separate scenario evidence.

Use the maintained input setup to recreate workloads from pinned originals. Keep raw captures as separate artifacts and verify their complete report against the recorded evidence hash. Export portable reports with `pipeline_benchmark.py export`; do not replace accepted image identities automatically after a failed comparison.

`v0.8.2/apple-m1-pro/native-release-observations.json` records the post-release comparison of the published 0.8.1 and 0.8.2 Mac apps. It preserves matched Off timing samples, separate sampled-footprint peaks and logical resource inventories, diagnostic GPU observations and source-report hashes. It also retains unavailable comparisons and repeated measurements rather than declaring a new performance budget. This compact summary is not a raw `regression.json` input to `compare.py`. Recreate matched native comparisons using the [release performance checks](../../../docs/releases.md#compare-performance-before-publication). Raw reports and traces remain separate evidence.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

_(none yet)_

<!-- INDEX:END -->
