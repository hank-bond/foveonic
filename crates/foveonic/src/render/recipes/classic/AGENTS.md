---
summary: "Classic recipe preparation resolves camera metadata into functional operation parameters."
---

# Classic preparation

Keep camera defaults and supported metadata branches beside the Classic recipe. Prepare each stage only when the requested output reaches it. Pixel arithmetic belongs in the shared processing nodes.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Directories

- **[prepare/](./prepare/AGENTS.md)** Classic preparation separates calibration, color transformation, reconstruction, and appearance.

### Source files

- **[correction.rs](./correction.rs)** Prepares metadata controls for signed drift, shield offsets, and polynomial correction.
- **[highlight.rs](./highlight.rs)** Selects Classic camera highlight controls before an architecture-unaware row stream.
- **[inspection.rs](./inspection.rs)** Lists visual outputs and comparable operation inputs for the selected Classic route.
- **[policy.rs](./policy.rs)** Resolves Classic camera defaults into numerical recipe controls.
- **[prepare.rs](./prepare.rs)** Shares Classic preparation contracts and metadata helpers between calibration, color, reconstruction, and appearance.
- **[range.rs](./range.rs)** Resolves high-ISO camera metadata into functional reconstruction and color controls.
- **[reference.rs](./reference.rs)** Defines shared numerical conventions recovered from the Classic reference implementation.
- **[route.rs](./route.rs)** Selects Classic stages without preparing later numerical parameters.
- **[spatial_gain.rs](./spatial_gain.rs)** Selects a cached white balance's closest preset spatial-gain table.
- **[white_balance.rs](./white_balance.rs)** Prepares source-dependent neutral and channel-mean statistics for Classic white balance.

<!-- INDEX:END -->
