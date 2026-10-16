---
summary: "Review versioned changes, push a release tag, and manually publish the unsigned macOS app from its draft GitHub Release."
kind: how-to
---

# Prepare an application release

[Requirements](specs/releases.md) define the unsigned Apple Silicon release scope. The [preparation plan](../plans/cargo-releases.md) records local validation and remaining review.

## Review locally

1. Set the application version in `[workspace.package]` in `Cargo.toml`. Both application crates inherit it. The initial version is `0.1.0`.
2. Review the matching `## 0.1.0` section in [CHANGELOG.md](../CHANGELOG.md). Replace its introductory placeholder with your own prose, or add the introduction to the GitHub draft later. Keep the change list below it.
3. Run the [workspace checks](../README.md#checks), then validate the notes and build the bundle:

   ```sh
   cargo xtask release-notes --tag v0.1.0
   cargo xtask bundle --tag v0.1.0
   ```

   Packaging does not install or launch the app. `cargo xtask run -- path/to/photo.x3f` builds and launches it for interactive review. Both build commands accept `--offline` after dependencies are cached. Local debug builds use `--debug` instead of `--tag`.

4. Review and commit the changes before creating a tag. Push the reviewed commit to GitHub.

## Build the draft

When ready to start CI, create and push the matching tag:

```sh
git tag -a v0.1.0 -m "Foveonic v0.1.0"
git push origin v0.1.0
```

The [release workflow](../.github/workflows/release.yml) runs checks on an Apple Silicon `macos-26` runner, builds with Xcode, and creates a draft GitHub Release containing:

- `Foveonic-v0.1.0-macos-arm64.zip`, containing `Foveonic.app`.
- `SHA256SUMS`, for verifying the downloaded archive with `shasum -a 256 -c SHA256SUMS`.

A new draft receives only the tagged changelog section. An existing draft keeps its title and notes, including your prose. Rerunning the workflow replaces its build assets. An already published release rejects uploads. Tags unrelated to application versions, such as `example-photographs`, do not trigger this workflow.

## Publish manually

Review the draft notes and download the archive. Extract and test the app on macOS 26 or later before publishing. The first release is Apple Silicon only and is not Developer ID signed or notarized. macOS Gatekeeper may block a downloaded app. For a build you trust, use the per-app **Open Anyway** control in **System Settings → Privacy & Security** rather than disabling Gatekeeper globally.

Add your introduction above the changelog in the draft if it is not already present. Publish only after review. CI never publishes the release automatically.
