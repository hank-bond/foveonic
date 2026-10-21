---
summary: "Family-authored editing panels share widgets and use processing-owned capabilities and defaults."
kind: reference
status: accepted
implementation: conforming
---

# Family editing controls

## Requirements

The GUI must select an editing panel for the loaded processing family. Each family may choose its controls, grouping and labels. Shared widgets and controls should be reused where their meanings agree. Within a family, camera metadata may constrain available choices or enable individual controls. Unsupported families do not need placeholder panels.

The processing crate must own supported control constraints and effective defaults. UI compatibility normalization and processing request validation must use that contract. Processing must retain no dependency on GUI widgets. Explicit invalid processing requests must fail rather than silently change user intent. The GUI may normalize incompatible saved settings when camera metadata arrives, preserving compatible edits and supported on/off choices.

Classic must show its Color/Monochrome selection, white balance, color mode, tone, reconstruction toggle, sharpening, highlight toggle, despeckle and camera-record repair controls. In monochrome, disable WB and chromatic highlight correction while preserving their color-mode preferences. Keep color-mode selection available and omit saturation. Omit Merrill monochrome mixing/noise controls, color noise/detail sliders, noise overrides, highlight method/threshold selection and intrinsic row correction. Merrill must preserve its existing color and monochrome editing behavior. Camera-provided preset order must remain intact.

Panel rendering must preserve settings unless the user edits a control. Use lightweight capture properties to select the family panel before rendering completes. Until the family is identified, show shared controls only and retain saved settings without applying another family's defaults. Full processing metadata supplies camera preset catalogues when available. Family panels must remain ordinary GUI composition functions. A generic widget schema or plugin framework is not required.

## Conformance

Check Classic and Merrill panels through real egui layout and interaction tests, including switching camera metadata, normalizing incompatible saved settings, reconstruction on/off and effective sharpening defaults. Check the processing contract independently of egui. Preserve existing Merrill interaction tests and run workspace checks.
