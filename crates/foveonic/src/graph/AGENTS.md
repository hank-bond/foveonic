---
summary: "Graph operations define supported node parameters, typed ports, cache keys, and named output dependencies."
---

# Graph definitions

The [graph structure](../graph.rs) validates connections and traverses dependencies. Keep the supported operation enum and its port metadata together. Recipes author connections, while execution stays in [the renderer](../render/AGENTS.md).

Preserve named output identities and exact parameter keys during organizational changes. Their identities drive inspection and interactive reuse.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[operation.rs](./operation.rs)** Defines supported operations, parameter keys, typed ports, and named side outputs.

<!-- INDEX:END -->
