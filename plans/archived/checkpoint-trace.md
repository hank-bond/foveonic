---
summary: "Explain repeated work in ordinary app captures through bounded graph demand and checkpoint metadata."
status: archived
---

# Checkpoint trace explanations

This slice extended the [checkpoint profiling findings](./checkpoint-profiling.md) into ordinary manual Perfetto captures, following the [diagnostic contract](../../docs/specs/performance-diagnostics.md). The owner approved the independent review's narrow slice: explain an expensive task's missing outputs, retained data and actual work. Keep cache policy and scheduling unchanged.

## Scope

Cache owns a lazy diagnostic instance identity and self-contained scalar checkpoint snapshots at graph entry. Execution owns per-output demand, availability, dependencies, publication and version linkage. Existing task context, node/GPU timing and worker memory snapshots provide the remaining observations. Native and reduced caches for one photo must remain distinguishable. Warm-start captures must not depend on pre-capture admission events.

The offline report joins these observations. Referenced allocation bytes are deduplicated and remain separate from worker budget totals, driver residency and physical footprint. An absent output does not establish an eviction without a prior matching observation. Truncated or cancelled graphs remain incomplete. The report explains observed work and does not predict counterfactual savings.

No resource registry, per-reference lifecycle tracking, full per-node memory inventory, new UI or policy simulator is part of this slice. Diagnostics retain metadata only and add no GPU waits.

## Checks

- [x] Add shared-boundary metadata without duplicating cache ownership or graph resolution.
- [x] Add an offline explanation report through the existing trace reader and timing analysis.
- [x] Cover multi-output partial reuse, aliases, distinct/replaced caches, warm capture, cancellation and missing trace history.
- [x] Verify capture preserves reader counts, activity and exact pixels.
- [x] Run a maintained app workflow, inspect its report and measure capture overhead separately.
- [x] Obtain independent code/design review, address findings and commit the completed slice.
- [x] Prepare a build for owner manual import/editing captures.

## Evidence and review

A warm-core recording begins after exposure preparation. Its actual Perfetto report contains one complete graph with exact cache/version links and identifies a missing pattern seed alongside cached noise tiles. The ignored Rust test proves identical GPU activity and exact pixels between Off and capture. Native capture leaves headless node/graph profile vectors empty. A gated Python check reads the real recorded metadata and validates dependency and version completeness.

The maintained two-photo cache workflow covers Smart Fit, one native zoom, shared exposure edits and Whole continuation with disk disabled and a 4 GB memory budget. Its first Wall run passed independent behavior and loss checks. The report has 26 graphs, three explicitly incomplete interrupted attempts, three cache identities, no orphans and 22 paired worker budget boundaries. Incomplete attempts are not assigned a cancellation cause merely from missing completion.

Matched Apple M1 Pro overhead runs used three trials per mode, with twelve shared exposure edits each. All trials passed and reported no competing known workload. Median foreground-group completion was 25.78 ms Off and 25.57 ms Wall. Whole settlement observed by the harness was 83.89 ms Off and 94.33 ms Wall, with ranges 71.8–93.1 ms and 79.0–115.4 ms respectively. Report this as slower Whole settlement in the recorded samples. It measures total existing capture overhead plus variation, not the incremental cost of the new metadata. Harness polling and overlapping ranges do not establish a precise causal slowdown. Foreground-group completion does not establish first-result or physical-presentation latency.

The independent design review found no remaining ownership, execution or report blocker. It corrected the distinction between recorded-history validity and unknown pre-capture history, required preserving skipped-producer outputs in the dependency table, and removed timestamp-only processing-scope entries. No policy simulator, reference-lifetime tracker or new runtime owner was introduced.

## Closing checks and owner review

The release workspace suite passed, including 539 processor and 519 GUI tests. Five focused GPU diagnostic tests passed. All forty frozen rendering comparisons stayed bit-exact across three Classic, three Merrill, three ordinary Quattro and one Tiger source at Full/Fast/Draft/Thumb. Strict workspace Clippy passed. Portable report tests and the recorded-metadata integration check passed. Formatting, artwork and documentation/index checks passed.

The rebuilt app is prepared for owner manual import/editing capture. That manual acceptance remains separate from the automated evidence above. Follow-up analysis belongs to the active [cache workflow investigation](../cache-workflow-measurements.md). Cache policy, numerical processing and background scheduling remain unchanged.
