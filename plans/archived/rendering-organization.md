---
status: archived
summary: "Group processing nodes and separate recipe construction from execution before adding monochrome rendering."
---

# Organize rendering code

Requirements: [processing graphs](../../docs/specs/processing-graphs.md), [render resources](../../docs/specs/render-resources.md), [pipeline inspection](../../docs/specs/pipeline-inspection.md), and [output strategies](../../docs/specs/output-strategies.md).

## Scope

The owner approved a behavior-preserving reorganization. Group sensor, color, and finishing nodes with their tests and shaders. Separate settings, authored recipe construction, execution, and artifact collection. Keep the existing public API and one enum-based graph executor. Monochrome algorithms, controls, and DNG representation are separate work.

## Steps

- [x] Inspect the current engine and reference implementation, including local reference changes.
- [x] Run baseline workspace tests and capture an all-enabled photographic float/TIFF comparison.
- [x] Group node implementations, shaders, GPU resources, and graph definitions.
- [x] Extract settings, shared technical construction, photographic/DNG recipe builders, execution, and artifacts.
- [x] Check workspace, GPU/fixture tests, exact photographic output, formatting, strict linting, and documentation indexes.

## Findings

The reference implementation shares original-source preprocessing before a distinct monochrome filtering/detail path. It bypasses color matrix, ColorDQ, and color residual-gain reconstruction. Mono sharpening and tone also have different behavior. Rendering mode and output strategy are separate choices. The subsequent implementation and DNG boundary are recorded in the [monochrome plan](../monochrome-rendering.md).

The existing graph already supports typed nodes, selective side outputs, guarded storage, and interactive reuse. Preserve lazy metadata preparation for early inspection outputs and DNG's stop before photographic exposure/tone/saturation. The public `render`, `noise`, and image APIs remain stable.

## Results and checks

Sensor, color, and finishing groups now own their Rust implementations, adjacent tests, and shaders. GPU registration, guarded storage, and scratch reuse have a separate directory. Graph structure is separate from operation definitions. The public renderer is about 310 lines, with settings, execution, artifacts, and authored recipe builders in child modules. No empty monochrome implementation or new processing settings were added.

All 25 moved shaders are byte-identical. The all-enabled blue-sign comparison uses Sunlight/Vivid, reconstruction, sharpening 0.75, exposure +0.5, contrast 0.2, saturation 1.15, and 800×1200 output. Its full-sensor float buffer and exported TIFF match the pre-reorganization output byte-for-byte.

Workspace tests pass, including 73 processor tests and 110 GUI tests. All 62 ignored processor GPU/fixture tests and eight ignored GUI GPU/worker tests pass serially, including selective inspection, interactive ownership/reuse, DNG appearance separation, and sample encoding. Strict all-target Clippy, formatting, documentation generation/checks, and diff checks pass. Documentation links and fixture include paths follow the new locations.

```sh
cargo test --offline --workspace
FOVEONIC_FIXTURE_DIR=path/to/x3f cargo test --offline --release --workspace -- --ignored --test-threads=1
cargo clippy --offline --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo run --offline -p docs-index
cargo run --offline -p docs-index -- --check
git diff --check
```
