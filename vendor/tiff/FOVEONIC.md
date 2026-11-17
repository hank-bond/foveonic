---
summary: "A pinned TIFF encoder patch aligns directories and external tag values."
kind: reference
---

# TIFF encoder alignment patch

Imported from the published image-rs TIFF 0.10.3 crate, upstream revision `695208cffe1ba8623fe4576584bcc87e13b63b65`, by the image-rs developers. The registry package SHA-256 is `af9605de7fee8d9551863fd692cce7637f548dbd9db9180fcc07ccc6d26c336f`. Upstream is [image-tiff](https://github.com/image-rs/image-tiff/tree/695208cffe1ba8623fe4576584bcc87e13b63b65), licensed under MIT. Preserve the upstream LICENSE and notices. Upstream CI files, changelog and package lockfile are omitted.

Two local calls to the existing `pad_word_boundary` helper align directory starts and out-of-line tag values before their offsets are recorded. The helper aligns to four bytes, which also satisfies classic TIFF's two-byte word boundary. A compressed strip may end on an odd byte. Without this padding, a later directory or tag value can begin at an invalid offset. Adobe's SDK independently reported the odd-directory defect in real exports.

The patch preserves encoded strip bytes, byte counts, compression, prediction and decoded samples. Only TIFF 0.10.3 is patched. The TIFF 0.11.3 decoder used by the image dependency remains upstream. Processor output tests exercise odd payload endings, directory links and decoded samples. Remove this local patch when an upstream release supplies the same guarantees and its outputs pass the maintained reader checks.
