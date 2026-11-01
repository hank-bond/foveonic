---
summary: "Prepare the reviewed Merrill noise and banding feature set as a local release candidate."
status: archived
---

# Prepare release 0.6.0

Follow [application releases](../../docs/specs/releases.md), [Merrill multiscale noise](../../docs/specs/merrill-multiscale-noise.md), [Merrill banding](../../docs/specs/merrill-banding.md), [sensor defect calibration](../../docs/specs/sensor-defect-calibration.md) and [the optimization plan](./merrill-noise-latency.md).

The owner approved continuing local release preparation after reviewing the optimized app. Keep the reviewed algorithms and both multiscale choices. Independent axis estimation and the user calibration workflow remain follow-ups. The isolated branch already descends from current committed main. Unrelated uncommitted work in main is outside this candidate. The owner subsequently approved committing the verified changes. Tag, push and publication remain review actions.

- [x] Finish first-render and memory measurements, distinguishing completed engine work from native feedback.
- [x] Verify camera-specific sparse defect locations are absent from the shared noise model and current speckle controls.
- [x] Confirm new-photo defaults with the owner.
- [x] Exercise preview, method switching, persistence, TIFF and DNG paths.
- [x] Prepare versioned notes, current help and release bundle.
- [x] Run final checks and record remaining limits for review.

The owner selected Reference defaults for the release. Multiscale and Directional remain selectable. The review build uses isolated preferences and does not establish a new application default. Existing valid authored settings retain their choices.

The shared shield model contains fitted row/column coefficients and broad column reference curves. The guarded row estimator contains covariance priors. These are generalized noise assumptions, not a camera-specific sparse defect map. Speckle detection reads the current image and camera metadata. The user calibration interface remains unimplemented. The release must not advertise it or claim transfer to every sensor or capture condition.

## Measurements and photographic checks

A fresh Coarse stress run with profiling Off retained variable completed times. Five warmed fresh samples had medians of 2.69 seconds before measurement reuse and 2.38 seconds after, with overlapping ranges of 1.71–3.09 and 1.67–2.72 seconds. This is not evidence of a stable fresh-render speedup. Repeated native row-strength edits fell from 1.24 seconds to 278 ms, column-strength edits from 1.09 seconds to 698 ms, and row-size edits from 1.05 seconds to 681 ms. The first edit still warms the frontier. GPU initialization and source decoding were outside these intervals. Runs were sequential and had no concurrent builds or measured renders. The open review app was not a controlled native workload.

The same stress processes reported peak physical footprints of 9.81 GiB before and 9.64 GiB after, with maximum resident sets of about 775 and 779 MiB. These process metrics include driver allocations and are not isolated peak GPU residency. Both reported zero swaps during the process. A separate settled review-app snapshot reported 1.6 GiB physical footprint and a 4.8 GiB historical peak. Its interaction history was uncontrolled and included photo changes. The existing one-GiB checkpoint limit remains unchanged. Transient memory and fresh Coarse latency remain follow-up work.

The owner reported a bright magenta speckle near the speaker in the dark DP3 capture. Inspection traced it to a two-pixel native cluster at row 1981, columns 3343–3344. It exists before calibration and multiscale filtering. Camera metadata lists one member, but not its neighbor. The immediate-ring detector retains the cluster because each point sees another bright neighbor. Compact Size 5 reaches the normal surrounding ring and removes it. Matched Reference and Multiscale renders both retain it with Reference or Compact Size 3, independently of banding being off, Reference or Directional. No denoiser regression was found. Existing compact GPU coverage checks cluster removal at Size 5/7, Size 3 retention and preservation of straight scene lines. A small matched photographic comparison is retained with the review artifacts.

The broader ignored-test sweep found two stale comparison setups. The tile-bypass fixture left speckles enabled in its no-reconstruction baseline even though speckles now follow the explicit Noise bypass. That fixture now disables speckles in both paths to isolate tile filtering. The DNG fixture put active HDR, Local Contrast and Universal sharpening into its supposedly deferred presentation. Those edits have been baked since the shared-finishing feature. The fixture now compares global presentation independently, asserts that spatial edits change samples, and checks profile identity at a fixed tone method. Neither correction changes photographic processing.

## Local review handoff

The workspace's 677 ordinary tests, strict release Clippy, formatting, documentation indexes and artwork checks pass. Twenty-five focused GPU checks pass across method switching, regional and reduced previews, monochrome, directional measurement reuse, independent band equations, speckle neighborhoods, shadow brightness protection and DNG sampling/presentation. The earlier twelve complete buffer comparisons remain exact. Three selected Color/Monochrome recipes write complete TIFF and DNG files. Independent file checks verify oriented dimensions, three 16-bit channels, nonconstant sample data, TIFF ICC profiles and DNG version/color-matrix tags. File structure and numerical checks do not establish third-party editor appearance.

The local bundle reports 0.6.0, Apple Silicon arm64 and macOS 15.0 in both bundle metadata and Mach-O load commands. Local ad-hoc signature verification and command-line startup pass. A separate review identity with isolated preferences launches on macOS 26.6.2 and renders a Full photograph. The owner opened a card in that window, so automated interaction stopped without changing the selected photographs or editing values. Native mode/preview interaction remains owner review. Automated GPU recipe checks cover those processing routes. Interactive macOS 15 compatibility remains unverified.

The local ZIP is for review. The candidate was prepared before the final optimization commit approval. The distribution workflow must still produce its Developer ID signed draft and checksum after approved commit/tag/push. No tag, push or publication was performed during local preparation. The owner subsequently held release preparation for further optimization. This bundle predates the resource-overlap changes in [the optimization record](./merrill-noise-latency.md) and must be rebuilt and checked before release. Generalized calibration transfer, the user calibration interface and independent axis estimation remain separate work.
