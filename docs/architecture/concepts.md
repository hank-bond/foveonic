---
summary: "Shared vocabulary distinguishes requested edits, render coordination, processing reuse and display without prescribing Rust types prematurely."
kind: reference
---

# Concepts and names

Use these meanings in architecture discussion. Existing code names below are navigation aids, not endorsement of their current boundaries. The owner approved request, job, result and handler as ordinary reusable vocabulary, with render identifying this domain. Other proposed names remain provisional until reviewed. This document is not a catalog of types that must be implemented.

## Reviewed work vocabulary

**Render request → render job → render result → result handler.** Use the domain qualifier where context needs it. Within an unambiguous render context, request, job, result and handler are sufficient.

| Term | Meaning |
| --- | --- |
| Request | Describes the work being requested. |
| Job | Scheduled work that executes a request. Normal completion is expected. |
| Result | The outcome returned by a job. A successful render supplies an image. Failure and cancellation need explicit meanings rather than an unexplained absence of an image. |
| Handler | The function or component that consumes a result and decides what the application does with it. Result handling names that responsibility. |

Requests and jobs are distinct because requests can share useful execution and can change while a job is running. This vocabulary does not prescribe one new job per UI event or one job per request. The concrete mapping belongs to the reviewed scheduling contract.

Image is the ordinary term for produced pixels and their processing context. Rendered image may qualify that output in prose without creating another architectural entity. Original file and decoded sensor data keep their existing meanings. In the GUI, a `RenderRequest` is what the app asks for and carries a `RequestId`, issued in order. A `RenderResult` is a delivered image ready to draw, linked to the `RenderRequest` it answers. A `TaskId` names the coordinator's work, which several requests can share.

The first implementation maps result handling to `RenderCoordinator::handle_result`. It consumes the current worker `Reply` and returns a small `ResultUpdate` describing permitted UI effects. Apply the shared vocabulary in discussion and review any representation rename with the unit that changes its contract.

The render runtime now runs the coordinator on its own thread. **View context** is an immutable description of what the viewport can show. The UI publishes it when its values change. **View updates** carry result-handler decisions and image data back to the UI, which checks that they still fit the current interaction before applying them. Status is an immutable summary of processed commands and request settlement.

## Scheduling vocabulary

The owner agreed these names on 2026-11-17 and 2026-11-18. They replace the separate serial and wide execution vocabulary. Requirements live in [render scheduling](../specs/render-scheduling.md).

| Term | Meaning | Replaces |
| --- | --- | --- |
| Scheduler | The core owner of the queue and the working set. It admits jobs against the memory target and advances them together. | Render worker loop, `Wide`, GUI admission estimates |
| Queue | The Scheduler's prioritized collection of submitted jobs. A newer job for the same target replaces the queued one. | `WorkPlan` service order, `Cursor` |
| Job | One submitted render: a target, settings, priority and requested output. It ends finished, failed or cancelled. | `Task` |
| Target | The photo and requested view that a job serves. Replacement and cancellation use it. | Unchanged |
| Priority | The single ordered set of tiers: Viewport, Auxiliary, Thumbnail, Export, Speculative. It governs the queue and the shared cross-thread gate. | `TaskPriority`, core gate `Priority` |
| Auxiliary | The tier for navigator images and scopes. The name may be revisited. | Navigator/scope priority |
| Speculative | Run-ahead work nobody has asked for yet: neighbor decoding, reduced preparation and full-image completion after a zoomed region. | Prefetch, idle, decode and background tiers |
| Working set | Everything the Scheduler keeps in memory for rendering: working photos and cached images. Trimming returns it to the user's memory target. | `ProcessingWorkingSet`, GUI frontend memory inventory |
| Working photo | One photo's working memory at one resolution: its sensor data and checkpoints. A dropped working photo is rebuilt from the file. The name follows the operating-system working set. | Session, `RenderSession`, `ReducedSession` |
| Native, Reduced | The two working-photo resolutions. Native keeps full-resolution sensor data. Reduced keeps sensor data downsampled to its working scale. | `SessionKind` |
| Working scale | How many times smaller than full resolution a render processes. It applies to nearly the whole graph, not only the result. | `working_scale`, `WorkingGrid` |
| File | The raw file on disk. Avoid "original". | Original file |
| Sensor data | The file's decoded data, which a working photo keeps until it is evicted. Native keeps it at full resolution. Reduced keeps it corrected and downsampled, stored with its checkpoints but never trimmed. | `Source`, decoded original, required or sealed cuts |
| Checkpoint | One node's stored output in a working photo, kept so later renders resume from it. Checkpoints are trimmable. | Stored intermediate results, graph cache entries |
| Restart input | A checkpoint an edit resumes from, ordered by when that edit was last used. | Restart frontier |
| Selected, Visible, Active | Photos in the browser selection; photos in a viewport tile; visible photos receiving edits. The working photos that viewports render from are visible and never evicted. | Shown photo |
| Purpose | What a job is for: Edit, View, Auxiliary, Thumbnail, Export or Speculative. It sets the job's priority and how its use counts toward keeping memory. | `PhotoUse`, priority-derived foreground use |
| Use | One job using one working photo, recorded with the job's purpose. The working set decides what each use refreshes. | Record use, touch |
| Image | What a job delivers: normally the final result, or the requested node for diagnostics. A checkpoint is never called an image. | Frame, rendered image |
| Display texture | The GUI's drawable copy of an image. It is a separate GPU allocation from the image's buffer. | `ImageTexture` |
| Visible image | An image the GUI draws. It is guarded and counted against the target. | Presented frame |
| Cached image | An image the GUI may show again but does not draw now. The scheduler drops it first under pressure, and the GUI resubmits its job when it is gone. | Kept result, `KeptResults` |
| Quality, Speed | The processing choice applied to either resolution. Speed reduces some intermediate resolutions and other costs. | Unchanged |
| Guard | A handle that keeps a working-set entry from being dropped while held. Jobs guard their working photo. The GUI guards visible images and holds weak handles to cached images. | Execution claim, active photo, parking |
| Thrashing | Repeatedly dropping and rebuilding working photos because more work is admitted than fits. Admission avoids it. | Cache eviction churn |
| Traversal | Comparing a graph with the working photo's stored results to find which nodes still need to run. | `GraphExecution` |
| Step | A job running its next uncached node. One node may use several GPU compute passes. | Graph advancement |
| Tick | Every admitted job takes one step, then the Scheduler waits for the GPU once. A single job is a tick of one. | Submission window, operation completion |
| Width | How many jobs a tick contains. It follows from admission and is never a user setting. | Render width |

Avoid "pass" for a node or tick because it already means a GPU compute pass in the engine. Avoid "active" for working photos. Recency, guards and one trimming order replace it.

## Viewport and image coordinates

The owner agreed these distinct meanings:

| Term | Meaning |
| --- | --- |
| Viewport | The screen area where a photograph is displayed, expressed in UI coordinates. |
| Visible region | The portion of the photograph currently shown through that viewport, expressed in native-resolution image coordinates. |
| Requested render region | The image area requested for processing. It may extend beyond the visible region for overscan or processing support. |

Zoom and pan define the transform between a viewport and its visible region. A single-photo view has one photograph viewport. A multi-photo comparison has one per photograph within the comparison area. A loupe follows the same distinction between its display area and the image region it shows.

Layout supplies viewport geometry. A photo-keyed UI owner retains each photograph’s zoom and pan across single and multi-photo layouts. UI interaction changes that state, from which the visible region is derived. The coordinator requests useful rendering, while the engine owns numerical alignment and required neighboring samples. These distinctions do not prescribe additional managers or one Rust type per term. Saved Crop remains a separate photographic composition setting.

## Established meanings

| Concept | Meaning | Current source |
| --- | --- | --- |
| Requested edits | The photographic settings the user has asked for, independent of whichever render last completed. | `App::settings`, multi-edit `Runtime`, sidecar `Session` currently synchronize these values. |
| Browser selection | Photographs included in the viewport and targeted by browser/file actions. It is not bounded by the editing limit. | Browser and viewport selection integration. |
| Editing targets | The viewport-selected photographs receiving editor changes. One target is individual editing. | Viewport selection and the photo runtime’s `editing_paths`. |
| Viewport photos | Photograph tiles the current layout draws. Every viewport photo has its settings loaded. The overflow indicator is not a photograph viewport. | UI layout’s `viewport_selection`, read through `App::viewport_photos`. |
| Planned photos | The viewport photos plus editing targets the layout has no room to draw. The view planner plans their images. | `App::planned_photos`, published to the coordinator as `Photos::planned`. |
| Processing engine | Recipes, graphs, numerical dependency reuse and execution. It does not decide browser selection or which UI view accepts a result. | Core render API, graph executor and sessions. |
| Cache | Rebuildable retained data with a specific representation, validity rule and owner. | Decoded disk bundles, processing inputs/checkpoints and presentation resources have separate owners. |
| Foreground work | Work needed for a current user interaction. | Current coordinator, service cursor and shared priority gate. The [Scheduler](#scheduling-vocabulary) will own it. |
| Speculative work | Work prepared ahead of a request in anticipation of use. It must not count as actual use for retention. | Decode prefetch and idle processing preparation. It becomes the Speculative priority tier. |

## Provisional names for review

| Name | Intended meaning | Does not imply |
| --- | --- | --- |
| Render runtime | The application-level owner of wanted render results, work service and completion decisions. | A second numerical graph, a generic actor framework or ownership of every cache. |
| Requested set | The current collection of wanted per-photo results. A single photo uses the same collection contract with one member. | Identical recipes or one shared mutable settings object. |

Do not equate a job finishing, its request settling, its pixels being retained, its image being admitted to a view and the UI applying that image. Names for the smallest observable operations should preserve their different effects. The [first migration unit](../../plans/render-runtime.md#result-handler-review-unit) defines the result handler's ownership and behavior before introducing replacement runtime types.

Running several photographs together is now part of the [Scheduler](#scheduling-vocabulary) rather than a separate execution path. It does not imply one shader dispatch across images. The [earlier investigation](../../plans/archived/wide-rendering.md) keeps its historical evidence.

## Refinement requests

A refinement request freezes a first render request and an optional whole-image continuation. The UI supplies visible image coordinates and reserves ordered revisions for both. The coordinator owns completion, replacement and when continuation becomes runnable. A request without continuation settles after its first result. Existing Smart interactions request whole-image continuation. Source loading and numerical processing keep their existing owners.

## Naming practice

Prefer established software terms with familiar meanings. Reuse request, job, result and handler in other domains when those meanings fit, qualifying the domain where context needs it. Define a new concept only when existing vocabulary cannot explain a behavior callers need to distinguish. If two different things need the same word, refine the distinction before naming their types. Avoid converting every lifecycle step into a new noun or exposing implementation details in a domain name. Introduce concrete API names with their first callers and migrate those callers together.

The vocabulary approach is inspired by UIX's [concepts](https://github.com/hank-bond/uix/blob/main/docs/architecture/concepts.md) and [naming guidance](https://github.com/hank-bond/uix/blob/main/docs/architecture/conventions/naming.md). Its framework-specific terminology does not become part of this application.
