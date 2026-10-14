---
summary: "Control help explains photographic effects in plain English, with reviewed A/B crops where they teach the adjustment clearly."
kind: reference
status: accepted
implementation: incomplete
---

# Photographic control help

## Requirements

- Editing controls, advanced controls and their link buttons, inspection choices, and scope choices **must** have explanations written for people familiar with photo editing, without assuming programming or Foveon sensor knowledge.
- Noise preset and Detail zero/default explanations **must** live in hover popovers, not as a persistent note below the controls. Use simple photographic language; professional photography knowledge **may** be assumed, but Merrill architecture expertise **must not** be assumed.
- Help **should** explain what changes, when it is useful, and the important tradeoff. It **must** describe the implemented behavior, including neutral/off settings and relevant dependencies. Technical names alone are not explanations.
- Highlights under Corrections **must** have help explaining Auto, the relative threshold offset, its neutralization/color tradeoff, and the distinction between manual 0 and the enable checkbox. It **must** remain distinct from tone Highlights and avoid promising recovery of lost capture detail.
- Sensor gain help **must** explain its ISO-like early signal adjustment, its difference from Exposure, and its noise/highlight tradeoffs. It **must not** imply that it changes capture ISO, gathers more light, or recovers detail lost at capture.
- Advanced help **must** explain the visible effect and how to switch between following a basic control and a separate manual setting. Camera-dependent noise presets **must not** be presented as uniformly increasing strength or as off at zero.
- Example comparisons **should** use obvious, intuitive crops from real photographs. Each pair **must** show the same crop at the same scale, name the settings, and hold unrelated settings fixed. Companion settings needed for the effect **must** be stated.
- Minimum/maximum comparisons **should** be used when informative; representative values or off/on comparisons **may** replace uninformative extremes. A black image at zero Sensor gain is not a useful explanation of the tradeoff.
- Example crops **must** be reviewed before becoming application help assets. If existing sources do not demonstrate an effect clearly, request a suitable photograph from the owner rather than exaggerating or claiming a subtle difference is obvious.

## Implementation direction

Start with complete text coverage in the existing hover popovers. Prepare a small set of A/B candidates for owner review before expanding or embedding them. See the [implementation plan](../../plans/photographic-help.md) for coverage, checks, and source-image findings. Processing behavior remains unchanged.
