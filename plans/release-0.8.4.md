---
summary: "Prepare and validate 0.8.4 with reviewed capture-metadata and TIFF interoperability fixes."
---

# Release 0.8.4

The owner requested another patch after reviewing the [EXIF fixes](./exif-export.md). Follow the [release requirements](../docs/specs/releases.md), [release guide](../docs/releases.md) and [test inventory](../docs/testing-coverage.md). Scope is capture metadata, the pinned TIFF alignment fix and maintained export checks. Processing algorithms, encoded image samples and DNG profile values are unchanged.

- [x] Review implementation, metadata, encoder and independent-reader checks.
- [x] Set version and notes to 0.8.4 and review the release diff.
- [x] Run available workspace, GPU/original-file, Python and native GUI correctness gates and record unavailable checks.
- [x] Obtain commit/tag approval and build production archives into a draft.
- [x] Verify exact archives, developer symbols and native startup/rendering on both desktops.
- [x] Compare packaged latency and memory with published 0.8.3 and present meaningful differences for owner review.
- [ ] Obtain publication approval, publish and record final identities.

The owner approved committing and pushing source `544c233b48504a38945639e0ab95533d2d3369f8` to main and creating/pushing `v0.8.4`. This did not approve publication. The [production workflow](https://github.com/hank-bond/foveonic/actions/runs/43999562361) passed all four jobs. Sol reviewed the implementation, release preparation and correctness orchestration.

## Completed verification

Targeted export evidence includes 645 active core tests, 35 real-source exports, 33 encoder combinations and 125 warning-free Adobe SDK reads with exact decoded samples. The [shared EXIF checks](../crates/foveonic/tests/common/capture_exif.rs) are the single source of validator-warning decisions and justifications. Adobe parsing/decoding evidence does not establish visual appearance approval.

Ordinary workspace tests and all-target Clippy pass on both owner test hosts. Python suites pass: six artwork tests, 199 profiling tests with one existing platform skip, and 35 supporting-script tests. Private Windows tests initially lacked DXC beside compiler-reported test binary paths. Staging the existing pinned DLLs resolved that host setup failure. The tagged hosted Windows workspace job also passes serially.

All 455 selected GPU/original-file checks pass: 408 core and 47 GUI/diagnostic checks. Another 154 ignored research, independent-golden or measurement checks remain excluded with their input/measurement reasons. The initial combined-original group passed 148 of 149 because the runner assigned mixed camera architectures to a Merrill-only decoder test. That test passes separately with the pinned full-resolution Merrill fixture. The initial result is retained. A lost SSH connection interrupted orchestration after 42 GUI worker checks. Only the five remaining diagnostics checks were resumed, and all pass.

All 48 selected native catalog jobs pass, including 1,440 slider cases, the 144-case numerical comparison, transitions, low-memory/native-detail work, lifecycle/browser/collection/choice journeys, diagnostic save-on-exit and pending-work shutdown. Every numerical output matches published-source 0.8.3 samples and geometry byte for byte. Processing-mode statistics complete without nonfinite failures.

Both actual production packages pass the separate native verifier's ten checkpoints with normal application stacks. Archive checksums, macOS Developer ID signature, hardened runtime, timestamp, arm64 architecture and executable/bundle macOS 15.0 deployment targets verify. Retained developer symbols match both executables. Public archives exclude symbols. The Windows package has its pinned DXC libraries and normal 1 MiB stack.

## Correctness concurrency

The owner authorized concurrent correctness work with section-specific memory guards, while performance stays exclusive. Twenty successful reused slider sessions cover the 1,440 cases. Additional interrupted and guard-deferred attempts remain recorded. Each completed 58-case Classic group took about 245 seconds serially: 174–175 seconds through native actions and approximately 70 seconds for independent verification. These are harness durations, not application latency measurements.

The serial run preserved 580 verified cases and the exact first 38 catalog jobs. A private controller imported the unchanged maintained assertions, isolated worker homes/settings/sidecars/caches/evidence and allowed only its exact owned app/renderer descendants past the quiet-host guard. Foreign workloads remained checked. Concurrent correctness timing stayed invalid. The ten-worker phase used 2 GiB per-process guards and a 24 GiB aggregate sampled guard. It completed four Merrill-reduced groups and deferred Quattro families that crossed 2 GiB. Two 4 GiB calibration groups then passed. After auditing all stable native and Python terminal records, the remaining six groups ran with 4 GiB guards. No terminal assertion/image failures were omitted or silently retried. Known guard deferrals and owner-interrupted attempts remain separate from successful evidence.

The final merge requires all 1,440 unique declared case keys, passing native/session/fresh-image assertions, exact app/renderer identities, declared ordered sources and recipes, and retained native-result hashes. Successful group metadata excludes interrupted attempts. Final controller exit status agrees with acceptance failure. The initial controller's latent zero-exit-on-failed-tail issue did not determine test success.

Observed native capacity peaks are approximately 1.5 GiB Classic, 1.7 GiB Merrill high, 1.3 GiB Merrill reduced, 2.5 GiB Quattro and 3.5 GiB Quattro H. The final six-worker aggregate sampled peak was approximately 14.6 GiB. These observations size correctness workers and do not substitute for matched release memory comparisons. Successful checked floating-point/TIFF captures are pruned with SHA-256 receipts. Approximately 96 GiB was removed while retaining JSON assertions, screenshots and failure/interrupt records.

## Remaining release decisions

The [portable release observations](../scripts/profiling/baselines/v0.8.4/apple-m1-pro/native-release-observations.json) record 12 matched catalog workloads, enabled HDR/ColorField workloads and three reversed confirmations using the exact published 0.8.3 and signed draft 0.8.4 Mac apps. Every latency workload is comparable and uncontended. Memory uses three separate alternating-order pairs for HDR, ColorField, multi-photo and native-detail work. Ordinary run variance is acceptable.

Draft and HDR tail increases do not repeat in reverse order. The eight-photo exposure-edit median rises approximately 12–14 ms in both orders, about 6%, with essentially stable p95. The difference persists after excluding the first slow edit. This is settled delivery to the whole group, not per-photo or shader time. No broad responsiveness regression or proven noise is established. The narrow repeated difference requires owner review without an invented budget.

Median sampled physical-footprint peak changes are HDR -0.38%, ColorField -7.06%, multi-photo -0.30% and native-detail +0.35%. No growth pattern is observed. These samples do not establish guaranteed peaks, settlement, measured VRAM or a causal optimization. All initial and reversed observations remain recorded.

The historical hosted Windows parallel native access violation remains unresolved in the [regression plan](./gui-release-regressions.md). Serial passes do not establish a fix. Full Windows GUI/performance comparisons remain unavailable with the current portable runner's POSIX monitoring requirements. Native Windows testing is Windows 11. Windows 10 and native Sequoia graphics coverage are unavailable on the prepared owner hosts. Hosted Sequoia checks establish signed binary loading only. Prior release-specific waivers do not carry forward.

Catalog job success does not make every user path covered. The inventory retains native file-dialog, some preferences/restart, clipboard and recorder coverage gaps. Browser-quarantine acceptance has not been automated. These limits and performance observations need the owner's release-specific publication decision.
