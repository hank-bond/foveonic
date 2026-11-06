---
summary: "Interactive rendering reuses immutable preparation and compact exact step identities without retaining unnecessary native data."
kind: reference
status: accepted
implementation: conforming
---

# Render preparation

## Reuse and invalidation

Interactive edits should reuse unchanged metadata-derived preparation, immutable lookup curves and operation identities. A downstream edit should not regenerate an unchanged upstream parameter array or copy its full payload into another cache key. Keep shared preparation useful across existing camera recipes and post-reconstruction RGB operations.

A step identity must include its operation, effective parameters, working grid and upstream identities as required by its numerical behavior. Source or control changes must invalidate the affected preparation and outputs. Do not hash photographic samples to identify source images. Source identity and immutable preparation ownership must remain explicit.

Compact fingerprints may reject unequal keys quickly. Exact equality must remain available for matching fingerprints or through shared immutable identity, so a collision cannot return another operation's cached pixels. Reuse fingerprints for immutable payloads rather than rescanning unchanged tables and clipping evidence on every edit. Avoid raw pointer addresses whose allocation lifetime is not retained by the identity.

## Ownership

Preparation caches must have bounded ownership. Entries must not keep complete decoded originals alive merely to reuse their metadata or curves. Native-only prepared arrays must be releasable when an inactive photo is sealed into a reduced working set. Report retained preparation bytes separately from GPU image checkpoints and avoid counting shared payload storage more than once within the reported owner.

Keep existing camera recipes, correction order, authoritative Full rendering and source-derived controls intact. Reuse should not require a second reduced-quality recipe or change stored settings. Working-set geometry and upstream control validity follow [small previews](./small-previews.md).

## Validation

Measure warm exposure feedback separately from preparation-changing edits and first renders. Validate cached results against fresh output, including source switching, WB/highlight/noise controls and shared curve dependencies. Existing input-damage errors, cancellation and guarded immutable GPU resources remain unchanged.

Profiling follows [profiling requirements](./profiling.md). Off avoids optional recording. Wall instrumentation must not add GPU completion waits. The coordinated implementation is tracked in [scaled editing optimization](../../plans/archived/scaled-editing-optimization.md).
