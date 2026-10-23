---
summary: "Small embedded thumbnails appear immediately and asynchronous cached previews follow the latest photographic render."
kind: reference
status: accepted
implementation: conforming
---

# Browser previews

- The browser **must** display the small embedded preview without waiting for sensor decoding or a full-size JPEG decode.
- Background work **should** prepare a sharper preview from the largest embedded JPEG. The viewport **should** use this while the photographic render is pending.
- A completed whole-image photographic render **must** refresh the browser thumbnail and larger preview. Inspection outputs, comparison inputs, differences, and detail regions **must not** replace photographic previews.
- Preview preparation, readback, resizing, and disk writes **must** run asynchronously, outside the main render and UI threads. Work **must** be bounded and yield priority to active rendering.
- Disk previews **must** use the decoded cache's source identity, temporary directory, policy, and shared byte budget. Edited rendered previews **must** be invalidated when their recipe changes. Late embedded work **must not** overwrite a newer rendered preview.
- The application **must not** retain full-size decoded JPEGs for the whole directory. Larger in-memory previews **must** remain bounded.
- A camera JPEG with different framing from the Raw **must not** supply the larger preview. The correctly framed small thumbnail remains until a photographic render supplies the missing pixels.

## Implementation

Previews fit within 2048 pixels on the longest side. The folder prefetch worker creates camera previews before decoding each eligible source. A separate replaceable GPU presentation job samples complete photographic output after foreground work yields. It reads back only the small display image, refreshes a 256-pixel thumbnail, and queues a JPEG companion for the existing disk-cache actor. Two larger previews and 256 thumbnails remain in memory. Small cached-preview reads stay available at background priority while rendering runs.

Recipe fingerprints use decoded canonical editing settings, so moving a recipe into the visible subfolder does not invalidate an unchanged rendered preview. Publication rechecks source identity and recipe. Cache checksum, recipe, policy, and size failures fall back to the embedded thumbnail. The DP0 cropped large JPEG is skipped.
