---
summary: "Increase manual recording capacity and bound completed capture bundles without pruning automated evidence."
status: archived
---

# Capture storage

The owner requested a 128 MB live Perfetto buffer and retention of the latest four saved runs after an SD-card exposure capture overwrote its early history. Keep live recording in RAM. Completed manual bundles, including derived analysis, are retained per destination parent. Use a stable destination across owner test launches.

Manual Stop/Save and orderly-exit saving use the recorder's existing off-UI control thread. Prune explicitly retention-managed completed bundles from the same producer only after a successful new save. Always preserve the new capture. Explicit scripted/test destinations keep their existing save behavior. Unrecognized and incomplete folders are outside automatic retention.

- [x] Change the shared default buffer to 128 MB.
- [x] Add completed-manual-bundle retention on the existing recorder thread.
- [x] Test retention, ownership filtering and save lifecycle in isolated directories.
- [x] Obtain independent review and run relevant gates.
- [x] Commit, rebuild and prepare the next zoom/loupe capture in the stable folder.

The independent review caught a cross-mode ownership issue: skipping pruning during a scripted save did not protect that bundle from a later manual cleanup. Opted-in saves now carry a retention marker, and cleanup requires that marker. Tests include a complete unmarked scripted bundle from the same producer. The known owner exposure capture is explicitly migrated into the stable managed parent.

The final independent review approved the marker-based ownership boundary and off-UI cleanup. The release workspace suite passed, including 541 core and 519 GUI tests. Strict workspace Clippy passed. An isolated real SDK exit-save test confirmed a 131072 KiB buffer, retention marker and exactly four remaining managed bundles after five older candidates. Pure retention tests preserved unmarked scripted evidence from the same producer, other applications, incomplete saves, missing traces and unrelated files. The existing exposure bundle was explicitly adopted and moved into the stable project capture parent. No photographic processing or cache policy changed.
