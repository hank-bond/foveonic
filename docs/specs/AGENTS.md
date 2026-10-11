---
summary: "Specifications record agreed application, graph, and output behavior independently of implementation progress."
---

# Specifications

Read the specification relevant to the work and its linked dependencies. Follow [`contributing.md`](../contributing.md) when changing requirements or their status. Acceptance records owner agreement. Implementation status records whether code and evidence satisfy the requirements.

These specifications capture the agreed scope. Detailed algorithm, format, and interface choices are discussed with the owner as implementation reaches them.

<!-- INDEX:START -->

<!-- Generated from document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

- **[application-workflows](./application-workflows.md)** _(accepted, incomplete, reference)._ A personal Merrill X3F app shares processing settings and one image engine between its GUI and batch CLI.
- **[gui-workspace](./gui-workspace.md)** _(accepted, conforming, reference)._ The GUI uses a persistent three-pane workspace with a reflowing organizer, current-buffer scopes, and reorderable editor sections.
- **[output-strategies](./output-strategies.md)** _(accepted, incomplete, reference)._ Compressed DNG preserves editing latitude and embeds color-mode presentation, while TIFF bakes the selected rendering.
- **[pipeline-inspection](./pipeline-inspection.md)** _(accepted, incomplete, reference)._ Pipeline inspection requests named node outputs, executes their dependencies, and retains only selected artifacts and checkpoints.
- **[processing-graphs](./processing-graphs.md)** _(accepted, incomplete, reference)._ Internal processing DAGs use typed connections, shared metadata, node settings, and selectable algorithms for sensor and tone processing.
- **[render-resources](./render-resources.md)** _(accepted, conforming, reference)._ Owned GPU render results separate image processing from display and export, with explicit pixel meaning and geometry.

<!-- INDEX:END -->
