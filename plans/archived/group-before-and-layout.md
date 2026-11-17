---
summary: "Apply transient Before to the editing group, simplify tile outlines and keep pane resizing live."
status: archived
---

# Group Before and viewport layout

Implementation and independent checks are complete. The owner received the build and continued to the final wide-rendering cleanup. Its matching final build includes these behaviors for further owner testing.


Requirements: [multi-photo editing](../../docs/specs/multi-edit.md), [editor workflow](../../docs/specs/editor-workflow.md) and [viewport layout](../../docs/specs/viewport-workflows.md#layout-and-interaction).

## Approved behavior

Before/After affects blue viewport editing targets, using each photograph's own Camera or Defaults recipe and authored framing. Visible non-editing photographs remain Current. Authored settings, Current images and native patches remain available for restoration. Technical focus may change within the selected group without leaving Before. Before images must not update Current numeric/default feedback, sidecars or browser photographic previews.

Use one outline: thin white for other tiles, thick blue for editing targets. Remove the separate most-recent-photo white highlight and inset blue outline. Disable navigator/scopes controls and new preparation while editing multiple photographs, and reject late scope display.

Both side panel dividers resize viewport tiles while held. Photo and caption gestures preserve their geometry through release. Resize ownership must use real widget input ownership rather than private panel IDs or edge-coordinate guesses.

## Implementation and evidence

- [x] Reproduce divider freezing with real egui panels and preserve photo/caption ownership through release, including empty viewports.
- [x] Keep alternate whole/detail images in the existing bounded per-photo owner and transfer selected presentation through the existing Before state.
- [x] Publish per-photo rendition intent, retire removed intents and preserve late Current feedback for After.
- [x] Use the shared per-photo path for ordinary Before, calibration changes and native refinement. Remove the redundant selected-photo refinement publisher.
- [x] Validate per-photo baselines/framing, Current restoration, cold selection, native coverage, browser isolation and disabled multi-edit analysis in real native journeys.
- [x] Complete final GUI/lint/document checks after independent review corrections.
- [x] Make the matching build available for owner testing.
- [x] Deliver the reviewed build and continue under the owner’s final cleanup authorization.

Real panel regressions fail under the former global pointer-down/release freeze and pass using photo/caption widget ownership. Separate regressions proved click-only captions and empty viewports lost ownership during cancelled drags. Giving those widgets drag sense preserves release geometry without new gesture state or changing click selection.

Two delayed-current coordinator/delivery regressions reproduced a lost edit when Current completed during Before. Current feedback now retains its authored snapshot for After while displayed tiles independently check their rendition. Two request-policy regressions reproduced blocked cold metadata acquisition and consumed Current demand for skipped warm targets. Eligibility now runs before dirty work is consumed. Metadata-less targets bootstrap through a small ordinary Current image.

The native cold-target journey caught the legacy selected-photo path labeling authored Current exposure as Camera before metadata was available. All ordinary Before now uses the shared per-photo path, including calibration changes. Existing opening timing/foreground handoff moved with that ownership. A later native selection test proved identical Current refinement left an opening guard live after rendering settled. Already-published intent now completes that handoff as newly published intent does.

The older scope assertion expected the whole-frame key while valid scopes completed on a covered native patch. A test-only probe proved matching requested/completed detail keys and patch-local sample bounds. The corrected expectation independently checks compatible source, policy, calibration, native coverage and sample coordinates. Scope production is unchanged.

Independent reviews found no remaining functional issues. Native two-, three- and four-photo journeys pass 66, 74 and 77 transitions respectively. These correctness runs use pinned public Merrill originals, a 6 GiB retained-cache budget and an 8 GiB physical-memory guard. Peak process footprints are approximately 4.40, 5.15 and 6.25 GiB across journeys that include native Quality work. They are not latency measurements. Earlier GPU captures also passed the new group-specific checks. The scenario timeout covers the complete transition journey, so choose its allowance and trial count explicitly.
