---
summary: "Run independent Merrill camera-cluster searches concurrently while preserving native ordering and exact outputs."
status: archived
---

# Independent camera-cluster scheduling

The owner approved integration after an isolated experiment. Requirements: [processing graphs](../../docs/specs/processing-graphs.md). This implementation slice is complete.

## Implementation

Prepare batches from the complete cluster search rectangles. Searches with disjoint rectangles may run concurrently. Every overlap keeps its original camera order across dispatches. One GPU workgroup searches each cluster, while its members retain their stable quality order, native candidate traversal and tie-breaking rules. Member and mask offsets remain unchanged when descriptors are grouped into batches.

The existing prepared repair owns the batches and includes them in its exact key and CPU resource accounting. GPU execution reuses the existing descriptor, member and mask buffers. There is no alternate repair algorithm or experimental runtime switch.

## Validation

Eleven camera-repair checks pass, including first-candidate ties, empty donor searches, native numerical fixtures, overlap dependencies and exact ordered-versus-batched pixels and masks. Six complete native Full photographic float comparisons with reconstruction enabled remain byte-identical. Three Thumb photographs match the unchanged renderer across all alternating timing trials.

Three alternating process pairs per photograph measured these medians:

| Photograph | Cluster GPU time | Complete fresh Thumb Wall time |
| --- | --- | --- |
| Hazy coast | 29.9 → 9.4 ms | 120.4 → 99.7 ms |
| Yellow flowers | 29.8 → 9.4 ms | 122.4 → 102.0 ms |
| Low-light speaker | 13.9 → 10.4 ms | 101.9 → 100.7 ms |

Hazy coast and yellow flowers have five independent clusters and 36 members. The speaker has two independent clusters and 13 members, with appreciable GPU timing variation and little overall render improvement. These are resident-decoded-source engine measurements. Decode, export, retained edits and application delivery are outside those rendering intervals. Full end-to-end speed was not measured. Workload monitoring found no competing app or compiler.
