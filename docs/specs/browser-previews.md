---
summary: "Small embedded thumbnails appear immediately and asynchronous cached previews follow the latest photographic render."
kind: reference
status: accepted
implementation: conforming
---

# Browser previews

- The browser **must** display a prepared local thumbnail for a ready cache entry. Uncached files use the small embedded preview without waiting for sensor decoding or a full-size JPEG decode.
- Background work **should** prepare a sharper preview from the largest embedded JPEG. The viewport **should** use this while the photographic render is pending.
- Embedded previews **must** honor their declared RGB color space across Classic, Merrill, Quattro and Quattro H. Prefer a valid embedded RGB ICC profile, then JPEG Exif color tags, then the camera's `COLORSPACE` metadata. Unlabelled Exif thumbnails inherit their enclosing JPEG. Display previews **must** retain their declared gamut and precision until the shared [display conversion](./display-color-management.md). Already-rendered previews **must not** receive camera-space conversion again. Unknown declarations may fall back to sRGB.
- A completed whole-image photographic render **must** refresh the browser thumbnail and larger preview. Inspection outputs, comparison inputs, differences, and detail regions **must not** replace photographic previews.
- Preview preparation, readback, resizing, and disk writes **must** run asynchronously, outside the main render and UI threads. Work **must** be bounded and yield priority to active rendering.
- Disk previews **must** use the decoded cache's source identity, temporary directory, policy, and shared byte budget. Edited rendered previews **must** be invalidated when their recipe changes. Late embedded work **must not** overwrite a newer rendered preview.
- The application **must not** retain full-size decoded JPEGs for the whole directory. Larger in-memory previews **must** remain bounded.
- A camera JPEG with different framing from the Raw **must not** supply the larger preview. The correctly framed small thumbnail remains until a photographic render supplies the missing pixels.

## Implementation

Previews fit within 2048 pixels on the longest side. Embedded display decoding retains known camera primaries in RGB16. Valid optional ICC profiles normalize to wide ROMM RGB16. Resizing averages linear-light values before primary conversion. Native RGBA16Float preview textures retain extended linear sRGB for interpolation, with the same final transfer and host color matching as the viewport. Preview-format versions reject older sRGB-only companions. The decoded RAW format is unchanged. The selection-neighborhood prefetch worker extracts thumbnails and suitable larger camera previews from the same original buffer used for RAW decoding. It publishes complete local readiness after those assets and metadata are stored. Ready selections do not reopen the original or sidecar to display previews. Background identity and recipe refresh invalidates outdated entries. A separate replaceable GPU presentation job samples complete photographic output after foreground work yields. It reads back only the small linear image, encodes a monitor-independent ROMM RGB16 preview, refreshes a 256-pixel thumbnail, and queues a lossless Deflate TIFF companion for the existing disk-cache actor. The companion header stores its declared primaries and geometry. Two larger previews and 256 thumbnails remain in memory. Small cached-preview reads stay available at background priority while rendering runs.

Recipe fingerprints use decoded canonical editing settings, so moving a recipe into the visible subfolder does not invalidate an unchanged rendered preview. Publication rechecks source identity and recipe. Cache checksum, recipe, policy, and size failures fall back to the embedded thumbnail. The DP0 cropped large JPEG is skipped. Its correctly framed small preview remains available locally until a render supplies a larger one.
