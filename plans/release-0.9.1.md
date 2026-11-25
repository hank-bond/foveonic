---
summary: "Remove the last-selected photograph from editing and display, and release 0.9.1."
---

# Release 0.9.1

The owner found after publishing 0.9.0 that a group does not behave as one: the most recently selected photograph is treated differently. The owner considers every such difference a bug and wants them fixed before advertising the release (2026-11-25). Requirements: [multi-photo editing](../docs/specs/multi-edit.md).

## Reported

- [ ] **One photograph follows a slider more closely than the others.** In a group of six, the last-selected photograph redraws promptly during a drag and the rest trail. Cause unknown. The latency jobs time when a group has settled and never when each photograph is drawn, so they cannot show it. Reproduce with a behavioral check that records each photograph's drawn edit during a drag, on 0.9.0 and 0.8.4, before changing anything. Ruled out: the viewport rebuilds every tile's binding on each pass, so the active-only `viewport::clear` does not hold other tiles back.
- [ ] **Crop and Rotate show only on the last-selected photograph.** Show the armed tool on every displayed photograph and edit the one that is dragged. The sidebar's crop ratio sets every editing target; a drag is locked to the dragged photograph's own ratio (owner, 2026-11-25). The crop control is an icon with no displayed value, so with differing ratios its list marks none. Other dropdowns keep “Multi”.
- [ ] **Other reads of the last-selected photograph.** `App::selected` appears in 51 places: the planner's `Round::selected` and `PhotoLooks`, which give that photograph its own treatment; delivery, which tells the worker a whole image arrived only for that photograph; the navigator, scopes, the Editing menu's family and preset list and displayed clipping values; and bookkeeping such as prefetch order and logs. Remove each visible difference. Carried from the [coherence cleanup](./coherence-cleanup.md).

## Release

- [ ] Correct the 0.9.0 notes, whose opening says every editing tool works across the group.
- [ ] Repeat the release checks on the 0.9.1 archives, including both Windows hosts.

## Later

- Owner idea, 2026-11-25: a per-photograph information area while editing several photographs, so each one's own values are visible. Broader than this release.
