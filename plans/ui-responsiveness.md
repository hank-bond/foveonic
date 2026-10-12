---
summary: "Measure full-quality slider feedback with active-edit input reuse, image-first replies, deferred navigator presentation, and safe supersession."
---

# Improve slider responsiveness

Requirements: [interactive rendering](../docs/specs/interactive-rendering.md), [render resources](../docs/specs/render-resources.md), [GUI workspace](../docs/specs/gui-workspace.md).

## Steps

- [x] Implement processing-owned effective keys and a bounded active-edit continuation boundary in the engine; prioritize Detail.
- [x] Check exact cold/cache/fresh float and TIFF outputs on six originals, retained ownership, engine rebinding, selective roots and safe interrupted-graph resume.
- [ ] Integrate latest-request scheduling, progressive same-photo/root feedback and image-first deferred scopes in the GUI.
- [ ] Replace navigator callbacks with deferred GPU-generated small textures and preserve clipping/interaction.
- [ ] Measure native feedback, normal memory and transient peaks; review remaining work.

## Engine checkpoint

The session retains one immutable source and one active input set. Processing modules own exact effective recipe bytes; the executor combines these with upstream generations and output identity. The full continuation boundary includes unchanged branches feeding downstream consumers, not only immediate inputs to the changed operation. Cached values never enter scratch. The safety limit is 1 GiB; around 2 GB total normal editing usage remains the soft target.

Core conformance includes five Detail strengths and ten additional settings changes per original, exact retained performance goldens and cold/warm resized TIFFs. Evidence: `/tmp/foveonic-ui-complete-boundary-gpu.log` and `.local/ui-responsiveness/complete-boundary-equality-1`. Six dispatches remain in cached Detail feedback with full-resolution texture preparation. GUI integration and its measured evidence will be recorded in the next checkpoint.
