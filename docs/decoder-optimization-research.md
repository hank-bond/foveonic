---
summary: "Fixed-stream X3F decoder experiments compare fused Huffman payload tables, explicit NEON predictors and independent Classic rows."
kind: explanation
---

# Fixed-stream decoder optimization

The [decoder plan](../plans/decoder-optimization.md) records the approved experiments that preserve original sensor samples, damage detection and cancellation on stable Rust. The existing reader already uses a twelve-bit Huffman category table, cached 64-bit lookahead and independent sensor-plane workers. Fixed camera bitstreams cannot gain new restart markers or independent entropy streams.

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

The paired-prefix experiment decodes two differences when both fit twelve lookahead bits. It adds a roughly 24 KiB pair table alongside the single-difference table. Exact samples and rejection behavior pass the tested originals and short streams. Consuming both differences before checking the first reconstructed sample can change the damage detail when that sample exceeds its range and the second difference is truncated. A production attempt must preserve that ordering, for example by falling back to one difference when the complete pair is unavailable. The simpler single-difference lookup is the current production slice.

Classic format-6 rows have real directory boundaries and reset all channel predictors. Four scoped workers reduced median opening from about 98.5 to 27.4 ms on SD9, 100.8 to 28.4 ms on SD10 and 134.6 to 37.9 ms on SD14. Every sample matched. Three workers measured 35.3, 37.0 and 48.6 ms respectively. Production integration must schedule these rows through the existing bounded speculative decode pool and preserve foreground priorities. Spawning an extra worker group inside each admitted photo would bypass that ownership policy. This experiment remains outside production.

## Validation and reproduction

The maintained Huffman tests compare fused differences with the category decoder across all 4,096 lookahead prefixes and every available-bit boundary from zero through 24 bits. They cover missing prefixes, long payload fallback, signed differences, predictor row parity, geometry limits, declared sample limits and typed background interruption. Original-file comparisons use the same complete loader with a test-only category-decoder switch and compare every decoded sample outside timing.

The production slice passed 37 X3F tests, three input-error tests and six background scheduling tests, plus exact sample comparisons on 29 local originals spanning Merrill, Classic, Quattro and reduced modes. All processor targets pass Clippy with warnings denied, and workspace formatting passes. Final five-repeat production medians were 236.080 to 134.777 ms for the parking sign, 235.697 to 140.686 ms for the desk, 69.098 to 40.579 ms for DP1s and 391.522 to 284.533 ms for DP0 Quattro. Complete opening decreased by about 27–43% in these four cases. The table above records the wider experimental comparison from its separate executable.

Run the maintained comparison serially with explicit local originals:

```sh
DECODER_RESEARCH_FILES=path/to/merrill.x3f:path/to/classic.x3f:path/to/quattro.x3f \
  cargo test --offline --release -p foveonic --lib \
  x3f::huffman::lookup_tests::profile_fused_original_loading \
  -- --ignored --exact --nocapture --test-threads=1
```

`DECODER_RESEARCH_REPEATS` defaults to five. Shared build and measurement sessions must coordinate access and copy their executable before another build can overwrite it. Warm opening measurements do not establish cold disk throughput, whole-GUI responsiveness or total process memory. Memory figures above describe additional decoder buffers, not process residency.
