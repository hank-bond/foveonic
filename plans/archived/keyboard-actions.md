---
summary: "Centralize workspace shortcuts and apply copied editing recipes to highlighted photographs."
status: archived
---

# Shared keyboard actions and recipe transfer

Implemented the approved shared registry and highlighted-photo paste behavior in [GUI workspace](../../docs/specs/gui-workspace.md) and [editing sidecars](../../docs/specs/editing-sidecars.md).

- [x] Centralize workspace bindings, availability and text/menu/modal exclusions.
- [x] Route browser actions through their existing owner and copy/paste through the editing session.
- [x] Reuse versioned clipboard recipes and asynchronous protected sidecar writes, preserving Import review edits.
- [x] Validate native clipboard events, focus exclusions, multiple destinations, frozen snapshots and protected recipes.
- [x] Run workspace checks and build the review app.

The current egui-winit adapter replaces modified C/V presses with Copy/Paste events, including Shift. The registry accepts those events using the platform modifier state and retains paste payloads. Copy uses egui's system-clipboard output with the same recipe schema as sidecars. No second clipboard-settings format or generic task executor is introduced.

Browser actions are queued before widgets consume input. Recipe transfer runs after browser highlighting so Select All and Paste within one frame use the new highlighted set. Unviewed destinations use the existing coalescing sidecar writer without source decoding or GPU work. Existing family normalization remains authoritative when a photograph becomes active.

Validation covers the existing browser focus/navigation/review suite through the shared registry, conflicting bindings, platform Command/Ctrl modifiers, native Copy/Paste payloads, text/menu/modal exclusions, recipe validation, transient preview exclusion, multi-photo persistence, independent frozen copies, Import retention and malformed-sidecar protection. All 584 ordinary workspace tests pass. Strict all-target Clippy, formatting, indexes and artwork checks pass, and the macOS review bundle builds. Native OS clipboard interaction remains an owner review item.
