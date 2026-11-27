---
summary: "Measured graph and shader optimizations, with the signs that another node can use them and the constraints that make them safe."
kind: how-to
---

# Optimization log

Use these patterns when reviewing a node or writing a shader. Keep each entry tied to a measured case. Record timing, memory and image comparisons in the linked study.

## Memory

### Reuse scratch across ordered passes

Look for a full-image allocation inside a loop over blur levels or axes. Identify the last read of each intermediate. Once that read has been submitted on the same queue, the next pass can overwrite the buffer if no graph output or inspector retains it. A pass that reads neighboring pixels needs a stable source for its entire dispatch. Measure held footprint separately from allocation traffic. To attribute a process peak to a node, compare the OS high-water mark at node entry and after GPU completion. [Clarity measurements](../plans/graph-node-performance.md#reusing-clarity-scratch-across-six-blur-levels)

### Reuse an input after its last graph read

Look for conversions that read and write one pixel at a time. A final graph consumer can take the input if the buffer has no other owner. Check both the remaining graph reads and retained cache or inspection handles. Packed Lab can then write back into the input. Keep a path for shared inputs and compare the original and updated views together. Measure allocation requests, lifetime peak and settled footprint separately. [Packed Lab measurements](../plans/graph-node-performance.md#reuse-the-two-rgb-buffers-along-that-path)

### Let a large temporary retire before allocating the next image

Measure the boundaries where a node with large scratch is followed by a full-size output. Quattro can let reconstruction finish before residual RGB allocates. Measure the latency cost and scope the wait to render sizes that benefit. Probe process footprint as well as graph ownership to distinguish a live image from GPU backing that is still in flight. [Native Quattro handoff](../plans/graph-node-performance.md#retire-full-native-reconstruction-before-residual-rgb)

### Write the format the next pass needs

Look for a full-image temporary followed by a per-pixel conversion. Quattro's chroma limiter applies the existing YUV-to-RGB conversion directly to the clamped values. Keep the stable neighborhood input, intermediate Float32 rounding, and any outputs that an inspector can request. Measure storage requests separately from process peak. [Chroma limiter measurements](../plans/graph-node-performance.md#convert-the-limited-chroma-directly-to-rgb)

### Give an optional output its own graph dependency

Look for a node that always writes a second image consumed by one processing mode or by the inspector. Make the secondary output a separate graph node and attach its private inputs there. Quattro Universal sharpening can then skip signed detail and its enlarged mask. Measure the first switch to a mode that needs the skipped branch. Also trace the whole render's memory peak, since changing when a buffer is released can move the peak downstream. [Quattro signed-detail measurements](../plans/graph-node-performance.md#request-signed-detail-only-when-a-consumer-uses-it)

### Share a calibration input when the applied gains are all one

Check the decoded coefficients used by the shader. Quattro’s RGB and blue shading nodes can return the same read-only storage when every table entry is exactly one. Check input geometry first, keep each graph output available to inspection, and count shared storage once in the cache. Which images benefit depends on their calibration and selected white balance. [Quattro measurements](../plans/graph-node-performance.md#quattro-reconstruction)

### Store only what the next pass reads

Follow the reads in the consuming shader. In Clarity the vertical blur needs one horizontal mean per neighbor. Its accumulated correction belongs to the center pixel and can stay in that pixel’s state. A single scalar plane therefore serves all six horizontal passes. Check every consumer of the old channels before narrowing the buffer. [Clarity measurements](../plans/graph-node-performance.md#reusing-clarity-scratch-across-six-blur-levels)

### Size the patch margin for the active filters

Look for a fixed border sized for the longest filter in any recipe. Keep the grid phase and derive the smaller border from the operations that actually run. Quattro native box contrast uses a smaller patch margin when Reference sharpening is off. Compare center and edge patches with their Whole crops, then measure first zoom, warm pans and footprint separately. [Quattro patch margin measurements](../plans/graph-node-performance.md#smaller-margins-for-quattro-box-contrast)

## Latency

### Batch independent repairs

Look for a loop that submits one sparse repair region at a time. Compare each region's write bounds with the samples read by its neighbors, including the donor patch halo. Group independent regions in one dispatch and keep dependent regions in their original order. Quattro went from 60 dispatches to one on a native DP2. Compare the repaired plane and the final pixels against serial repair. [Quattro cluster measurements](../plans/graph-node-performance.md#batch-independent-blue-defect-clusters)

### Shorten filter support in Speed mode

A wide neighborhood can dominate native rendering even after its loads are tiled. Below capture ISO 800, Quattro Speed reduces the blue filter from 121 to 49 taps while retaining the same color weights and blend. Put the effective radius in the cache identity and keep small working grids on their existing support. Compare SDR display crops as well as float deltas. Measure high-ISO grain and process peak separately, since faster submissions can change when other buffers retire. [Speed blue-filter measurements](../plans/graph-node-performance.md#shorten-the-speed-blue-filter-window)

Quattro's coarse RGB pass also caps its radius after computing the working grid. At native resolution it samples a checkerboard of offsets within that window, keeping the center and reach. Estimated ISO 400 and the long-exposure path keep their original support. Check repeating fine texture and grain in the areas of greatest difference, compare against Quality on the same grid, and verify that the Speed patch agrees with its whole-image continuation. [Sparse Speed chroma measurements](../plans/graph-node-performance.md#sample-alternating-taps-in-the-speed-chroma-filter)

### Share the halo around a neighborhood filter

When adjacent workgroups repeatedly load the same border pixels, try increasing the tile size. Quattro’s 16 × 16 bilateral tile roughly halves the number of shared-tile loads per output for an eleven-wide window. Keep each pixel’s sample order and test partial workgroups. Larger groups use more local storage, so time the individual filter and the completed render on each target GPU. [Quattro measurements](../plans/graph-node-performance.md#quattro-reconstruction)

### Stop a filter at the last band that affects the result

Inspect the final mix at neutral control values. With Merrill Coarse Amount at one, its broad blur cancels and both map inputs can read the Fine boundary. Derive the loop length from the effective controls and use it for dispatch accounting too. Exercise the inspector and non-neutral fallback. Algebraic cancellation can change floating-point rounding. [Merrill Detail measurements](../plans/graph-node-performance.md#skipping-the-broad-blur-at-neutral-coarse-amount)

## Correctness

### Keep the order of floating-point operations when changing storage

When a rewrite only moves data, preserve the filter taps, accumulation order and border handling. Compare complete float outputs on each backend against its own baseline. For a separable filter, use a textured two-dimensional fixture with a partial workgroup on both axes. Flat fields and single-row sinusoids can both pass with a broken vertical pass. [Clarity comparison](../plans/graph-node-performance.md#reusing-clarity-scratch-across-six-blur-levels)

### Inspect the largest local differences in Speed

A whole-image similarity percentage can hide isolated defects. Check both the area with the largest average error and a crop centered on the largest pixel difference. Bypassing Quattro's chroma protection changed a few pixels at saturated edges by over 100 SDR codes despite most of the image staying within one code. [Chroma limiter measurements](../plans/graph-node-performance.md#convert-the-limited-chroma-directly-to-rgb)

### Verify the render that produced the measurement

A saved edit can name a disabled control, and a photo recipe omits transient scale, region and processing policy. Use the diagnostic request snapshot for replay. Check the delivered view and scale, and confirm the measured node actually ran. This caught a Fit memory trial that rendered native and a nonzero Clarity setting bypassed by the selected Tone method. [Resource profiling](./profiling.md#resource-overlap)
