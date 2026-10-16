---
status: archived
summary: "Manage release-hosted example photographs, explicit comparison rendering, and checked-in images through one shared popover template."
---

# Manage photographic help comparisons

## Closure

The owner accepted the implemented photographic and interface results and authorized closing review-only plans. The implemented examples and tooling are accepted. Pending-review statements below describe earlier checkpoints. Unembedded advanced-control candidates remain optional future examples, not newly approved assets.

Requirements: [photographic help](../../docs/specs/photographic-help.md), [GUI workspace](../../docs/specs/gui-workspace.md), and [image information bar](../../docs/specs/image-information-bar.md).

## Steps

- [x] Add a manifest format for named sources, fixed crops, shared recipes, variant labels, and captions.
- [x] Download missing supplemental-release X3Fs into an ignored local directory.
- [x] Add an explicit render command producing sRGB PNGs and a review sheet.
- [x] Embed checked-in images through a shared popover template, with offline build checks for missing references.
- [x] Add the owner's Off/Auto/+0.8 Highlights example and test selection without changing photo edits.
- [x] Check tooling, GUI interactions, documentation, and offline builds.
- [x] Apply owner-reviewed header/card styling and display the example photograph's name from its source filename.
- [x] Keep White balance and Color mode rows visible while metadata loads, without moving the editing controls below them.
- [x] Simplify downloads and generation by removing checksum assets, hash calculations, and render receipts.
- [x] Add the owner's complementary Bad pixels and Speckles examples, with the other correction enabled in each pair.
- [x] Add the brighter +3 EV recycling-bin Row banding example without amplifying the removed pattern.
- [x] Prepare local comparisons for all seven advanced Noise and Detail controls on three shared crops.
- [x] Preserve linked Mean decay limits while allowing manual values through 1, then prepare higher-value comparisons for owner review.
- [x] Owner accepted the implemented examples and interface and closed the visual-review checkpoint.

## Agreed approach

The long-lived [Example photographs release](https://github.com/hank-bond/foveonic/releases/tag/example-photographs) holds original X3Fs without editing sidecars. New examples are appended without replacing published source files. Comparisons identify sources by official filename; manifests record the crop and processing settings, and Git tracks the manifests and PNGs. Source downloads and image generation are separate explicit steps. Normal application builds require only checked-in manifests and PNGs, never the original photographs, network access, Python, or GPU rendering.

Each comparison has one directory. The manifest owns its brief explanation, crop, shared CLI recipe, and two or more labeled variants. A shared egui template switches a fixed-size image and caption using variant buttons. Popover interaction must not change processing settings or trigger renders. Existing text-only help remains unchanged for controls without examples. The initial Highlights example uses the owner's specified crop and −0.5 EV recipe; final native visual review remains an owner checkpoint.

## Complementary defect examples

A three-state A/B survey of all seven official photographs located isolated visible changes with both corrections off, Bad pixels only, and both on. Four-state follow-up renders added Speckles alone. The first proposed Bad pixels dot at `(935,1968)` in `turquoise-still-life.x3f` is repaired identically by either control, so it does not distinguish them.

The owner instead selected the 2×2 magenta leaf defect at `(1582,2737)` in `glossy-leaves-and-buds.x3f` for Bad pixels, and the isolated magenta turquoise dot at `(1278,1993)` for Speckles. Four-state renders confirm that only Bad pixels fixes the leaf patch and only Speckles fixes the turquoise dot. Each help pair keeps the companion correction enabled, with Noise and Detail and sharpening off. The approved crops and recipes are recorded in the [requirements](../../docs/specs/photographic-help.md#bad-pixels-and-speckles-examples) and comparison manifests.

The owner subsequently requested simpler copy without semicolons. Descriptions, notes, and captions now use shorter sentences. Image recipes and PNGs are unchanged.

Help refers to the camera's recorded defect information without assuming its factory/service origin or whether the camera updates it later. The reference implementation establishes how records are consumed, not the camera-side map lifecycle.

## Row banding example

The reference implementation's row-banding guide shows individual camera layers with a narrow fixed display range. Its optional 8× before-view illustration amplifies the removed row pattern, while the actual 1× correction is subtler. The current recycling-bin sidecar already has Noise and Detail off, so smoothing does not explain the owner's current comparison.

Fresh color comparisons at Exposure 0, +3, and +5 EV keep the correction itself unchanged. The owner selected +3 EV with Noise and Detail off. The 400×400 crop near the report's region preserves native pixels and the photograph's orientation. The manifest renderer now produces the integrated Off/On pair with simple captions. Its checkbox opens the shared popover, including when disabled. No illustrative amplification was imported into photographic help.

## Advanced Noise and Detail review

The owner approved an initial local sweep of all seven advanced controls. Three native 512×512 crops use the stroller's light/shadow boundary, the leaf and bud, and the parking sign's white lettering. Each control has its linked default and three manual samples. All noise presets stay at 2, Detail at 1, Exposure at 0 EV, and sharpening off. Technical corrections remain enabled and ColorDQ stays automatic. These are review candidates, not embedded help assets.

The first 66 candidate renders show the clearest changes for Edge sensitivity and Layer detail mix. Base filter blend changes leaf texture. Mean edge cutoff is more subtle. Luma noise scale and Brightness mix remain weak standalone examples at Detail 1. The sign's reflective pattern remains a limitation for a clean edge lesson.

The sweep found that reduced local filtering capped manual Mean decay after override resolution. The leaf and sign's linked values around 0.95, explicit 0.92, and explicit 0.99 produced identical sRGB crops. Repeated baseline renders were also pixel-identical on all three crops. Source inspection confirmed the 0.92 ceiling in the reduced-local branch.

The owner chose to retain the linked ceiling and try the full manual 0–1 range. Shared parameter preparation now resolves the linked ceiling before applying an override. The GUI displays the effective linked value so unlinking still preserves the image. The later unconditional ceiling is removed rather than introducing another processing path.

A follow-up renders linked, manual 0.92, 0.95, 0.99, and 1 on all three photographs. Full-resolution PNGs accompany the native crops for seam review. All three linked crops remain pixel-identical to the previous renderer. The leaf and sign now display linked 0.92, which matches explicit 0.92. Higher manual values change the images, but the effect remains subtle at Detail 1. Relative to linked output, manual 1 changes the crops by mean 0.27–0.31 and maximum 3–15 sRGB 8-bit codes. This is not yet a strong standalone help example.

Workspace tests, strict all-target Clippy, reference parameter fixtures, reduced-local/global GPU fixtures, and advanced override GPU checks pass. Capturing linked values preserves exact prepared coefficients and GPU base pixels. Manual 0.95, 0.99, and 1 change the base and retain finite base and reconstructed output. The offline macOS app bundle is rebuilt. Owner visual review remains pending.

## Findings and checks

Initial native review found the example note's default 9 pt Small style too tiny. The template now uses normal body text for notes and captions. Follow-up owner review restored the normal heading, with the extension-free photo name to its right in body-size italics. An offset border groups the image and its attached lighter-gray caption. The source label is derived when embedding the catalog, so no new manifest field or image regeneration is needed. Final visual review of this revision remains pending.

The same review identified loading jitter in Editing: camera-choice rows were only created after the first render supplied metadata. Both rows now remain visible with saved values or placeholders, disabled until their camera lists are available. Pointer/layout tests cover absent, empty, and ready metadata, saved/unresolved values, unchanged control positions, ignored loading clicks, and selection once choices become available. Processing and metadata scheduling are unchanged.

The release contains seven official X3Fs. The owner removed distributed sidecars, then chose to remove checksum assets and render receipts as unnecessary for this workflow. Sync now asks GitHub CLI to download missing X3Fs, skipping existing files and leaving local sidecars untouched. Rendering uses the manifest's explicit CLI arguments, never implicit sidecar loading, and freezes the executable per run. The four comparison manifests and their nine PNGs are prepared for commit. Rendering-setting changes require explicit regeneration and review; copy edits only require rebuilding the app.

The build script validates manifests and PNG references and generates a small embedded Rust catalog. Its shared validator has tests for missing variants, unsafe paths, and wrong dimensions without access to originals or a renderer. GUI tests decode every embedded PNG and switch all variants of all four comparisons at unchanged image bounds. They open/click Highlights through label, slider, and value in Auto/manual and enabled/disabled modes, and exercise all three correction popovers with their checkboxes on/off and their controls editable/disabled, without changing photographic settings. A tooltip initially offered only egui's default Area height, clipping the example; explicitly offering screen height fixes that while a scroll region handles smaller windows. Caption height is reserved across states to avoid shifting the image.

Workspace tests pass, including 106 GUI tests with eight opt-in fixtures skipped. The presentation test checks font sizes and italic flags, shared header alignment, the offset border, and the connected caption background. Variant switching retains the image bounds. Six example-tool tests pass with Pillow, including companion-correction and brightened Row banding recipe checks, as do three app-bundle tests, strict workspace Clippy, workspace formatting, documentation generation/read-only checks, and whitespace checks. The updated offline release app bundle builds successfully. Final native visual review remains pending; no new GPU algorithm tests were needed because processing code is unchanged.
