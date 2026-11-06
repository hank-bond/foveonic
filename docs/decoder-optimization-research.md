---
summary: "Fixed-stream X3F decoder experiments compare fused Huffman payload tables, explicit NEON predictors and independent Classic rows."
kind: explanation
---

# Fixed-stream decoder optimization

The [decoder plan](../plans/archived/decoder-optimization.md) records the approved experiments that preserve original sensor samples, damage detection and cancellation on stable Rust. The existing reader already uses a twelve-bit Huffman category table, cached 64-bit lookahead and independent sensor-plane workers. Fixed camera bitstreams cannot gain new restart markers or independent entropy streams.

## Applicable research

[Drijvers et al. (2010)](https://pure.tue.nl/ws/files/3308166/Metis244492.pdf) vectorize table searches on a configurable VLIW processor. Their reported 24-fold gain compares with linear table search, which differs from this reader's direct prefix lookup. The transferable ideas are compact table data and fewer conditional loads. The reported multiplier does not predict an X3F speedup.

[Johnston and McCreath (2017)](https://users.cecs.anu.edu.au/~Eric.McCreath/papers/JohnstonMcCreathHuff2017.pdf) decode from every input bit, then combine valid boundaries using a large intermediate table. Their GPU implementation only marginally beats a simple serial decoder and depends heavily on memory transfers. X3F also needs payload interpretation and predictive reconstruction. This approach remains unimplemented because extra work and storage conflict with the current loading-memory goal.

[libdeflate](https://github.com/ebiggers/libdeflate/blob/master/lib/decompress_template.h) combines table information with extra payload bits and uses a buffered fast path. [Zstandard's Huffman decoder](https://github.com/facebook/zstd/blob/dev/lib/decompress/huf_decompress.c) includes two-symbol lookup entries and four independent streams. These motivate fused differences and a paired-difference experiment. Independent streams are useful only where the camera format already provides them. No external code was copied.

[LibRaw's Fuji compressed reader](https://github.com/LibRaw/LibRaw/blob/master/src/decoders/fuji_compressed.cpp) illustrates adaptive gradient prediction and independent strip decoding. The transferable lesson is to identify real format boundaries before scheduling workers. Its coding and predictors differ from X3F. No Fuji decoder is included in this work.

The [Rust portable SIMD project](https://github.com/rust-lang/portable-simd) and [standard-library documentation](https://doc.rust-lang.org/stable/std/simd/index.html) still identify `std::simd` as experimental and nightly-only when checked in November 2026. Experiments therefore use stable `std::arch::aarch64` NEON intrinsics with scalar comparisons. No toolchain or dependency change is required.

## Measurements

Local release-mode trials used an Apple M1 Pro and Rust 1.98.1. One owned test executable ran serially without competing agent builds or measured workloads. Source opening includes cached file reads, sensor decoding, interleaving and camera metadata. Five interleaved repeats compared every decoded floating sample exactly outside each timed interval. These are local observations rather than performance targets.

| Original | Existing source open | Fused prefix and payload | Fused entropy then NEON predictor | Paired prefix and payload |
| --- | ---: | ---: | ---: | ---: |
| Merrill blue parking sign | 239.441 ms | 130.971 ms | 137.814 ms | 118.301 ms |
| Merrill low-light desk | 241.238 ms | 137.448 ms | 143.635 ms | 117.309 ms |
| Classic DP1s | 71.099 ms | 40.154 ms | 41.681 ms | 34.615 ms |
| Quattro DP0 | 395.142 ms | 278.262 ms | 286.315 ms | 257.851 ms |

The fused table precomputes the signed difference when the category prefix plus payload fits twelve bits. Longer combinations use the existing checked category decoder. Its 4,096 four-byte entries add 16 KiB per active plane, around 48 KiB across three plane workers. Original files, predictor equations, row cancellation points and decoded storage stay the same.

Explicit NEON substantially accelerates reconstruction in isolation: about 25.7 ms to 8.1 ms per 16-megapixel plane. Separating entropy from prediction adds a full difference buffer and another pass. Three such buffers add about 96 MiB, and complete source opening is slower than the fused scalar loop. NEON interleaving measured about 6.4 ms against 6.6 ms for the compiler-friendly scalar iterator. Neither result justifies adding a production intrinsic path.

The initial paired-prefix experiment decodes two differences when both fit twelve lookahead bits. It adds a roughly 24 KiB pair table alongside the single-difference table, or 72 KiB across three workers. Consuming both differences before checking the first reconstructed sample can change the damage detail when that sample exceeds its range and the second difference is truncated. A completed follow-up adds a real-bit availability guard and falls back to one difference at incomplete pairs. Every sample across 29 originals, exhaustive pair entries, short damaged streams, cancellation and an explicit range-before-truncation regression pass.

The compatible pair path loses its initial speed advantage. Five-repeat single-to-paired medians were 132.682 to 148.818 ms for the parking sign, 139.412 to 136.574 ms for the desk, 39.956 to 41.218 ms for DP1s and 284.025 to 304.741 ms for DP0 Quattro. With the required bounds decision and pair dispatch, the candidate regresses three of four inputs while adding table storage. The candidate is rejected and removed from production. The checked single-difference lookup remains selected.

Classic format-6 rows have real directory boundaries and reset all channel predictors. The completed production path writes disjoint chunks of the existing output allocation. Active loading uses at most four workers with inherited scheduling context. Speculative photos queue at most their configured pool's worker count directly through the existing shared FIFO pool. No nested worker group is created per speculative photo. The photo admission turn releases after every chunk is queued. Every chunk joins before errors are collected in row order, preserving the earliest damaged row's technical cause.

Five interleaved repeats on the final executable preserve every sample and measure complete source opening:

| Original | Serial rows | Foreground, at most four workers | Shared one-worker pool | Shared two-worker pool | Shared four-worker pool |
| --- | ---: | ---: | ---: | ---: | ---: |
| SD9 | 98.528 ms | 27.115 ms | 101.658 ms | 56.168 ms | 28.087 ms |
| SD10 | 99.547 ms | 28.491 ms | 100.984 ms | 53.018 ms | 29.216 ms |
| SD14 | 135.266 ms | 37.401 ms | 136.101 ms | 71.303 ms | 38.875 ms |

These are all three format-6 originals in the 29-file validation set. Foreground loading is about 3.5–3.6 times faster. One shared worker stays close to serial throughput. Shared measurements use the inherited interruptible decoding context. More workers improve one-photo latency within the existing global worker bound. Deterministic two-photo tests verify FIFO admission and that every task runs only on configured pool workers, including when the next photo begins before the preceding photo finishes.

The table, compressed input and final output are borrowed by disjoint chunks. There is no second image allocation or copied row payload. Only small result slots and worker stacks are additional. Separate SD14 process-memory captures retain the harness's expected frame plus its current decoded frame. Peak footprint measured 131,957,240 bytes serially and 132,170,256 bytes with foreground workers, an increase of about 208 KiB. Peak RSS increased by 368 KiB. These local high-water observations include the correctness harness and are not a memory limit or a GUI prediction.

## Validation and reproduction

The maintained Huffman tests compare fused differences with the category decoder across all 4,096 lookahead prefixes and every available-bit boundary from zero through 24 bits. They cover missing prefixes, long payload fallback, signed differences, predictor row parity, geometry limits, declared sample limits and typed background interruption. Original-file comparisons use the same complete loader with a test-only category-decoder switch and compare every decoded sample outside timing.

The first production slice passed 37 X3F tests, three input-error tests and six background scheduling tests, plus exact sample comparisons on 29 local originals spanning Merrill, Classic, Quattro and reduced modes. Its five-repeat production medians were 236.080 to 134.777 ms for the parking sign, 235.697 to 140.686 ms for the desk, 69.098 to 40.579 ms for DP1s and 391.522 to 284.533 ms for DP0 Quattro. Complete opening decreased by about 27–43% in these four cases. The first table records the wider experimental comparison from its separate executable.

The final Classic slice passes 53 focused CPU tests, all processor targets under Clippy with warnings denied and workspace formatting. Tests cover inherited scheduling context, active and queued cancellation, releasing the next photo's admission, worker bounds, joining every chunk after an error and earliest-row damage details. The complete mixed candidate also preserves all 29 original outputs. The rejected pair was then removed, retaining the previously validated single lookup, and all three format-6 originals were compared again on the final executable.

Run the maintained comparison serially with explicit local originals:

```sh
DECODER_RESEARCH_FILES=path/to/merrill.x3f:path/to/classic.x3f:path/to/quattro.x3f \
  cargo test --offline --release -p foveonic --lib \
  x3f::huffman::lookup_tests::profile_fused_original_loading \
  -- --ignored --exact --nocapture --test-threads=1
```

`DECODER_RESEARCH_REPEATS` defaults to five. Shared build and measurement sessions must coordinate access and copy their executable before another build can overwrite it. Warm opening measurements do not establish cold disk throughput, whole-GUI responsiveness or total process memory. Memory figures above describe additional decoder buffers, not process residency.

The Classic comparison uses `x3f::format6::parallel_tests::profile_bounded_classic_rows` with the same original-path and repeat variables. It compares serial row math, four foreground workers and shared pools of one, two and four workers. `DECODER_CLASSIC_MODE=serial`, `foreground`, `shared1`, `shared2` or `shared4` selects one mode for separate process-memory captures. Pool creation and destruction stay outside each source-opening timer.
