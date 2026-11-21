---
summary: "Internal processing DAGs use typed connections, shared metadata, node settings, and selectable algorithms for sensor and tone processing."
kind: reference
status: accepted
implementation: incomplete
---

# Processing graphs

## Requirements

- The engine **must** construct and execute an internal directed acyclic graph (DAG) for the selected processing configuration.
- DNG and TIFF **must** have distinct graph recipes built from the same node library and executor. Color and [Monochrome](./monochrome-rendering.md) are independent rendering choices for either output. Mono branches after shared sensor corrections, before color conversion.
- Recipes **must** support omitting stages, inserting stages at different positions, and selecting different packaging nodes.
- Linear RGB code buffers **must** retain their declared nominal white through processing descriptions, materialized textures, inspection, navigators and scopes. Repeated numerical operations at different recipe boundaries **must** have distinct cache identities.
- Connections **must** identify the supported data format and processing space. The graph **must** reject incompatible connections.
- Nodes **must** be able to access the input file's metadata.
- The graph **must** distinguish sensor-signal processing from color and tone processing.
- Calibration **must** preserve camera-derived gains, signed samples, and headroom. There is no user-adjustable global Sensor gain.
- Camera-record bad-pixel repair and [dynamic despeckling](./despeckle.md) **must** have separate shared GUI/CLI controls and both **must** be enabled by default. Camera-record repair **must** use the reference's camera-derived minimum badness for each source unless an explicit manual cutoff in 0–15 is selected. The order **must** be calibration, enabled camera-record repair, enabled [highlight restoration](./highlight-restoration.md), enabled despeckling, enabled [row-banding correction](./row-banding.md), then noise/detail reconstruction and camera color. Each correction **must** be independently disableable. The camera-record node **must** detect bad reds from immutable original sensor samples with the unscaled camera noise model, preserve duplicate detections and main-list override masks, then apply main, JPEG, detected, and distinct packed-cluster repairs in reference order. Pixel lists **must** preserve native read/write dependencies, duplicates, and neighbor masks, use the reference's trimmed/chroma-anchored arithmetic, and read prior repairs so adjacent marked defects are corrected rather than skipped. Independent records **may** execute together only when equivalent to native ordered execution. Packed clusters **must** preserve stable quality ordering, fixed known-patch masks, progressive template masks, and native masked patch matching. Cluster searches with disjoint complete search rectangles **may** execute concurrently. Overlapping searches **must** retain camera order, and members within a cluster **must** remain ordered. Camera mode **must** select pixel and cluster badness using their respective camera offsets; a manual value selects both the minimum pixel badness and exact cluster class. Earlier inputs and retained artifacts **must** remain immutable. The cutoff-default change is tracked in the [camera repair plan](../../plans/archived/camera-repair-default.md).
- Noise and detail processing follows [detail and noise processing](./detail-and-noise.md). Within full-resolution reconstruction, [column conditioning](./column-conditioning.md) **must** produce typed conditioned raw tiles consumed by luminance/detail estimation, distinct from independently filtered base tiles and full-frame layers. [Pattern conditioning](./pattern-conditioning.md) **must** distinguish unconditioned/conditioned ratio tiles and preserve an explicit edge-scratch seed dependency; intermediate inspection **must** remain selective.
- Supported operations **must** allow alternative algorithm implementations with their own settings and dependencies.
- User-adjustable values **must** be node settings. Graph structure and wiring remain internal to the application.
- Nodes **may** keep intermediate data internal, including color fields created during highlight reconstruction.

## Conformance

The engine constructs and runs both output recipes using shared operations. It rejects an unsupported data-space connection. Selecting a supported algorithm or changing its settings selects the corresponding processing behavior.

## Regional demand

Each converted operation **must** declare the input region needed for its requested output region. Coordinate conversion, filter support, alignment and global measurements belong to the operation. The graph **must** propagate requirements backward and combine all consumers' requirements before execution. The initial representation is Unused, one bounding rectangle, or the whole output domain. Combining rectangles **may** include samples between them.

Demand describes required samples independently of storage. A whole table does not imply a whole photographic image. Operations without a regional contract **must** conservatively request their complete input domain. Shared global analysis **must** retain its established photographic domain independently of viewport position.

The resolver **must** select concrete execution geometry before normal numerical cache planning. A valid retained Whole buffer **may** satisfy regional demand. A compact buffer **must** retain its physical origin and stride at its consumers, and its numerical identity **must** distinguish incompatible coverage. Resolving requirements **must not** load sensor samples, allocate GPU resources or run image processing.

Native sectional results **should** match samples from the same region of a Whole render exactly when arithmetic and sample order are preserved. Any numerical tolerance **must** be justified and measured separately from performance. The [sectional rendering plan](../../plans/sectional-rendering.md) tracks the initial Banding and reconstruction prototype and remaining unconverted operations.

## Implementation latitude

Execution may begin with straightforward dependency-order scheduling. Add caching and scheduling machinery when concrete responsiveness needs justify it.

### Resumable execution

A job steps through its graph. Traversal owns its state between steps while borrowing the prepared graph, immutable input context, working photo and profile for each step. Every render uses this one execution path, whether it runs alone or with other jobs in a tick. Execution must preserve output samples and geometry, dependency selection, retained-input validity, cancellation checks, progress, scratch retirement and diagnostics at every width.

An execution error is terminal. A successful job stores its results in its working photo when it finishes. Pausing between steps must not store an incomplete recipe. Diagnostic context and priority checks must remain scoped to the step performing the work.

### Working photo guards

A job must hold a guard on its working photo until its submitted work has retired and its results are stored. Several photographs may be guarded at once. Only one job may use either resolution of the same photo at a time. The working set must not trim or drop a guarded entry, and shared preparation trimming must keep a guarded entry's metadata and source identity. A guard is not a memory reservation or a GPU wait.

Removal, replacement and explicit clearing must wait for or reject guarded entries before changing them. The job must retire submitted work before releasing its guard.

### Scheduling

The [Scheduler](./render-scheduling.md) owns admission, priority, ticks and the working set. Fresh and retained inputs, exports and the CLI use the same execution path.
