---
summary: "Download official photographs and explicitly render named comparisons whose PNGs are embedded in photographic help."
kind: how-to
---

# Maintain photographic help examples

The [Example photographs release](https://github.com/hank-bond/foveonic/releases/tag/example-photographs) is a growing supplemental collection, independent of application versions. Append new X3Fs to that release without replacing existing source files. Editing sidecars are not distributed. Comparisons identify photographs by their published filenames and record the crop and processing settings in their manifests.

## Download originals

From the repository root:

```sh
cargo xtask examples sync
```

This uses authenticated GitHub CLI (`gh`) to download the release X3Fs into `.local/example-photographs/`. It stages missing photographs before importing them and skips existing files without comparing or replacing them. Existing local editing sidecars are left alone. Remove or move an original aside if you want to download it again. Use `cargo xtask examples sync --source-dir PATH` to select another directory.

A successful online sync records the release asset inventory in `.release-assets.json` inside that ignored cache. `cargo xtask examples sync --offline` checks that all photographs in the last synced inventory are present without contacting GitHub. It cannot discover photographs appended since that sync. The [README screenshot task](gui-screenshot.md) uses this same cache and sync implementation.

Originals, local edits, full TIFFs, and review sheets stay outside Git. Ordinary builds do not download anything or require this directory.

## Add or edit a comparison

Create a directory beneath [`assets/help/`](../crates/foveonic-gui/assets/help/AGENTS.md), named for a stable help ID. Its `comparison.json` follows the [Highlights example](../crates/foveonic-gui/assets/help/highlight-correction/comparison.json):

- `id`, `title`, `description`, and `note` supply the shared popover copy. State companion settings in the note when needed to understand the effect.
- `source` names an official X3F file.
- `crop` gives `[left, top, right, bottom]` in oriented export pixels, with right and bottom **exclusive**. Convert an inclusive viewport readout by adding one to its final x/y values.
- `image_size` gives the shared PNG width and height. Preserve the crop's aspect ratio when choosing it.
- `base_args` records the shared CLI processing recipe. Rendering does **not** load local editing sidecars, so later GUI edits cannot alter the example.
- `variants` contains two or more entries with stable `id`, visible `label`, `caption`, PNG `image` filename, and variant-specific CLI `args`. Keep unrelated settings fixed.

Use plain filenames, not paths, for source and image names. Put the explanation in the manifest rather than duplicating it in Rust. Bind the control's help key to the comparison ID in [`help.rs`](../crates/foveonic-gui/src/help.rs); the shared template handles image layout, buttons, captions, and disabled-control hover behavior.

## Render and review

Build the renderer explicitly:

```sh
cargo build --offline --release -p foveonic
```

Use a Python environment with Pillow and its LittleCMS support already available. No tool installs dependencies automatically.

```sh
python3 scripts/help-examples/examples.py render highlight-correction
```

For a different cache, pass `--source-dir PATH` before the Python `render` subcommand. The command freezes the renderer executable for that run, exports every variant, applies identical crops and Lanczos scaling, and converts the TIFF's embedded color profile to sRGB. Candidate PNGs and a side-by-side `review.png` go into `.local/help-renders/{id}/`. Full TIFFs and the frozen executable are temporary.

After reviewing the images and copy with the owner:

```sh
python3 scripts/help-examples/examples.py render highlight-correction --write
```

This explicitly updates the comparison directory's PNGs. Review those changes before committing. The manifest records the source photograph, crop, image size, and processing arguments; Git tracks the manifest and PNGs together. Regenerate images when those rendering settings change. Correcting copy alone only requires rebuilding the app.

## Build and check

```sh
cargo test --offline -p foveonic-gui
python3 -m unittest discover -s scripts/help-examples -p 'test_*.py'
```

The GUI build scans comparison directories, validates their manifests, checks each referenced PNG's existence and dimensions, and embeds those bytes. It needs neither Python, original photographs, network access, nor GPU rendering. Missing images fail the build with an explicit-generation instruction. GUI tests decode all embedded PNGs and exercise comparison switching; Python tests check safe manifests, image references, and explicit output updates. Cargo task tests cover source imports and offline inventory checks. Rendering tests run when Pillow is available.

The shared hover popover follows the [presentation rules](./specs/photographic-help.md#popover-presentation). It displays the example photograph's name automatically from `source`, without its extension, and keeps one crop at a fixed scale, with buttons for its labeled variants and a caption below. Its selection is temporary UI state, never a photographic edit or an export setting.
