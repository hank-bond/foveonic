---
summary: "Verification tools measure Merrill native control mappings and pixel consumers without modifying reference files."
---

# Verification tools

Run only owner-approved investigations, with native workloads sequential and explicit input/library identities. Installed libraries and reference repositories are read-only. Never treat a Rust-generated expected image as independent native evidence. Keep application exports, native engine replay, and numerical component checks separately labeled. Preserve failed captures and do not silently replace existing output directories.

## Running measurements

Setup and component captures use Python 3.11 or later with its standard library. Full-frame replay also needs NumPy, Xcode's C++ compiler, and LLDB on ARM64 macOS. The tools do not install dependencies. Library identity is pinned before native calls.

Replay takes a JSON case list: `name`, original `source`, decoded `camf`, full-sensor RGB Float32 `input`, and numerator/denominator pairs for `aperture` and `shutter`. Optional `resolution`, `monochrome`, `chroma`, and `luminance` select the request. Supply preprocessing inputs explicitly; replay does not decode or preprocess photographs. Choose a new output directory and retain at least 10.25 GiB free disk space.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[column_presets.py](./column_presets.py)** Retain guarded synthetic native color-column samples for all five banding presets.
- **[noise_controls.py](./noise_controls.py)** Measure all Merrill native noise presets and boundaries without changing processing or reference files.
- **[replay_banding.py](./replay_banding.py)** Replay all five native Merrill banding positions on identical inputs and retain consumer/output evidence.
- **[trace_banding.py](./trace_banding.py)** Trace installed Merrill Stage 2 and actual column arguments on owned diagnostic inputs.

<!-- INDEX:END -->
