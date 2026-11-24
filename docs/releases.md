---
summary: "Configure signing, review Windows and macOS archives, and manually publish a draft GitHub Release."
kind: how-to
---

# Prepare an application release

[Requirements](specs/releases.md) define the signed Apple Silicon release scope. The [signed-release plan](../plans/archived/signed-releases.md) tracks validation. Release CI signs the app, submits it to Apple for notarization and staples the ticket. Ordinary local builds need no Apple credentials.

## Configure Apple credentials

An active paid Apple Developer Program membership is required. In Keychain Access, create a certificate signing request with your email and a recognizable Common Name. In [Apple Developer Certificates](https://developer.apple.com/account/resources/certificates/list), issue a **Developer ID Application** certificate using **G2 Sub-CA** and that request. Import the downloaded `.cer` into the same Mac's login keychain. Under **My Certificates**, confirm its matching private key appears beneath it, then export the identity as a password-protected `.p12`. A `.cer` alone does not contain the private key. Keep the export and its password securely backed up outside Git.

In the GitHub repository's **Settings → Secrets and variables → Actions**, configure these repository secrets:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE_P12_BASE64` | Base64-encoded `.p12` containing the certificate and private key. |
| `APPLE_CERTIFICATE_PASSWORD` | The `.p12` export password. |
| `APPLE_ID` | The Apple Account email used for notarization. |
| `APPLE_APP_SPECIFIC_PASSWORD` | An [app-specific password](https://support.apple.com/102654) for that account. |

Copy the encoded export directly to the clipboard on macOS, then paste it into the secret field:

```sh
base64 -i "/path/to/DeveloperID.p12" | pbcopy
```

Under the **Variables** tab, add `APPLE_TEAM_ID` using the ten-character Team ID from Apple Developer membership details. The export must contain exactly one valid Developer ID Application identity for that team.

CI imports the identity into a temporary keychain and removes it and temporary certificate files even on failure or cancellation. It temporarily adds the signing keychain to the runner's search list, preserves the original entries and restores them during cleanup. The default keychain stays unchanged. Keep credentials out of source files and release assets. Replace the secrets when renewing the certificate or changing its export password. Replace the app-specific password if it is revoked; a rejected credential fails the notarization step.

## Review locally

1. Set the application version in `[workspace.package]` in `Cargo.toml`. Both application crates inherit it. The examples below use `0.3.0`.
2. Review the matching `## 0.3.0` section in [CHANGELOG.md](../CHANGELOG.md). Replace its introductory placeholder with your own prose, or add the introduction to the GitHub draft later. Keep the change list below it.
3. Run the [workspace checks](contributing.md#run-checks), then validate the notes and build the bundle:

   ```sh
   cargo xtask release-notes --tag v0.3.0
   cargo xtask bundle --tag v0.3.0
   ```

   Packaging does not install or launch the app. `cargo xtask run -- path/to/photo.x3f` builds and launches it for interactive review. Both build commands accept `--offline` after dependencies are cached. Local debug builds use `--debug` instead of `--tag`.

4. Review and commit the changes before creating a tag. Push the reviewed commit to GitHub.

## Build the draft

When ready to start CI, create and push the matching tag:

```sh
git tag -a v0.3.0 -m "Foveonic v0.3.0"
git push origin v0.3.0
```

The [release workflow](../.github/workflows/release.yml) runs checks on an Apple Silicon `macos-26` runner and builds with Xcode. It also builds Windows x64 on a Windows runner using a pinned DirectX shader compiler. It signs the app with Developer ID, hardened runtime and a secure timestamp, then verifies the signature before creating the ZIP and checksum and uploading them to a draft GitHub Release:

- `Foveonic-v0.3.0-macos-arm64.zip`, containing `Foveonic.app`.
- `Foveonic-v0.3.0-windows-x64.zip`, containing the unsigned app and its DirectX compiler DLLs and license notices.
- `SHA256SUMS`, containing the two application archive hashes. Verify each archive using its corresponding checksum.

Signing or signature-verification failures stop uploads, without an unsigned fallback. After signing, CI submits the app to Apple's notary service, waits up to 30 minutes, staples the ticket and requires Gatekeeper acceptance. A rejected or timed-out submission stops the upload.

Matching dSYM and PDB archives remain developer CI artifacts. Public application downloads exclude these archives and the Windows PDB. Keep each symbol archive with the exact application build it describes, and preserve a developer copy before CI retention expires. The macOS packaging step compares the executable and dSYM UUIDs. The Windows symbol archive records hashes of the executable and PDB. Verify these identities before publication. User support reports retain structured errors and best-effort stacks, but production packages do not guarantee resolved function names or source lines. Native crash investigation may require a crash dump and matching retained symbols.

When replacing an unpublished draft that contains symbol downloads, validate the rebuilt application archives and retain matching developer symbols before deleting the standalone symbol assets. Regenerate the public checksums for the application archives only. Published historical releases remain unchanged.

The Windows archive is unsigned and must be tested on Windows before publication. The Windows build runs independently, and its draft upload waits for the macOS draft. A new draft receives only the tagged changelog section. An existing draft keeps its title and notes, including your prose. Rerunning the workflow replaces its build assets. An already published release rejects uploads. Tags unrelated to application versions, such as `example-photographs`, do not trigger this workflow.

Start independent host lanes against one frozen candidate as soon as their inputs and builds are ready. Full local correctness can run before CI packages arrive. Preserve the local source identity, executable hashes, toolchain and build flags with that evidence. A quiet Mac can run the required Rust checks while another Mac runs native correctness, and Windows checks can run alongside both. On each host, finish compilation and correctness before its exclusive performance measurements. Keep host reports separate when their contracts or executable hashes differ.

On two quiet Macs, one useful partition gives the 32 GiB host the slider matrix, then its exact packaged smoke and exclusive performance work. The other host can finish required Rust checks and run the remaining native jobs, including the numerical matrix. Use the [existing job selection and exact-union report merger](testing-coverage.md) with identical frozen tools and executable hashes. Performance may begin after its own host is clear while the other host continues correctness. Join all required results before publication. This partition needs measurement on the prepared hosts before claiming a total release duration.

The final evidence combines these local candidate checks with native startup/rendering smoke and matched latency/memory checks of the exact packaged executables. Verify their source revision, build configuration and package hashes. Do not describe a local test executable as the signed package or relabel its results with the package hash. A source or relevant build-configuration change invalidates affected evidence and needs the corresponding checks again.

To overlap local checks with CI validation, download the early Actions artifact for that exact workflow run and commit: `Foveonic-macos-arm64-COMMIT` or `Foveonic-windows-x64-COMMIT`. Verify its package identity before running native checks. These artifacts appear before the remaining tests finish and are unapproved candidates. A later CI failure still blocks the release. Keep Mac and Windows host lanes independent, and begin each package's smoke checks as soon as that package is available. Run performance comparisons exclusively on their host after its builds and correctness work stop. Reconcile both host reports, all CI checks and the final draft executable hashes before publication.

The workflows cache compatible dependency compilation, preferring matching compiler/native tools, runner image, target flags and dependency manifests. A final [restore prefix](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching) permits an older dependency manifest only within the same toolchain fingerprint. Cargo still [validates dependency fingerprints](https://doc.rust-lang.org/stable/nightly-rustc/cargo/compiler/fingerprint/index.html) and runs the build and checks. Workspace products and assembled packages are rebuilt. Both observed Cargo output layouts are excluded from the workspace cache. Cache effectiveness is unmeasured until a hosted run exercises it. GitHub cache visibility is scoped by ref. A tag can restore an eligible default-branch cache, but one release tag cannot warm another tag directly. The manual build workflows on `main` can seed compatible caches without adding test jobs to every push.

## Compare performance before publication

Every release needs a matched latency and memory comparison against the previous published app before publication, in addition to correctness checks. Use the exact packaged executables on the same quiet test host, with the same pinned photographs, fixed recipes, display geometry, processing policies and cache budgets. Record executable hashes and actual delivered resolution. A workload label such as Full or Fast does not establish the working grid. Keep enabled ColorField, nonneutral HDR, native-detail and representative multi-photo work in the declared coverage, alongside camera defaults.

Record a render run from the previous release tag and from the candidate on the same host, and compare them. The comparison lists every case whose pixels changed, with the size of the change, and every timing and peak-memory change. Review it against the changelog, which says what was meant to change, and upload `comparison.json` and the candidate's run report with the release. Follow [render runs](./profiling.md#render-runs-from-any-revision).

```sh
python3 scripts/profiling/pipeline_benchmark.py record --revision PREVIOUS_TAG \
  --cases RENDER_CASES.json --directory .local/runs/before
python3 scripts/profiling/pipeline_benchmark.py record --revision CANDIDATE_COMMIT \
  --cases RENDER_CASES.json --directory .local/runs/after
python3 scripts/profiling/pipeline_benchmark.py compare .local/runs/before .local/runs/after \
  --output .local/runs/comparison.json
```

The correctness command `acceptance.py --full` deliberately excludes latency jobs. Run the existing latency domain separately for both versions:

```sh
python3 scripts/profiling/acceptance.py --domain latency \
  --binary PATH_TO_PREVIOUS_APP --cases CASES.json \
  --directory .local/release-performance-before \
  --record-latency-baseline --latency-trials 3
python3 scripts/profiling/acceptance.py --domain latency \
  --binary PATH_TO_CANDIDATE_APP --cases CASES.json \
  --directory .local/release-performance-after \
  --latency-before .local/release-performance-before --latency-trials 3
```

Use fresh output directories and enough repetitions to assess variability. The existing comparator requires complete behavior, checked uncontended timing and matching workload identities. Review per-action median and p95 changes, first-photo and complete-group delivery, and UI-update gaps. Three trials are a minimum, not evidence of statistical significance. These scenarios begin after the app and one active photograph have loaded. They do not measure cold process startup. Repeated exposure edits may reuse upstream recovery, so they do not establish ColorField or denoiser recomputation costs. Add the maintained [fresh-processing, checkpoint and native-detail measurements](profiling.md) needed by the declared coverage. The latency catalog alone does not cover every enabled feature or native-detail workflow.

Differences consistent with ordinary run-to-run variation are not regressions. Investigate meaningful, repeatable slowdowns or memory growth. Repeat suspicious comparisons in reversed order and inspect the raw trials before attributing them to the release. A small isolated p95 change does not require a fix merely because its percentage is nonzero.

Run memory measurements separately from timing, using the same recipes and workflows. Preserve sampled process footprint and settled usage, the sampling interval, stage boundaries and any separately available GPU-memory observations. On macOS, use physical footprint rather than treating RSS or graph-owned bytes as total application memory. Keep loading peaks, retained multi-photo state and post-edit settlement distinct. The [profiling guide](profiling.md#resource-overlap) describes the existing resource measurements. A memory guard limit is an operational stop threshold, not an approved regression budget.

Use matched Wall/Perfetto captures to explain changes after ordinary Off comparisons. Trace durations, screenshots, image verification and reused correctness sessions must not substitute for uninstrumented latency measurements. Keep compact versioned reports and raw timing/memory records. Bound diagnostic captures and discard disposable successful image captures after their checks complete rather than accumulating every run indefinitely.

Do not publish with missing comparisons or unexplained regressions without an explicit release-specific owner decision. Record unavailable platforms, failed workloads, unmatched settings and known measurement gaps. A prior app that cannot start does not provide a valid native baseline. Do not modify it silently or weaken workload matching to obtain a comparison. Without an approved regression budget, results are observations that require review, not an automatic performance pass.

## Publish manually

Run all maintained correctness checks in the [test inventory](testing-coverage.md), including native GUI acceptance on available supported platforms. List unavailable inputs and unperformed checks for owner review. The hosted workflow's workspace tests include GUI unit tests, while its packaged `--help` checks only load the executable. The 0.8.1 Windows main-thread stack overflow escaped those checks because `--help` returns before graphics and the application handoff. Native smoke on an interactive Windows desktop reproduces the failure in the original archive and passes after heap-owning the prepared application.

Before publishing, download and extract the exact draft archives, verify `SHA256SUMS`, and run the [packaged GUI verifier](../scripts/verify_gui_release.py) on both macOS and an interactive Windows desktop:

```sh
python3 scripts/verify_gui_release.py --binary path/to/Foveonic.app/Contents/MacOS/foveonic-gui --directory .local/release-smoke-macos
```

On Windows, use `--binary path/to/foveonic-gui.exe` from a desktop terminal or a task running with the logged-on user's interactive token. A noninteractive SSH session may fail surface creation even for working releases. Supply `--source path/to/blue-parking-sign.x3f` if the pinned [example photograph](testing-coverage.md) is elsewhere. The verifier copies it into an owned folder, isolates smoke settings, requires successful exit and all ten workspace checkpoints, and saves stdout, stderr, captures and `result.json`. It rejects an existing output directory. Confirm the report's executable hash matches the extracted release executable. Do not enlarge the application's stack for this check. This startup/rendering smoke supplements the complete acceptance suites and does not establish native file-dialog or every editing workflow's coverage.

Review the draft notes and download the archive through a browser so macOS applies its normal download checks. Releases target macOS 15 Sequoia or later. Sequoia runtime testing remains pending user feedback. Verify the executable and bundle both target 15.0, and test the app on available supported systems before publishing. Downloads are Developer ID signed and notarized, so the first launch shows only the ordinary confirmation for a downloaded app.

Releases before 0.9.0 are not notarized and need **System Settings → Privacy & Security → Open Anyway** on first launch. Do not disable Gatekeeper globally.

Previously published unsigned releases remain unchanged. The first signing-only release, [v0.2.0](https://github.com/hank-bond/foveonic/releases/tag/v0.2.0), passed hosted checks, checksum/signature verification and an isolated native GUI smoke test before owner-approved publication. The [plan](../plans/archived/signed-releases.md) records the evidence and testing limits.

Add your introduction above the changelog in the draft if it is not already present. Publish only after review. CI never publishes the release automatically.

Keep release notes focused on changes in that version. Platform support, signing and first-launch guidance live in this guide and need not be repeated in each release. Mention them in version notes only when that behavior changes.

## Manage Latest

Version tags such as `v0.3.0` identify fixed release commits. GitHub's **Latest** designation is separate from Git tags. Drafts do not replace the published Latest release, and no moving Git tag named `latest` is needed.

After reviewing the draft and app, publishing with an explicit Latest selection is available in GitHub's release editor or through:

```sh
gh release edit v0.3.0 --draft=false --latest
```

Run this only with owner publication approval. The stable [latest application release link](https://github.com/hank-bond/foveonic/releases/latest) follows that designation. Photograph collections are not application releases and should not be marked Latest.
