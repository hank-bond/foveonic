---
summary: "Independent browser and viewport selections support bounded relative multi-photo editing with viewport-only tools."
kind: reference
status: accepted
implementation: incomplete
---

# Multi-photo editing

## Selection and tools

Browser selection determines which photographs appear in the viewport. It has no editing-count limit and remains the scope for processing, review/file operations and copying or pasting settings. Large selections use progressively smaller viewport tiles down to a minimum readable size, followed by an “N more” overflow indicator. Selection alone must not retain a complete editable processing session for every photograph.

Viewport selection determines which photographs receive editor changes. Selecting one photograph edits it individually. Selecting several enables multi-edit directly, without a separate editing mode or activation button. Browser selection initializes viewport selection to its first L photographs in browser order, where L is the configurable multi-edit limit. The default limit is eight. Additional browser-selected photographs remain outside the editing subset. Normal clicks replace viewport selection. Option-click toggles membership and Shift-click selects a range in viewport order, respecting the limit. Editing targets must remain explicit and distinct from the active photograph that supplies metadata and tool context.

The photograph toolbar must include a Pointer tool with shortcut V. Pointer is the default tool and the only tool that changes viewport selection. Hand, Crop, Rotate and picking tools must preserve viewport selection when used on the photograph. Selecting another tool therefore holds the current selection without a separate lock mode. Hand uses H and pans the image. Space temporarily pans without changing the selected tool. Finishing or leaving an editing tool returns to Pointer. Browser and editor widgets always use ordinary pointer interaction independently of the selected viewport tool. Rotation and other photograph tools must not intercept browser thumbnail clicks or replace their pointer cursor.

The first tool-routing slice retains individual tool operations on the active viewport photograph. Applying one tool gesture to several viewport-selected photographs is deferred. Keep selection ownership separate from gesture routing so that this can be added without a second selection model or explicit multi-edit mode.

## Editing behavior

Multi-photo overview rendering uses Thumb independently of how many editing targets are selected. Selecting one photograph inside an overview must not restore the single-photo quality or rerender unchanged pixels. The editing limit bounds the concurrently editable set, independently of the potentially much larger browser selection. Individual photograph edits and preview state must survive switching between photographs, including Import review. Keep a bounded actual-use history of completed results so unchanged target switches and re-adds can reuse them. A completed older or baseline preview must not replace a more recent edited appearance.

Numeric sliders show the minimum and maximum values of the viewport-selected photographs as a range. A drag applies a relative adjustment to each photograph. Each value independently clamps at its allowed limits, compressing the displayed range as other photographs approach a saturated value. Double-click resets the affected setting for all editing targets. Equal values display the ordinary single-value slider. Numeric field entry assigns one explicit value to all targets.

Dropdowns display “Multi” when their selected values differ. Choosing an option assigns it to every editing target. A mixed label is a display state and must not become a saved photographic value. Shared controls must preserve camera capabilities and valid per-photo settings. The implementation must not silently apply an active photograph’s unrelated settings to other targets.

Import keeps edits in memory under its existing write policy. Browse persists each photograph’s requested settings through the existing asynchronous sidecar owner. Copy/paste remains scoped to browser selection.

## Resource direction

Disk caching and in-memory caching are separate user concepts and settings. The memory budget must be user-set with a sensible default designed for a 16 GB machine. Approximately 2 GB steady application memory and 4 GB peak application memory are the initial targets, to be validated by measurement rather than treated as guaranteed allocator limits.

Profile every retained memory owner, including active and multi-edit photographs, processing checkpoints and parameter tables, delivered images, browser thumbnails/previews, shared preparation, speculative preloads and temporary rendering resources. Count shared allocations once. Actual-use recency must remain distinct from speculative loading. Prefetch alone must not promote an entry above genuinely used photographs.

The multi-edit count limit is a separate user setting within the memory design. Determine the interaction behavior first. Final retained-memory partitioning, admission and eviction policy remain separate work and must not be inferred solely from the Thumb pixel count.

## CPU execution

Parallel CPU processing must use the host's available logical CPUs through a shared processing pool. CPU capacity and the number of simultaneously retained source buffers are separate limits. Foreground and speculative work retain their priority and cancellation rules. GPU execution and disk I/O keep separate owners. A dependency-bound stream or serial recipe walk need not invent parallel work solely to occupy every core.
