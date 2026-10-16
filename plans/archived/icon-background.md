---
status: archived
summary: "A local macOS app bundle supports native automatic light/dark layered-X icons with opaque colored slabs and a restrained bevel."
---

# Refine the application icon

## Closure

The owner accepted the implemented interface results and authorized closing review-only plans. This implementation slice is complete. Earlier visual/system-style review checkpoints are closed by owner direction, not by a new automated system-style test.

The owner requested Apple guidance/template research, stronger top-to-bottom background shading, clearer colored layers, thicker slabs, a lit top face, and a 90% width trial. The default Liquid Glass trial was rejected; the approved replacement is a narrow, opaque bevel consistent with upper-left lighting. Requirements: [GUI workspace](../../docs/specs/gui-workspace.md).

## Steps

- [x] Retain an editable SVG source.
- [x] Read current Apple icon guidance and inspect native Icon Composer workflow.
- [x] Increase slab thickness 16→24, retain 16-unit gaps, distinguish cobalt blue/yellow-green/vermilion, and shade the graphite face.
- [x] Trial 90% visible logo width and a stronger light-to-dark background gradient.
- [x] Use the owner-saved Composer document and its native macOS mask, replacing the handmade-mask generation path.
- [x] Disable foreground Liquid Glass/specular/translucency/group shadow; add a narrow directional bevel to the top face.
- [x] Check 1024/128/32 native exports, source geometry, bevel orientation, opaque colors, and explicit glass-free settings.
- [x] Run Python/Rust tests, native GUI launch, Clippy, formatting, and documentation/index checks.
- [x] Investigate oversized Dock tile and replace full-canvas preview output with Apple's production-compiled ICNS and native margins.
- [x] Combine light/dark artwork and background specializations in one Composer document.
- [x] Build and launch a local `.app` with Apple's compiled asset catalog and icon metadata; remove the runtime static override.
- [x] Migrate instructions/tests and verify compiled appearances, bundle contents, and native launch.
- [x] Owner authorized grouped commits after code and automated checks.
- [x] Owner accepted the implemented appearance and closed the visual/system-style review checkpoint.

## Research and decisions

[Apple's current HIG](https://developer.apple.com/design/human-interface-guidelines/app-icons) specifies 1024×1024 square unmasked layers, centered content, simple shapes, vector foregrounds, and background colors/gradients. It gives no mandatory foreground fill percentage. [The Composer workflow](https://developer.apple.com/documentation/xcode/creating-your-app-icon-using-icon-composer) lets the system render the mask and materials; importing a pre-masked silhouette is discouraged.

An official production-template download stopped at its license prompt; no agreement was accepted and no template content was imported. The owner instead saved the native `Foveonic.icon` document. Installed Xcode's older export tool lacked local help; the newly installed standalone Composer provides documented `ictool --help` and supports generation-26 macOS exports. The native renderer now supplies the mask, rather than an independently copied or guessed template curve.

The first Composer preview enabled default materials and produced broad glossy rims and washed/refraction-like colors over the entire foreground. The owner rejected it emphatically. It stayed a temporary preview, not the runtime raster. Current document settings disable those foreground effects. JSON field conventions were checked against the publicly readable Maccy icon document; no Maccy artwork was imported.

## Implementation

[`logo.svg`](../../crates/foveonic-gui/assets/logo.svg) retains the face polygon and projection while revising depth, palette, and light. The owner removed the obsolete initial-import copy. [`generate-icon.py`](../../crates/foveonic-gui/assets/generate-icon.py) reads it without modifying it and derives twelve 1.4-source-unit inward chamfer facets on the top X. Facet normals determine brightness toward upper-left light; concave corners stay sharp without an outside halo.

Generated unmasked foreground SVGs are 1024×1024, with a 90% visible-width placement. `Foveonic.icon/icon.json` owns the light/dark background gradients, foreground specializations, and disabled glass settings. Generation updates both foregrounds, then invokes Xcode's production `actool` for macOS 26 with all standalone icon sizes. It produces `Assets.car`, `Foveonic.icns`, and icon plist metadata; `iconutil` extracts the default 1024-pixel rendition as `icon.png`. The manual `icon.svg` composition was removed. The document remains editable in Composer.

The [Cargo tasks](../../crates/xtask/AGENTS.md) now create a local `Foveonic.app` beside the reported Cargo executable, package native icon resources, merge the compiler's icon keys into `Info.plist`, and launch via `open`. The stable bundle ID is `org.foveonic.foveonic`. Eframe's existing `foveonic` preference ID is unchanged. No static Dock override remains. Local bundled launches replace direct binary launches on macOS. The original Python packager was migrated in the [release preparation](./cargo-releases.md), which also adds unsigned draft-release builds.

## Evidence

- Python tests cover inset geometry, directional shading, twelve opaque facets, unchanged source elements, 90% placement, and explicit disabled foreground effects.
- GUI raster tests check a light opaque top, darker bottom, transparent corner, dark face, and distinct RGB palette within the export's approximately one-code sRGB dithering.
- Native exports at 128/32 retain band separation and a subtle top-face edge, without the rejected glass rim.
- Tests, Clippy, builds, and twelve isolated native launch frames pass.

## Correct Dock sizing

The owner's Dock screenshot established that the entire tile, not its foreground, was oversized. Composer's `ictool --export-image` produces a full-canvas preview; it does not include macOS application-icon padding. The HIG describes a 1024-square design canvas but does not prescribe an arbitrary shrink percentage for this export. The earlier suggested 82% was only a screenshot estimate and was not implemented.

The `actool` manual included with Xcode documents `--app-icon`, `--compile`, `--platform`, and `--standalone-icon-behavior all`. Compiling the actual owner document produced `Foveonic.icns` with native masks/margins and size-specific renditions. Its 1024-pixel PNG has a centered 824-pixel opaque tile, 100-pixel margins each side (80.46875% width), with translucent shadow outside. The screenshot's neighboring roughly 128-pixel tiles versus our previous roughly 156-pixel tile corroborates that normalization; it is not the source of the geometry.

The generator uses this production pipeline directly: no manual resizing, alpha padding, guessed percentage, or custom mask. The bundle contains the compiled native asset catalog and fallback ICNS so macOS controls appearance and size renditions. The logo's existing 90%-of-tile placement stays unchanged. Raster tests guard the half-alpha tile boundary at x=100 and x=924 on the center scanline, alongside the existing background/palette tests.

Native resources now live only in the local bundle as build outputs; no second installed icon path is maintained.

## Automatic appearances

After reviewing the separate light-X/dark-background variant, the owner approved native automatic switching and migration to a local `.app` launch workflow. One `Foveonic.icon` now contains default and dark background/image specializations; the separate dark document and static review ICNS were removed. Both foregrounds derive from `logo.svg`. RGB elements and polygon geometry remain identical.

Composer field conventions were researched using public Maccy/CotEditor documents and the installed tool; no artwork was imported. An export experiment established that plain `fill`/`image-name` fields take precedence over corresponding specialization arrays. Removing the plain fields and specifying both default and dark entries produces the approved dark background and silver X. Tests guard this silent precedence trap.

Checks cover unchanged colored layers and geometry, light-face gradient, directional bevel shading, both background settings, specialization references, bundle resources, compiler-owned metadata, executable permissions, rebuild cleanup, custom Cargo target paths, and launch argument preservation. `assetutil` confirms Aqua/DarkAqua icon stacks, different background gradients, and foreground/dark-foreground vector references. `NSWorkspace.icon(forFile:)` recognizes the local app as a native `ISBundleIcon`; drawing it under an app-local dark appearance alone does not override system icon-style settings, so live system-style switching remains for owner review rather than changing the owner's preferences during tests.

Composer renders verify both appearances. Debug/release bundles build successfully; direct bundled and LaunchServices smoke launches each captured twelve workspace frames with isolated preferences. Eight Python tests and 26 GUI tests pass; GUI Clippy passes. The earlier concurrent `smoke.rs` formatting discrepancy is resolved; final workspace formatting and diff checks pass. Owner visual/system-style review remains pending.

The owner authorized grouped commits; no system installation or bundle publishing was performed.
