---
summary: "Editable layered-X artwork feeds one native Icon Composer document with glass-free light and dark appearances."
---

# Desktop assets

`logo.svg` is the current editable revision: the original face polygon/projection, 24-unit slabs with 16-unit gaps, distinct cobalt/yellow-green/vermilion layers, and a softly lit graphite face. Its viewBox tightly bounds the projected geometry.

`generate-icon.py` derives a narrow inward bevel on the top X from that face's edges. Upper-left-facing edges are lighter; opposite edges are darker. The bevel is opaque and does not add refraction, blur, or a white outline. The dark artwork changes only the neutral top slab to softly lit silver; colored slabs and all geometry remain identical. Both generated foreground SVGs in `Foveonic.icon/Assets/` are unmasked transparent 1024×1024 canvases. The visible logo occupies 90% of the Composer canvas width as a review choice, not an Apple requirement.

The owner-created [`Foveonic.icon/icon.json`](./Foveonic.icon/icon.json) controls backgrounds and platform rendering. Its default specialization uses a near-white-to-cool-gray background and graphite X; its dark specialization uses a charcoal-to-near-black background and silver X. Foreground glass, specular effects, translucency, and group shadow are explicitly disabled for both. Use specialization arrays instead of retaining plain `fill` or `image-name` fields alongside them: Composer gives plain fields precedence and silently ignores those variants.

Xcode's `actool` compiles the document for macOS 26 with `--standalone-icon-behavior all`, producing `Assets.car`, `Foveonic.icns`, and compiler-owned icon plist keys. [`build-macos-app.py`](../build-macos-app.py) packages them in a local `.app` with a stable bundle identifier. macOS selects the icon appearance according to system icon settings; the Rust app does not set a static Dock image. Direct Cargo launches are not the supported macOS icon workflow. The separate dark document and static Dock override were removed.

`iconutil` extracts the compiled default ICNS rendition as `icon.png` for non-macOS window icons and regression checks. Its opaque tile spans 824 pixels, centered with 100-pixel margins in the 1024 canvas; these margins come from Apple's compiler. Composer's `ictool` preview fills the canvas and is unsuitable as a direct Dock image. No manual mask, padding, or guessed shrink factor is added.

Regenerate artwork and the static PNG from the repository root:

```sh
python3 crates/foveonic-gui/assets/generate-icon.py
python3 -m unittest discover -s crates/foveonic-gui/assets -p 'test_*.py'
```

Generation requires Xcode's macOS 26-capable `xcrun actool` and macOS `iconutil`. `--compile-dir /tmp/foveonic-icons` retains compiled resources for inspection. `--size 32 --output /tmp/icon-32.png` extracts the default small rendition with native margins; supported sizes are 16/32/64/128/256/512/1024. Native dynamic resources are build outputs, not an additional artwork source. Nothing is installed by generation.

Edit `logo.svg` for projection/palette/slab changes; edit the Composer document for background/platform styling. The generator never overwrites `icon.json`. Reload the document in Icon Composer after outside edits before saving from the app. Do not edit generated foreground/raster files as a second artwork source.

[Apple's guidance](https://developer.apple.com/design/human-interface-guidelines/app-icons) specifies centered 1024×1024 square layers and system masking/effects, without a fixed foreground percentage. The [review plan](../../../plans/icon-background.md) records research, owner rejection of Liquid Glass, and checks.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[generate-icon.py](./generate-icon.py)** Derive light/dark beveled artwork and compile one native macOS icon with automatic appearances.

<!-- INDEX:END -->
