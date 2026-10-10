---
summary: "Establish the shortest Merrill X3F conversion path through the shader graph to DNG and TIFF."
read_when: "Read when beginning or resuming the first end-to-end conversion."
---

# Establish end-to-end conversion

Requirements: [`processing-graphs.md`](../docs/specs/processing-graphs.md) and [`output-strategies.md`](../docs/specs/output-strategies.md).

## Steps

- [ ] Inspect reference decoding, calibration, color, and output paths. Review the minimum required stages and first export milestone with the owner.
- [ ] Connect Merrill decoding and metadata access to the minimum internal graph.
- [ ] Implement the selected image-processing stages in shaders and produce an inspectable image.
- [ ] Connect the distinct TIFF rendering and compressed DNG packaging paths, resolving format and profile choices with the owner as needed.
- [ ] Review a real photograph end to end, including DNG editing in Lightroom and Capture One. Record remaining gaps.

## Progress and findings

The repository has documentation tooling and no image processor. Detail reconstruction, denoising, and highlight reconstruction can be omitted from this first slice. Reference stages are candidates to assess for necessity, rather than a required porting checklist.
