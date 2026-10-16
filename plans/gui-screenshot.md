---
summary: "Share example-photo syncing through Cargo and explicitly refresh a lossless WebP GUI screenshot for the README."
---

# Automate the README screenshot

Requirements: [GUI screenshot](../docs/specs/gui-screenshot.md), [photographic-help source management](../docs/specs/photographic-help.md), and [GUI workspace](../docs/specs/gui-workspace.md).

## Steps

- [x] Add one Cargo example-sync command with missing-file imports, cached offline inventory, and source-directory overrides.
- [x] Remove Python syncing and migrate existing callers, tests, and guidance.
- [x] Add a fixed capture recipe using isolated native screenshot machinery and lossless WebP encoding in the workspace task.
- [x] Generate and inspect a screenshot, reference it from the README, and verify originals/sidecars remain unchanged.
- [x] Run workspace tests, Clippy, formatting, Python-tool tests, and documentation generation/checks.
- [x] Owner reviewed the condensed capture and approved the changes for commit and push.

## Decisions

Reuse `.local/example-photographs/` for the full published collection so capture and photographic-help tooling share sources. An ignored inventory records the last successfully synced release asset list for offline validation. Existing originals remain untouched and are not compared against remote sizes or checksums. Source sync is an explicit workflow action, never a build hook.

Capture application contents only. The recipe fixes UI dimensions in points, while native display density determines captured pixel dimensions. Keep the graphical/GPU capture out of release CI. No README prose is to be edited as part of adding its image reference.

## Layout review

The initial 1440×1000-point capture was too wide. The owner requested the more condensed layout in the supplied screenshot. Measuring that reference gives a roughly 2650-pixel-wide content area, with 264-point organizer and 330-point tools panes at 2× display density. The revised 1325×1097-point recipe produces a 2650×2194-pixel capture. Zoom 2.1× relative to Fit and a fixed pan keep the crop close to the reference, with a 64% navigator zoom readout.

## Validation

The real online sync discovers all seven published X3Fs and skips the seven cached originals. Offline inventory validation succeeds. Native capture completes and the encoder verifies a pixel-exact WebP round trip before replacing the asset. The compact image is approximately 5.2 MiB. All fourteen original/sidecar file hashes remain unchanged after capture, and every line of the owner's original README prose remains present.

Workspace tests pass, including 114 GUI tests and 16 task tests. Source-sync tests cover missing-file import, unsafe/duplicate names, preservation of existing originals regardless of their size, sidecar preservation, failed/incomplete staging, and offline missing-file reporting. Screenshot tests cover lossless encoding, preserving the previous asset on bad input, inherited-mode isolation, recipe validity, and shared CLI source options.

Strict all-target Clippy, formatting, documentation generation/checks, five artwork tests, and four Python comparison-tool tests pass. The optional Python rendering test is skipped because Pillow is unavailable in the selected interpreter. No processing algorithms were changed. Validation performed no tag operations or publication. The owner approved the condensed capture and authorized committing and pushing the changes.
