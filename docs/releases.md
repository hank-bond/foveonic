---
summary: "Configure Developer ID signing, review versioned changes, and manually publish the unnotarized macOS app from its draft GitHub Release."
kind: how-to
---

# Prepare an application release

[Requirements](specs/releases.md) define the signed Apple Silicon release scope. The [signed-release plan](../plans/archived/signed-releases.md) tracks validation. Release CI signs the app but does not submit it for notarization. Ordinary local builds need no Apple credentials.

## Configure Apple credentials

An active paid Apple Developer Program membership is required. In Keychain Access, create a certificate signing request with your email and a recognizable Common Name. In [Apple Developer Certificates](https://developer.apple.com/account/resources/certificates/list), issue a **Developer ID Application** certificate using **G2 Sub-CA** and that request. Import the downloaded `.cer` into the same Mac's login keychain. Under **My Certificates**, confirm its matching private key appears beneath it, then export the identity as a password-protected `.p12`. A `.cer` alone does not contain the private key. Keep the export and its password securely backed up outside Git.

In the GitHub repository's **Settings → Secrets and variables → Actions**, configure these repository secrets:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE_P12_BASE64` | Base64-encoded `.p12` containing the certificate and private key. |
| `APPLE_CERTIFICATE_PASSWORD` | The `.p12` export password. |

Copy the encoded export directly to the clipboard on macOS, then paste it into the secret field:

```sh
base64 -i "/path/to/DeveloperID.p12" | pbcopy
```

Under the **Variables** tab, add `APPLE_TEAM_ID` using the ten-character Team ID from Apple Developer membership details. The export must contain exactly one valid Developer ID Application identity for that team.

CI imports the identity into a temporary keychain and removes it and temporary certificate files even on failure or cancellation. It temporarily adds the signing keychain to the runner's search list, preserves the original entries and restores them during cleanup. The default keychain stays unchanged. Keep credentials out of source files and release assets. Replace the secrets when renewing the certificate or changing its export password. Apple Account and app-specific-password secrets are not used.

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

The [release workflow](../.github/workflows/release.yml) runs checks on an Apple Silicon `macos-26` runner and builds with Xcode. It signs the app with Developer ID, hardened runtime and a secure timestamp, then verifies the signature before creating the ZIP and checksum and uploading them to a draft GitHub Release:

- `Foveonic-v0.3.0-macos-arm64.zip`, containing `Foveonic.app`.
- `SHA256SUMS`, for verifying the downloaded archive with `shasum -a 256 -c SHA256SUMS`.

Signing or signature-verification failures stop uploads, without an unsigned fallback. There is no notarization submission, ticket stapling or Gatekeeper-acceptance gate.

A new draft receives only the tagged changelog section. An existing draft keeps its title and notes, including your prose. Rerunning the workflow replaces its build assets. An already published release rejects uploads. Tags unrelated to application versions, such as `example-photographs`, do not trigger this workflow.

## Publish manually

Review the draft notes and download the archive through a browser so macOS applies its normal download checks. Releases target macOS 15 Sequoia or later. Sequoia runtime testing remains pending user feedback. Verify the executable and bundle both target 15.0, and test the app on available supported systems before publishing. Downloads are Developer ID signed but not notarized, so Gatekeeper may block the first launch.

For a build you trust, attempt to open the app, then use **System Settings → Privacy & Security → Open Anyway** for that app and confirm the prompt. Subsequent launches normally retain that approval. A newly downloaded version may be checked again. Do not disable Gatekeeper globally.

Previously published unsigned releases remain unchanged. The first signing-only release, [v0.2.0](https://github.com/hank-bond/foveonic/releases/tag/v0.2.0), passed hosted checks, checksum/signature verification and an isolated native GUI smoke test before owner-approved publication. The [plan](../plans/archived/signed-releases.md) records the evidence and testing limits.

Add your introduction above the changelog in the draft if it is not already present. Publish only after review. CI never publishes the release automatically.

## Manage Latest

Version tags such as `v0.3.0` identify fixed release commits. GitHub's **Latest** designation is separate from Git tags. Drafts do not replace the published Latest release, and no moving Git tag named `latest` is needed.

After reviewing the draft and app, publishing with an explicit Latest selection is available in GitHub's release editor or through:

```sh
gh release edit v0.3.0 --draft=false --latest
```

Run this only with owner publication approval. The stable [latest application release link](https://github.com/hank-bond/foveonic/releases/latest) follows that designation. Photograph collections are not application releases and should not be marked Latest.
