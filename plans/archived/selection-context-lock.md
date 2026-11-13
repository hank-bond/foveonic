---
summary: "Prevent coalesced Pointer clicks from reacquiring the egui context lock during selection bookkeeping."
status: archived
---

# Selection context lock

Pointer selection could deadlock when primary press and release arrived in the same frame. The handler held `Context::data_mut` while calling `Response::clicked`. A clicked response reads input through the same context, attempting to reacquire its lock. A live process sample showed the UI thread blocked in this handler and the render worker subsequently blocked requesting repaint. This was a UI lock failure, not a slow GPU render.

The handler now snapshots the click and press eligibility before taking the data lock. The closure only reads and updates its stored gesture ownership. Tool eligibility, release cleanup, modifiers and Space behavior remain unchanged.

A bounded real-egui regression establishes the widget, then delivers press and release together in two frames. It verifies clicked responses, both input edges, released pointer state and replacement-selection gestures. The pre-fix code times out after three seconds. The corrected code passes. Timeout handling does not join a deadlocked test thread.

All 520 ordinary GUI tests pass. Strict release GUI/all-target Clippy, formatting and documentation checks pass. The native `transition-journey` scenario passes with two declared photographs, seed 23, four selection/edit pairs and fresh image comparisons. Its capture is complete, loss-free and uncontended. Independent largest-model review approved the fix and regression. Rendering algorithms and scheduling are unchanged.
