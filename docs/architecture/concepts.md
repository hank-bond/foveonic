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

Image is the ordinary term for produced pixels and their processing context. Rendered image may qualify that output in prose without creating another architectural entity. Original file and decoded sensor data keep their existing meanings. The current worker's `Result<Option<Frame>>` is an implementation to interpret during migration, not the definition of a result.

The first implementation maps result handling to `RenderCoordinator::handle_result`. It consumes the current worker `Reply` and returns a small `ResultUpdate` describing permitted UI effects. Existing `Request`, `Task`, `Reply` and `Frame` representations keep their names while their boundaries are unchanged. Apply the shared vocabulary in discussion and review any representation rename with the unit that changes its contract.

The render runtime now runs the coordinator on its own thread. **View context** is an immutable description of what the viewport can show. The UI publishes it when its values change. **View updates** carry result-handler decisions and image data back to the UI, which checks that they still fit the current interaction before applying them. Status is an immutable summary of processed commands and request settlement.

## Viewport and image coordinates

The owner agreed these distinct meanings:

| Term | Meaning |
| --- | --- |
| Viewport | The screen area where a photograph is displayed, expressed in UI coordinates. |
| Visible region | The portion of the photograph currently shown through that viewport, expressed in native-resolution image coordinates. |
| Requested render region | The image area requested for processing. It may extend beyond the visible region for overscan or processing support. |

Zoom and pan define the transform between a viewport and its visible region. A single-photo view has one photograph viewport. A multi-photo comparison has one per photograph within the comparison area. A loupe follows the same distinction between its display area and the image region it shows.

Layout supplies viewport geometry. UI interaction owns zoom and pan, from which the visible region is derived. The coordinator requests useful rendering, while the engine owns numerical alignment and required neighboring samples. These distinctions do not prescribe additional managers or one Rust type per term. Saved Crop remains a separate photographic composition setting.

## Established meanings

| Concept | Meaning | Current source |
| --- | --- | --- |
| Requested edits | The photographic settings the user has asked for, independent of whichever render last completed. | `App::settings`, multi-edit `Runtime`, sidecar `Session` currently synchronize these values. |
| Browser selection | Photographs included in the viewport and targeted by browser/file actions. It is not bounded by the editing limit. | Browser and viewport selection integration. |
| Editing targets | The viewport-selected photographs receiving editor changes. One target is individual editing. | Viewport selection and multi-edit runtime. |
| Processing engine | Recipes, graphs, numerical dependency reuse and execution. It does not decide browser selection or which UI view accepts a result. | Core render API, graph executor and sessions. |
| Cache | Rebuildable retained data with a specific representation, validity rule and owner. | Decoded disk bundles, processing inputs/checkpoints and presentation resources have separate owners. |
| Foreground work | Work needed for a current user interaction. | Current coordinator, service cursor and shared priority gate. |
| Speculative work | Work prepared ahead of a request in anticipation of use. It must not count as actual use for retention. | Decode prefetch and idle processing preparation. |

## Provisional names for review

| Name | Intended meaning | Does not imply |
| --- | --- | --- |
| Render runtime | The application-level owner of wanted render results, work service and completion decisions. | A second numerical graph, a generic actor framework or ownership of every cache. |
| Requested set | The current collection of wanted per-photo results. A single photo uses the same collection contract with one member. | Identical recipes or one shared mutable settings object. |

Do not equate a job finishing, its request settling, its pixels being retained, its image being admitted to a view and the UI applying that image. Names for the smallest observable operations should preserve their different effects. The [first migration unit](../../plans/render-runtime.md#result-handler-review-unit) defines the result handler's ownership and behavior before introducing replacement runtime types.

Wide execution is a [separate future engine investigation](../../plans/wide-rendering.md): advancing ready graph work across photographs rather than finishing each graph serially. It does not imply one shader dispatch or a barrier across all images. It is outside this ownership migration.

## Naming practice

Prefer established software terms with familiar meanings. Reuse request, job, result and handler in other domains when those meanings fit, qualifying the domain where context needs it. Define a new concept only when existing vocabulary cannot explain a behavior callers need to distinguish. If two different things need the same word, refine the distinction before naming their types. Avoid converting every lifecycle step into a new noun or exposing implementation details in a domain name. Introduce concrete API names with their first callers and migrate those callers together.

The vocabulary approach is inspired by UIX's [concepts](https://github.com/hank-bond/uix/blob/main/docs/architecture/concepts.md) and [naming guidance](https://github.com/hank-bond/uix/blob/main/docs/architecture/conventions/naming.md). Its framework-specific terminology does not become part of this application.
