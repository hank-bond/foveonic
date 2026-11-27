---
summary: "Reduce full sensor decode latency for Merrill and Quattro using measured bit-reader, packing, and shared-pool decoder changes."
---

# Reduce X3F plane decode latency

A Fit render from an uncached original still waits for the full sensor decode. Prioritize Merrill and Quattro. The [X3F input requirements](../docs/specs/x3f-input.md) cover original sample values, supported sizes and damage reporting. The [earlier decoder experiments](../docs/decoder-optimization-research.md) cover fused Huffman lookups and the cost of a separate SIMD reconstruction pass.

- [x] Compare a scalar row loop with separate even and odd predictors against the existing plane decoder.
- [x] Compare a rolling bit buffer with the existing cached-word reader.
- [x] Verify exact samples, damage reporting and cancellation on the measured decoder.
- [x] Hand over the first slice. The owner requested continued work on DP Quattro.
- [x] Measure the cost of packing the Quattro planes and compare a row-based assembly loop.
- [x] Build a separate macOS review app for the row-packing change.
- [x] Measure CAMF separately in the Foveonic and FlashRAW comparison. Check whether the fused Huffman lookup improves full type-5 CAMF decoding.
- [x] Measure a wider Huffman refill and an isolated parallel top-plane prototype. Keep full CAMF decoding.
- [x] Test full opens for split Quattro and Merrill planes with the shared processing pool.
- [x] Reproduce the mixed-priority wait and move phase joins onto the file caller.
- [x] Compare the final build with the preceding decoder and FlashRAW. Record peak memory, exactness checks, and owner test build.

## First measurements

FlashRAW's [X3F codec](https://github.com/ybouane/flashraw/blob/v0.5.0/crates/flashraw-codecs/src/x3f.rs) and [bit reader](https://github.com/ybouane/flashraw/blob/v0.5.0/crates/flashraw-core/src/bits.rs) informed the experiments. The paired row loop increased complete source-opening time by 1–2% on all five originals and was removed. The selected reader keeps the next prefix in the high bits of a rolling word and refills four bytes at a time. It tracks how many real bits remain so padded lookahead cannot make a truncated input valid.

These are local Apple M1 Pro medians for separate optimized binaries before and after the reader change. The baseline reader is from `925cebd`. Each file used four process runs in baseline, candidate, candidate, baseline order. Each run did three warmup opens and seven timed opens through `Source::open`, without a decoded cache. The [example](../scripts/profiling/test_inputs.json) and [external](../scripts/profiling/external_raw_samples.json) manifests pin the inputs. The sensor interval includes Quattro plane assembly. Complete opens include filesystem-cached reads and camera metadata.

| Original | Sensor stage before → after | Complete open before → after |
| --- | ---: | ---: |
| Merrill HI, blue parking sign | 103.71 → 86.79 ms | 123.14 → 105.72 ms |
| DP3 Merrill MED | 50.77 → 42.80 ms | 65.18 → 56.68 ms |
| DP3 Merrill LOW | 25.49 → 21.51 ms | 38.30 → 33.88 ms |
| DP0 Quattro | 233.81 → 205.71 ms | 274.19 → 242.50 ms |
| sd Quattro H | 260.86 → 230.63 ms | 314.61 → 280.95 ms |

## Validation

All decoded sample bits match the baseline across twelve originals spanning Merrill HI, MED and LOW, Quattro, Quattro H and Classic. Two damaged SD1 Merrill files retain their exact error details. The independent bit-reader and predictor checks cover padded tails, long payloads and cancellation. The original-file recording and cache round-trip tests pass, including CAMF and storage comparisons. Quarter-scale renders of the parking-sign Merrill and DP0 Quattro complete on Metal. Workspace tests, release profiler example tests, strict Clippy, formatting and documentation index checks pass. The independent review found no correctness issues. A separate macOS app bundle is ready for owner testing.

## Quattro row packing

The next slice packs fixed-size cells from contiguous rows, then applies sparse format-35 blue replacements. A separate five-open DP0 trace attributed 46.61 ms to packing before the change and 5.78 ms after it. The top plane took about 156 ms in both traces. CAMF still took about 30 ms. These are inclusive elapsed scopes.

The matched release runs used the rolling reader as their baseline and the same four-process alternating protocol. Warm filesystem reads are included.

| Original | Complete open before → after |
| --- | ---: |
| DP0 Quattro | 239.80 → 200.34 ms |
| DP2 Quattro | 213.05 → 173.78 ms |
| sd Quattro | 256.51 → 215.14 ms |
| sd Quattro H | 277.45 → 222.38 ms |

In a separate 11-pair run on the same DP0 file and M1 Pro, Foveonic took 194.82 ms for in-memory decode and FlashRAW 0.5.0 took 129.24 ms for in-memory open and raw-u16 output. The outputs differ in Quattro preparation and retained CAMF metadata. Foveonic's timings include the full CAMF decode and parsing used by its recipes.

All sample bits match across seven Quattro originals and one Merrill original. The DP0 serial-plane check and an odd-stride fixture exercise the packed placement. DP0's quarter-scale Metal render matches the preceding build's output digest and geometry. Independent review found no correctness issues.

## CAMF decoding and comparison

Type-5 CAMF uses the same Huffman differences as the sensor planes with an eight-bit wrapping predictor. Reuse the fused lookup. The first build moved the shared lookup out of line and increased sensor decoding time by about 10–13 ms. Keep it inlined at both call sites.

Report plane decoding, sensor preparation, CAMF handling, and in-memory total in separate columns. Foveonic's CAMF column sums its full `CAMF decompress` and `CAMF parse` timings. A local timing probe around [FlashRAW's CAMF lookup and extraction](https://github.com/ybouane/flashraw/blob/v0.5.0/crates/flashraw-formats/src/sigma/mod.rs#L430) records elapsed time and the number of bytes decoded, including read-ahead. FlashRAW decodes CAMF on demand for selected calibration. Its Quattro sensor preparation is the `Calibration` stage, which covers interleaving, autofocus repair, and lower-layer expansion. Foveonic packs the native planes and retains the complete CAMF block and typed metadata.

### Foveonic before and after

The first timing column for each original sums CAMF decompression and parsing. Complete opens include warm filesystem reads. Both are medians from the same four-process alternating protocol as the earlier slices.

| Original | CAMF before → after | Complete open before → after |
| --- | ---: | ---: |
| DP0 Quattro | 30.67 → 20.15 ms | 199.68 → 188.86 ms |
| DP2 Quattro | 31.82 → 20.41 ms | 174.77 → 163.40 ms |
| sd Quattro H | 44.42 → 23.91 ms | 225.07 → 204.32 ms |

### Matched comparison with FlashRAW

Both builds ran on the M1 Pro with the same three public originals. FlashRAW 0.5.0 used CPU raw-u16 output and the local comparison build with link-time optimization disabled. The CAMF and in-memory columns use eleven alternating runs per decoder after warmup. Foveonic's plane and preparation columns use five separate Perfetto-traced opens. Independent stage medians may differ from the total elapsed median.

| Original | Decoder | Unpack planes | Sensor preparation | CAMF | In-memory total | CAMF bytes decoded |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| DP0 Quattro | Foveonic | 156.8 ms | 6.0 ms | 20.2 ms | 182.3 ms | 3,126,480 |
| DP0 Quattro | FlashRAW | 112.8 ms | 12.6 ms | 1.6 ms | 127.9 ms | 241,664 |
| DP2 Quattro | Foveonic | 130.9 ms | 5.7 ms | 20.6 ms | 157.0 ms | 3,165,656 |
| DP2 Quattro | FlashRAW | 86.4 ms | 12.2 ms | 1.6 ms | 101.9 ms | 237,568 |
| sd Quattro H | Foveonic | 164.7 ms | 8.0 ms | 24.0 ms | 196.8 ms | 3,756,068 |
| sd Quattro H | FlashRAW | 106.3 ms | 15.4 ms | 0.3 ms | 123.9 ms | 36,864 |

Decoded CAMF and parsed metadata match the preceding build on eight originals across types 2, 4, and 5. DP0's quarter-scale Metal render retains its output digest and dimensions. Independent review found no correctness issues. A separate macOS review build is available.

## Wider refill

DP0's last CAMF record is `BadClustersChromaF23`, used by the [chroma defect repair](../crates/foveonic/src/render/recipes/quattro/defects.rs). Related luma defect records also sit near the end. The type-5 byte stream has a sequential predictor, so locating these records still requires walking the full stream. Keep complete decoding and put the next effort into sensor plane unpacking.

The bit reader refills six bytes at a time using a masked 64-bit load. Tracking the real bits buffered covers truncation without a second bit count. The short refill still leaves any seventh byte for the next fill. This keeps the exact Huffman lookup, sample validation, and row cancellation. A separate row-loop and packed table trial each lacked a consistent gain. The previous four-byte reader is the baseline below. Alternating runs used uncached decoded sources with warm filesystem reads. Desktop and sync processes were active during the measurements.

| Original | Sensor stage before → after | CAMF before → after | Complete open before → after |
| --- | ---: | ---: | ---: |
| DP0 Quattro | 163.03 → 157.49 ms | 20.53 → 19.44 ms | 190.28 → 183.83 ms |
| DP2 Quattro | 135.53 → 132.45 ms | 20.40 → 19.58 ms | 162.52 → 158.84 ms |
| sd Quattro H | 172.20 → 169.60 ms | 23.80 → 23.09 ms | 204.05 → 201.08 ms |
| Merrill HI, blue parking sign | 86.16 → 84.27 ms | 13.40 → 12.45 ms | 105.73 → 102.55 ms |

In the updated DP0 comparison, Foveonic measured 152.9 ms for plane unpacking, 6.2 ms for sensor preparation, 19.4 ms for CAMF and 177.0 ms in-memory total. FlashRAW measured 112.8 ms, 13.9 ms, 1.7 ms and 130.2 ms with the same boundaries and differing sensor preparation and CAMF coverage described above. Foveonic's plane and preparation columns came from separate traces.

Eight originals preserve all decoded sample bits, six preserve their complete CAMF and parsed metadata, and both damaged Merrill files retain their error details. The DP0 quarter-scale Metal render matches the preceding digest and geometry. The independent review found no correctness issues. A separate macOS review build is available.

## Isolated parallel plane experiment

The offline prototype split only the top plane into equal compressed-byte ranges. Workers decoded differences from guessed byte boundaries. The merge verified overlapping code boundaries, recovered row positions from the joined sample counts, and reconstructed rows from their two predictor seeds. Each measurement includes worker creation, joining, merging and reconstruction. Seven rounds alternated variant order on the M1 Pro. Every sample matched the scalar plane on the three originals.

| Original | Scalar | Four workers | Eight workers |
| --- | ---: | ---: | ---: |
| DP0 Quattro | 150.21 ms | 56.86 ms | 41.15 ms |
| DP2 Quattro | 131.02 ms | 50.03 ms | 41.52 ms |
| sd Quattro H | 160.39 ms | 64.51 ms | 46.65 ms |

These timings cover one plane in a standalone process. The prototype holds an extra full plane of differences during merging, about 44 MiB on DP0 and 57 MiB on Quattro H before allocation slack. Next validation needs the shared processing pool, bounded temporary storage, cancellation, deterministic damage reporting, and complete uncached source-open latency.

## Shared-pool decoding

Probe each cut over the longest possible Huffman token, including its category bits. Accept a split only when every earlier fragment lands exactly on the next cut, starting at bit zero. The declared sample count selects the end of the last fragment. Recover the same wrapping row and column predictors from the joined differences. An invalid join, sample count or range falls back to scalar decoding in stored plane order, which preserves the first technical cause of damage. The scout follows [Dougall Johnson's bounded prefix synchronization](https://dougallj.wordpress.com/2022/07/30/parallelising-huffman-decoding-and-x86-disassembly-by-synchronising-non-self-synchronising-prefix-codes/).

The entropy phase queues whole planes and fragments together. The next phase joins the fragments and reconstructs each split plane as a leaf job. The file caller waits outside the processing pool. In the first shared-pool trial, a viewport or export worker waiting for its own fragments could take a paused lower-tier job and never resume while its own demand was active. The flat arrangement completes the viewport work before releasing its demand in the reproduced interleaving. Cache prefetch keeps whole-plane jobs and fills the pool across its admitted photos.

These medians are from the MacBook Pro with eight performance cores and two efficiency cores. On that host, packed Quattro divides its full-resolution plane into eight fragments. Merrill assigns three, three, and two fragments to its three planes. Complete opens used the same four-process alternating protocol and warm filesystem reads, with the decoded cache bypassed. Peak resident memory was measured in separate runs. The baseline is the wider-refill build above.

| Original | Complete open before → after | Increase in peak RSS |
| --- | ---: | ---: |
| DP0 Quattro | 184.35 → 83.41 ms | 48 MiB |
| DP2 Quattro | 158.64 → 77.15 ms | 46 MiB |
| sd Quattro H | 202.63 → 94.65 ms | 60 MiB |
| Merrill HI, blue parking sign | 104.28 → 72.71 ms | 95 MiB |
| DP3 Merrill MED | 55.20 → 40.16 ms | 47 MiB |
| DP3 Merrill LOW | 32.63 → 25.21 ms | 25 MiB |

A separate matched run from the original cached-word reader to the shared-pool build measured 269.69 → 83.12 ms for DP0 Quattro, 240.55 → 77.48 ms for DP2 Quattro, 311.89 → 97.18 ms for sd Quattro H and 123.41 → 72.70 ms for Merrill HI.

The following comparison used eleven alternating runs per decoder after warmup on the same host and files. Foveonic's preparation and plane columns come from a separate five-open wall trace. Plane time includes predictor reconstruction. CAMF and in-memory total are independent medians from the comparison runs. FlashRAW's calibration stage and extracted CAMF differ from Foveonic's native packing and full CAMF coverage described above.

| Original | Decoder | Decode planes | Sensor preparation | CAMF | In-memory total |
| --- | --- | ---: | ---: | ---: | ---: |
| DP0 Quattro | Foveonic | 56.57 ms | 5.66 ms | 19.11 ms | 75.71 ms |
| DP0 Quattro | FlashRAW 0.5.0 | 112.23 ms | 10.58 ms | 1.61 ms | 125.72 ms |
| DP2 Quattro | Foveonic | 48.31 ms | 5.74 ms | 19.38 ms | 70.12 ms |
| DP2 Quattro | FlashRAW 0.5.0 | 86.01 ms | 10.12 ms | 1.59 ms | 99.09 ms |
| sd Quattro H | Foveonic | 59.14 ms | 7.48 ms | 22.65 ms | 86.74 ms |
| sd Quattro H | FlashRAW 0.5.0 | 106.13 ms | 12.75 ms | 0.26 ms | 120.82 ms |

All sample bits match across fifteen originals. Nine retain identical decoded CAMF and typed metadata. Both damaged Merrill files retain their exact technical causes. DP0 Quattro and Merrill HI retain their quarter-scale Metal render digests and geometry. Workspace tests, strict Clippy, formatting, and documentation checks passed. Release-profile original tests passed for serial-plane parity, trace parity, cache round trips, and prefetch cancellation and admission. The debug run of the folder-prefetch original test reached its 15-second deadline. The signed macOS review build is available for owner testing. The benchmark ends at source loading, and Finder Quick Look latency remains unmeasured.

## Further research

- Overlap word refill with lookup on one core. Dougall Johnson's [Apple M1 experiment](https://dougallj.wordpress.com/2022/08/26/reading-bits-with-zero-refill-latency/) uses the old bit buffer for the current lookup while loading the next word. The reported 6% applies to the experimental DEFLATE decoder. X3F needs a separate timing and damage check.
