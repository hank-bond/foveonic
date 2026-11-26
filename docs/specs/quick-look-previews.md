---
summary: "macOS Quick Look shows embedded X3F thumbnails in Finder and renders space-bar previews with the shared engine at a reduced working size."
kind: reference
status: accepted
implementation: incomplete
---

# Quick Look previews

Finder icons, Spotlight results and the space-bar preview use macOS Quick Look. Foveonic **must** supply X3F thumbnails and previews through Quick Look app extensions embedded in the macOS app. Windows Explorer previews are outside this scope.

## Thumbnails

The thumbnail extension **must** return the photograph's embedded camera preview without decoding sensor data. It **must** follow the [browser-preview](./browser-previews.md) rules for embedded color declarations and framing. A camera JPEG with different framing from the Raw **must not** be used. Thumbnails **must** be upright, following the orientation recorded in the file. The extension uses the smallest embedded image whose long edge reaches the requested size, and otherwise the largest correctly framed one. Requests larger than the embedded preview receive that preview's own size.

## Space-bar previews

The preview extension **must** render the photograph through the shared engine. It **must** use the camera's default recipe only, ignoring saved recipes beside the photograph, with the [Speed processing quality](./processing-quality.md) and a reduced [power-of-two working scale](./small-previews.md).

The working scale **must** be the largest power-of-two divisor whose oriented long edge remains at or above a named 2048-pixel minimum. Photographs that cannot be reduced without falling below the minimum render at native size. For Merrill this selects half size for High files (2352 pixels) and native size for Medium and Low files. The rule does not depend on display size. The minimum is a named constant.

The extension **should** hand rendered pixels to Quick Look directly rather than encoding an intermediate image file. If rendering fails, the extension **should** show the embedded camera preview, upright and correctly framed as for thumbnails, instead of an error. A Quick Look reply is one image, so a successful preview always shows the render.

## Packaging

Both extensions **must** be built and embedded by the app packaging task and signed with the app-sandbox entitlement, which Quick Look requires. Release builds sign each extension with the app's Developer ID before the app itself, keeping the sandbox entitlement, so notarization covers them. The app **must** import the existing `com.sigmaphoto.photopro-x3f` type for the `.x3f` extension so Quick Look can route files without another Sigma application installed. Quick Look starts a new extension process for each preview, so engine startup is part of every preview's cost.
