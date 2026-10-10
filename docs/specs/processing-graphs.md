---
summary: "Internal processing DAGs use typed connections, shared metadata, node settings, and selectable algorithms for sensor and tone processing."
kind: reference
status: accepted
implementation: incomplete
---

# Processing graphs

## Requirements

- The engine **must** construct and execute an internal directed acyclic graph (DAG) for the selected processing configuration.
- DNG and TIFF **must** have distinct graph recipes built from the same node library and executor.
- Recipes **must** support omitting stages, inserting stages at different positions, and selecting different packaging nodes.
- Connections **must** identify the supported data format and processing space. The graph **must** reject incompatible connections.
- Nodes **must** be able to access the input file's metadata.
- The graph **must** distinguish sensor-signal processing from color and tone processing.
- Supported operations **must** allow alternative algorithm implementations with their own settings and dependencies.
- User-adjustable values **must** be node settings. Graph structure and wiring remain internal to the application.
- Nodes **may** keep intermediate data internal, including color fields created during highlight reconstruction.

## Conformance

The engine constructs and runs both output recipes using shared operations. It rejects an unsupported data-space connection. Selecting a supported algorithm or changing its settings selects the corresponding processing behavior.

## Implementation latitude

Execution may begin with straightforward dependency-order scheduling. Add caching and scheduling machinery when concrete responsiveness needs justify it.
