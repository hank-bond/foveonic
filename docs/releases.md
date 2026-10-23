---
summary: "Configure Apple credentials, review versioned changes, and manually publish the signed, notarized macOS app from its draft GitHub Release."
kind: how-to
---

# Prepare an application release

[Requirements](specs/releases.md) define the signed Apple Silicon release scope. The [signed-release plan](../plans/signed-releases.md) tracks validation. Release CI signs and notarizes the app. Ordinary local builds need no Apple credentials.

## Configure Apple credentials

An active paid Apple Developer Program membership is required. In Keychain Access, create a certificate signing request with your email and a recognizable Common Name. In [Apple Developer Certificates](https://developer.apple.com/account/resources/certificates/list), issue a **Developer ID Application** certificate using **G2 Sub-CA** and that request. Import the downloaded `.cer` into the same Mac's login keychain. Under **My Certificates**, confirm its matching private key appears beneath it, then export the identity as a password-protected `.p12`. A `.cer` alone does not contain the private key. Keep the export and its password securely backed up outside Git.

In the GitHub repository's **Settings → Secrets and variables → Actions**, configure these repository secrets:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE_P12_BASE64` | Base64-encoded `.p12` containing the certificate and private key. |
| `APPLE_CERTIFICATE_PASSWORD` | The `.p12` export password. |
| `APPLE_ID` | The Apple Account email with access to the developer team. |
| `APPLE_APP_SPECIFIC_PASSWORD` | An app-specific password created for notarization at [account.apple.com](https://account.apple.com/), not the account's normal password. |

Copy the encoded export directly to the clipboard on macOS, then paste it into the secret field:

```sh
base64 -i "/path/to/DeveloperID.p12" | pbcopy
```

Under the **Variables** tab, add `APPLE_TEAM_ID` using the ten-character Team ID from Apple Developer membership details. The export must contain exactly one valid Developer ID Application identity for that team.

CI imports the identity into a temporary keychain and removes it and temporary certificate/submission files even on failure or cancellation. It does not change the runner's default keychain or search list. Keep credentials out of source files and release assets. Replace the secrets when renewing the certificate or rotating the app-specific password.

## Review locally

1. Set the application version in `[workspace.package]` in `Cargo.toml`. Both application crates inherit it. The examples below use `0.1.1`.
2. Review the matching `## 0.1.1` section in [CHANGELOG.md](../CHANGELOG.md). Replace its introductory placeholder with your own prose, or add the introduction to the GitHub draft later. Keep the change list below it.
3. Run the [workspace checks](../README.md#checks), then validate the notes and build the bundle:

   ```sh
   cargo xtask release-notes --tag v0.1.1
   cargo xtask bundle --tag v0.1.1
   ```

   Packaging does not install or launch the app. `cargo xtask run -- path/to/photo.x3f` builds and launches it for interactive review. Both build commands accept `--offline` after dependencies are cached. Local debug builds use `--debug` instead of `--tag`.

4. Review and commit the changes before creating a tag. Push the reviewed commit to GitHub.

## Build the draft

When ready to start CI, create and push the matching tag:

```sh
git tag -a v0.1.1 -m "Foveonic v0.1.1"
git push origin v0.1.1
```

The [release workflow](../.github/workflows/release.yml) runs checks on an Apple Silicon `macos-26` runner and builds with Xcode. It signs the app with Developer ID, hardened runtime and a secure timestamp, then submits a temporary ZIP to Apple's notary service. After acceptance, it staples and validates the ticket, verifies the signature and checks Gatekeeper acceptance. Only then does it create the final ZIP and checksum and upload them to a draft GitHub Release:

- `Foveonic-v0.1.1-macos-arm64.zip`, containing `Foveonic.app`.
- `SHA256SUMS`, for verifying the downloaded archive with `shasum -a 256 -c SHA256SUMS`.

Signing, notarization or verification failures stop uploads, without an unsigned fallback. Notarization waits up to 20 minutes. A timeout fails the build even if Apple continues processing the submission. Review the Actions log for Apple's submission ID and status before deciding whether to rerun.

A new draft receives only the tagged changelog section. An existing draft keeps its title and notes, including your prose. Rerunning the workflow replaces its build assets. An already published release rejects uploads. Tags unrelated to application versions, such as `example-photographs`, do not trigger this workflow.

## Publish manually

Review the draft notes and download the archive through a browser so macOS applies its normal download checks. Extract and test the app on macOS 26 or later before publishing. Confirm it opens without an unidentified-developer or malware-verification block. A normal first-launch confirmation for an internet download is expected. If verification fails, investigate rather than bypassing Gatekeeper to approve the release.

Previously published unsigned releases remain unchanged. The first signed hosted build and downloaded-app review are still pending as recorded in the [plan](../plans/signed-releases.md).

Add your introduction above the changelog in the draft if it is not already present. Publish only after review. CI never publishes the release automatically.

## Manage Latest

Version tags such as `v0.1.1` identify fixed release commits. GitHub's **Latest** designation is separate from Git tags. Drafts do not replace the published Latest release, and no moving Git tag named `latest` is needed.

After reviewing the draft and app, publishing with an explicit Latest selection is available in GitHub's release editor or through:

```sh
gh release edit v0.1.1 --draft=false --latest
```

Run this only with owner publication approval. The stable [latest application release link](https://github.com/hank-bond/foveonic/releases/latest) follows that designation. Photograph collections are not application releases and should not be marked Latest.
