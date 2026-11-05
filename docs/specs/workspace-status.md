---
summary: "Workspace activity separates live progress from a session log of file operations and errors."
kind: reference
status: accepted
implementation: conforming
---

# Workspace status

Progress and request latency must occupy a footer below the browser with the same height and separator as the viewport metadata. The top row to the left of the photograph toolbar must retain compact live activity and access to the Log modal. Displayed buffers, rendering quality and repeated timing belong to their existing readouts rather than status text.

The Log modal must retain file-operation results and errors for the current session across source changes and later successful operations. Ordinary photograph navigation, slider renders and superseded requests must not produce history entries. Batch processing must record its destination, start and completion counts, saved output paths and completed imports. Failures must retain their full reason and affected source when available. Automatic edit-save failures must remain retryable.

The Log button must use the same red text, border and tinted background as an unavailable destination while errors remain unread. Opening the modal acknowledges those errors without removing them. A persistent failure must not turn the button red again on each repaint. A new error or a resolved failure that recurs must mark it unread again.

History is bounded in memory and does not become an on-disk audit system. Log text must be selectable. Active work and background batch counts remain visible without opening the modal.
