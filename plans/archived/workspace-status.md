---
summary: "Separate browser-footer progress, concise live activity and acknowledged session history."
status: archived
---

# Simplify workspace status

Requirements: [workspace status](../../docs/specs/workspace-status.md).

- [x] Add shared footer and error-button styling, a bounded session log and error acknowledgment.
- [x] Connect file-operation and error events without logging render churn or repeated polled errors.
- [x] Move progress below the browser and simplify the top-row live readout.
- [x] Verify state transitions, layout and native interaction, then rebuild and reopen for review.

The shared footer has one height and separator for browser progress and photograph metadata. The Log and missing-destination buttons share one error style. A bounded session history owns unread-error acknowledgment, while persistent display, source-scan and edit-save failures are recorded only on arrival or recurrence. Batch receipts preserve actual output paths and completion summaries. Render completions, canceled requests, ordinary browsing and automatic successful edit saves do not populate the history.

Workspace tests and strict Clippy pass. An actual two-photo Browse batch verifies frozen edits, output files and the logged output paths. Native macOS captures verify the aligned browser footer, error-button styling, selectable log details, acknowledgment on opening, retained history after closing and renewed red styling for another error. Formatting, documentation indexes and artwork checks pass. The optimized review app is reopened on the combined example folder.
