---
summary: "Record native Merrill banding presets, reached pixel consumers, and the remaining application-export checks."
---

# Reference banding verification

The owner approved native verification before selecting a strength control, then approved a five-position snapping slider based on the results below. Implementation is tracked in the [slider plan](./banding-slider-and-settings-defaults.md). Matching application exports remain unverified.

## Completed checks

- [x] Compare complete guarded native parameter records across camera metadata, all five presets, HI/MED/LOW, color/secondary monochrome, and selection/ISO boundaries.
- [x] Replay identical Stage 2 inputs at all five positions through full/reduced color and selected monochrome consumers; retain final RGB/detail samples and consumer input/output hashes.
- [x] Explain the difference-view geometry from the current crop, orientation, and presentation code.
- [x] Record the corrected window mapping and the limits of the evidence before proposing the shared GUI/CLI control.
- [x] Retire the temporary investigation harnesses while preserving fixtures, regression tests, and documented findings.

## Native results

The installed library SHA-256 is `037419dc04bb1afe86dddad7a14888a1b36d6c00a0231ec50a6974a9963ce5e6`, matching the retained independent audit.

The setup sweep passes **58,374 guarded records in 11,550 five-position groups**, including selection-boundary and finite-clipping probes. It covers one original CAMF per DP1/DP2/DP3/SD1, all 125 chroma/luminance/banding combinations, HI/MED/LOW, color and secondary monochrome, and Float32 ISO-knot neighbors through the safe table range. Changing banding alters only the column coefficient, window, and preset-index words (`0xac`, `0xb0`, `0xb4`). Known undefined upper-table reads were excluded.

The pixel sweep passes **65 native Stage 2 requests on thirteen configurations**. Color covers all four cameras, reconstruction scales `1/.875/.75/.625/.5/.375`, ISO 100 through approximately ISO 6400, and HI/MED/LOW. Monochrome covers DP3 ISO 100/1600 and SD1 ISO 100. Every request reaches all sixteen color tiles or eight monochrome strips. Owned buffer guards and original inputs remain unchanged. Every nonmidpoint position changes final RGB in these examples; that is not proof of photographic improvement.

### Reached preset mapping

| Position | Horizontal window | HI coefficient below ISO 800 | MED/LOW coefficient below ISO 800 |
| --- | --- | --- | --- |
| 0 | 15 | .625 | .875 |
| 1 | 21 | .5 | .75 |
| 2 | 27 | .375 | .625 |
| 3 | 33 | .25 | .5 |
| 4 | 39 | .125 | .375 |

Native ISO scaling applies at ISO 800. Both color and monochrome consume the selected window. Larger positions broaden the window and reduce the range-distance coefficient, allowing stronger smoothing when estimating offsets. The coefficient is not a multiplier on the final subtraction. Color retains a fixed 6% eligibility gate; monochrome also uses the selected preset in its correction limit.

The earlier fixed-window assumption was wrong. The existing reference Rust color caller preserves window 27, but the installed native consumer uses nondefault windows too. All five requests per configuration have identical Stage 2 inputs, and color raw tiles are identical before column correction. Row preprocessing is a separate earlier stage.

## Remaining boundary

- [ ] Close the application button-to-rational mapping and direction audit.
- [ ] Compare original row preprocessing across application requests and reconcile engine replay with matching application exports.
- [ ] Extend monochrome photographic coverage beyond the recorded DP3/SD1 cases if that path becomes implementation scope.

These measurements use supplied identical inputs. They are native setup/pixel evidence, not new decoding or preprocessing proof, complete application-output verification, or new Rust parity evidence. Complete RGB/detail planes, identities, and failed captures remain local. Independent component fixtures and their Rust regression tests remain in the repository.

## Difference-view geometry

Inspection presents the `ActiveImageArea` crop with camera rotation, not the full sensor. Dark pixels outside the displayed image rectangle are viewport background. Middle gray inside it means unchanged samples. Row correction separately preserves asymmetric bounds and rejects unqualified estimates, so unchanged internal strips do not establish that calibration margins are being displayed. Exact qualifying rows still require that photograph's diagnostics. See [row correction](../docs/specs/row-banding.md) and [inspection views](../docs/render-resources.md).

## Harness corrections

Failed measurements are preserved. Streamed hashing replaced `hashlib.file_digest`, unavailable in the debugger's Python. Persistent return breakpoints keyed by thread replaced overlapping one-shot breakpoints. The first broad pixel sweep stopped on its incorrect fixed-window assertion after observing window 15; the corrected sweep checks actual consumer windows against effective native settings and passes all 65 requests. These changes affected measurement tools, not processing defaults.

The temporary setup sweep, component capture, pixel replay, debugger trace, and C++ worker were retired after this investigation. Git history at `7742f3a` preserves the harness implementations. Their removal does not change the recorded evidence or unresolved application-export boundary.
