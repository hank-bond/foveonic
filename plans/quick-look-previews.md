---
summary: "Ship Quick Look X3F previews in 0.9.2 with one source of a photograph's defaults for the app, the command line and the extensions."
---

# Quick Look previews

Implements [Quick Look previews](../docs/specs/quick-look-previews.md). Released in 0.9.2 together with one source of a photograph's defaults, which the preview needs and which the [coherence cleanup](./coherence-cleanup.md) had planned for 0.10.0 (owner, 2026-11-25). The rest of the related command-line and export cleanup stays in 0.10.0; see [what 0.10.0 takes over](#what-0100-takes-over).

## Probe findings

A throwaway sandboxed preview extension called the engine through a C interface on an Apple M1 Pro with macOS 26. It rendered camera-default Merrill photographs at native size with Speed quality and returned a JPEG reply.

- Metal is available inside the sandboxed extension. Only the app-sandbox entitlement was needed. An ad-hoc signature registered and ran locally.
- Each preview started a new extension process. Engine creation cost 280–310 ms every time.
- Native-size totals were 554 ms (Low), 796 ms (Medium) and 1168 ms (High). Rendering took 165–472 ms and JPEG encoding 60–273 ms.
- Peak physical footprint reached 685 MiB (Low), 1.1 GiB (Medium) and 1.8 GiB (High) without the extension being terminated. Outside the sandbox, High at half size peaked near 0.9 GiB, with about 630 ms expected in total.
- Apple's raw support is a private system bundle with no public plug-in point. Legacy `.qlgenerator` plug-ins no longer load on macOS 15. A warm background helper could remove per-preview engine startup but adds a resident process. It is deferred until extension timings show the need.

## Slice 1: space-bar preview

- [x] Return encoded pixels in memory through `Engine::encode_image`. File export now uses it, replacing the graph's file-writing step.
- [x] Move camera white-balance and color-mode defaults into core (`resolve_camera_choices`). Slice 2 moves them into render and makes the function private.
- [x] Add `foveonic-quicklook` with the working-scale rule and a C interface returning RGB16 pixels and their ICC profile.
- [x] Add the Swift preview extension, compiled, ad-hoc signed with the sandbox entitlement and embedded by `cargo xtask bundle`. The app imports the X3F type.
- [x] Release and test workflows sign the extension before the app and keep its sandbox entitlement (`--preserve-metadata=entitlements`). Notarization then covers the extension. Checked ad-hoc locally; the Developer ID signature is first checked by a release run.
- [x] Ignored GPU check: a Merrill High original renders at 2352×1568.
- [x] Owner testing of the space-bar preview from the packaged app (2026-11-25). The first preview after installing a build took several seconds while Metal compiled every shader; later previews took about a second.

## Slice 2: one source of a photograph's defaults

Replaces the coherence cleanup item of the same name. Agreed with the owner on 2026-11-25.

As Shot is one of the white-balance and color-mode options the engine offers. Callers choose an option or leave the default, As Shot. Only the engine knows which preset As Shot means for a photograph. The GUI does not need to know.

- [x] **The engine lists the options.** `ControlCapabilities::white_balance_options` and `color_mode_options` list As Shot first, and `option_label` names each option. As Shot is `render::AS_SHOT`. The GUI menu and the multi-photo availability check use these lists. The GUI's own As Shot entry and its two list sources are gone.
- [x] **Render resolves As Shot.** `Settings::with_camera_choices` names the presets. `prepare_recipe` calls it beside `resolved_exposure`, so every render path gets concrete names and recipes do not change. Artifacts and images keep the requested As Shot, so the decoded-preview cache still matches the saved recipe. `dng::write_source` resolves once at its entry for the profile name and appearance. Automatic ColorDQ and Merrill's automatic sharpening resolve where they read the white balance. Resolution never fails: a choice the file did not record usably stays As Shot, so inspection outputs that do not use it still render, as before. `validate_for_camera` reports "As Shot is unavailable … Choose a preset" only when the rendering uses that choice. The first full GPU run caught the earlier strict version failing Quattro and Classic inspection outputs.
- [x] **Delete the other translations.** Removed from the render worker (twice) and its separate defaults-error channel, presentation (twice), preparation, the toolbar's export check, multi-edit automatic values, `decode_cache/previews.rs`, GUI export, Quick Look, the CLI's Quattro-only fill and the `profile_editing` example. `resolve_camera_choices` is deleted.
- [x] **Keep the stored form.** An empty string still means As Shot, so sidecars, presets and multi-edit's JSON-pointer edits are unchanged. The typed choice waits for 0.10.0's `Settings` split.
- [x] **The command line starts from camera defaults.** `parse_settings_for_camera` parses the options onto `camera_settings`, once to learn the chosen rendering and once onto that rendering's defaults. `--wb` and `--mode` are optional. The `--rendering`/`--mono-*` pre-scan, the reparse from `Settings::new("", "")`, the Quattro-only choice fill and `Settings::validate` are gone. Noise controls a flag edits are filled from the camera's `default_noise_controls_for_metadata`, which reads Quattro's recorded values. The profiling examples start from `camera_settings` and keep their fixed Sunlight and Standard choices.
- [x] **Sidecar noise reduction.** A Merrill recipe without noise controls gets the camera default, noise reduction on, when it is normalized for its family. The app saves the normalized recipe back. This includes 0.3.0 and 0.4.0 sidecars, which saved an off checkbox as missing controls: the owner decided on 2026-11-25 that missing means default. An explicit off, stored as controls with noise disabled, stays off. Classic keeps its own toggle, since it has no noise adjustments, and Quattro leaves missing controls to the file's recorded values.
- [x] Keep `--reconstruction` in the Merrill render cases. It changes nothing on this revision, but comparable runs drive earlier revisions with the current cases, and 0.9.x and earlier would otherwise measure Merrill without noise reduction. Drop it once no compared revision predates 0.9.2.
- [x] Update the [despeckle spec](../docs/specs/despeckle.md), the CLI help and the changelog.
- [x] The test-inventory planner accepts the Quick Look library's two kinds (`staticlib` and `rlib`) as one library test target. Slice 1 had made planning fail.
- [x] Consolidation pass (owner request, 2026-11-25). Recipe building shares one `recipe_settings` step (camera choices, Auto exposure, family processing) between `prepare_recipe` and reduced preparation. The automatic ColorDQ and sharpening values share `Settings::noise_couplings`. `validate_for_camera` asks for a recorded preset only when As Shot is used, and resolution never fails. The color guide resolves its fallback mode through the same function. `EncodedImage` carries its extent, `output::write` takes it whole and is the one place encoding options are validated, and the PNG writer reuses the shared sample reader. One `parse_args` serves the CLI and its tests. Engine-only lookups (`apply_capture_defaults`, recorded choices) are crate-private.
- [x] Review fixes. Reduced preparation (`sensor_data_graph`) built recipes without resolving As Shot, so a default Merrill photograph failed in reduced views and prefetch ("unavailable white balance"); it now resolves through the same function, with a GPU check that a reduced As Shot render equals the recorded presets. The CLI and two profiling examples again fill Quattro noise controls a flag edits from the file's recorded values, as the app does, with a unit test. The recipes' own As Shot fallbacks (Quattro photographic and DNG, the color guide) and the Merrill monochrome capture default that stored "Standard" are removed. `default_white_balance` and `default_color_mode` are crate-private.

Checks: As Shot renders the same TIFF and DNG bytes as the recorded Sunlight and Standard on a Merrill photograph, and the CLI default render matches `--reconstruction`. A unit test shows the CLI starting from the app's camera defaults, including recorded Monochrome.

Accepted cost: an As Shot render and an explicit preset with the same value are different settings, so switching between them renders again (owner, 2026-11-25).

## Slice 3: thumbnails and fallback

- [ ] **Build GPU pipelines on first use.** Measured 2026-11-25 on the M1 Pro in fresh processes: the GPU adapter and device take about 20 ms, and building all 316 pipelines at engine start takes 253–256 ms with Metal's cache warm and 1640 ms on a new build's first run. Each pipeline is built the first time `Gpu::pipeline` asks for it, folding in the color-field group's existing lazy path. One lock serializes pipeline building, because wgpu 25 keeps one error-scope stack per device across threads. The GUI keeps building every pipeline behind its startup screen through the same path (owner, 2026-11-25). A test builds every kernel so shader errors still fail CI. Measure preview startup before and after. The background helper stays deferred unless the remainder matters.

- [ ] Move embedded-preview framing selection and file orientation into `foveonic::preview`. The GUI applies only editing rotations.
- [ ] Add the thumbnail extension using the embedded camera preview.
- [ ] Fall back to the embedded preview when rendering fails. The space-bar preview otherwise always renders the raw: a Quick Look reply is one image, so it cannot show the embedded JPEG first and replace it (owner, 2026-11-25).
- [ ] The preview GPU check names a tracked photograph instead of reading `MERRILL_X3F`, following the ignored-test item in the coherence cleanup.
- [ ] Measure timings and peak memory for each Merrill size in the packaged extensions.
- [ ] Owner testing in Finder, Spotlight and the space-bar preview.

## Release 0.9.2

- [ ] Changelog entries for the preview extensions and the defaults change.
- [ ] Release checks on the 0.9.2 archives, including a downloaded build registering the extensions.

## What 0.10.0 takes over

Recorded in the [coherence cleanup](./coherence-cleanup.md). Each builds on this plan:

- A typed As Shot choice, in the `Settings` split into recipe and render parameters.
- One export path for the GUI and the command line, in scheduler step 10.
- The smaller command-line copies found while reviewing slice 1.
