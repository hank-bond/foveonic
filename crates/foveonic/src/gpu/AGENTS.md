---
summary: "GPU internals register node shaders and protect storage ownership and scratch reuse."
---

# GPU resources

The [device facade](../gpu.rs) owns submissions, transfers, tracing, and readback. Kernel registration loads shaders from their owning node groups. Shared presentation sampling shaders live in `shaders/`.

Keep raw buffers inside the GPU/storage layer. Reads borrow tracked ownership and writes require exclusive guards through submission. Scratch storage is reusable only after its last reader releases it.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[kernels.rs](./kernels.rs)** Registers compute kernels and builds pipelines from the owning node shaders.
- **[scratch.rs](./scratch.rs)** Recycles guarded storage only after the last shared reader releases it on the ordered GPU queue.
- **[storage.rs](./storage.rs)** Prevents a later GPU write from changing an image that still has readers.

<!-- INDEX:END -->
