---
summary: "Move the sensor reduction boundary earlier with working-resolution inputs, unchanged Full output and existing recipe ownership."
---

# Early sensor reduction

Requirements: [small previews](../docs/specs/small-previews.md) and [render preparation](../docs/specs/render-preparation.md).

Keep existing recipes, typed graphs, execution and cache owners. Metadata remains in original camera coordinates. Processing stages receive explicit working geometry and physical support instead of a fabricated smaller camera. No selected operation is disabled to meet a timing target.

## Sequence

The completed checkpoint uses area means, compact brightness guides, profile-sized Adaptive analysis, reduced bright-only speckle detection and original clipping evidence on the working grid. The investigation sections below are chronological. Their temporary alternatives are superseded by the later adoption sections and are not selectable application paths.

Remaining work is native guarded-row filtering and the corresponding Classic/Quattro boundaries, followed by broader quality and latency acceptance. The Multiscale slice is complete and archived below. Calibration stays native for correctness. This checkpoint does not claim that the complete small-preview specification is implemented.

- [x] Review recorder lifecycle, overflow metadata recovery and task-based work attribution before using traces.
- [x] Map the current pre-pool stage chains and preserve a Full float baseline before numerical changes.
- [x] Move Merrill row correction after pooling for Color and Monochrome. Preserve physical row-bank phase, scale polynomial support and verify Full bytes, working geometry and retained/fresh results.
- [x] Record the power-of-two scale contract and node-owned parameter preparation.
- [x] Resolve reduced speckle correctness after resampling. Keep every input on its declared grid, preserve Full, and review both detectors at preview sizes.
- [x] Correct Adaptive Tone structure probes that collapse to zero at Draft and Thumb. Preserve Full/Fast output and compare automatic brightness across scales.
- [x] Move Merrill highlight recovery after basic calibration and mapped repairs. Retain separate evidence only where the selected algorithm requires it.
- [ ] Resolve native shield/row calibration separately from multiscale neighborhood processing.
- [x] Move Directional cleanup to the working grid without retaining an alternate native branch or bypassing selected speckle processing.
- [ ] Apply the same distinction to Quattro/Tiger mapped defects, phase calibration, neighborhood repairs and highlight recovery.
- [ ] Verify all supported families and selected controls through the same scale contract. Record remaining native work explicitly.
- [ ] Measure cached-source first preparation separately from retained edits and complete group delivery. Review photographic quality before release.

## Completed Multiscale slice

The owner accepted the review build and approved the checkpoint. The [completed slice](./archived/multiscale-working-grid.md) records signal units, explicit repair provenance, Full comparisons, Adaptive tolerance and timing evidence. Guarded-row estimation is the next Merrill stage.

## Findings

At the start of this pass, the Merrill cut followed repair, highlight recovery, speckle filtering and row correction. Quattro similarly reduces after its repair/filter/recovery chains. These are pipeline boundaries, independently of scheduling. Several operation descriptions still carry native dimensions and support sizes, so relocating a graph edge alone would produce incorrect addressing or scale.

The first Merrill slice keeps the existing row-correction operation and selected polynomial degrees. Its description maps sensor geometry onto the working grid. Patch sizes follow physical support, and row banks retain their original sensor phase independently of padding. Color and Monochrome prepare the same description and execute it after their sensor reduction. Other pre-pool stages remain open work.

## First working-grid slice

Row correction passes its native GPU fixture and follows pooling in authored Color and Monochrome graphs at divisors one, two, four and eight. Source-free reset renders match fresh output at divisors two, four, eight, sixteen and sixty-four. A complete Full float image with reconstruction and row correction selected is bit-identical before and after this change.

Three alternating headless Wall trials with two repeated fresh renders per process measured a Thumb median of about 296 → 227 ms, excluding source opening and export. Full repeated fresh-render medians were about 667 and 659 ms. Runs used the same original, explicit reconstruction, WB/mode and 512×342 export size, with no competing app/compiler workload detected. These are engine measurements, not native application latency or retained-edit measurements. One cold sample is insufficient for a performance claim. The engine remains above the first-Thumb target and the remaining pre-pool filters/recovery are still open.

The compared Thumb samples have unchanged framing. On one hazy-coast photograph, tone-mapped RGB RMS change was about 0.00010, with a 0.00221 maximum on a nominal 0–1 range. This one-image comparison does not replace broader photographic review.

## Speckle and Directional working grids

Both speckle detectors now prepare support, inclusive detector bounds and noise thresholds beside their implementation. Reference filtering has no sensor-code dependency. Compact retains its bright-region guard using explicitly connected sensor codes reduced with the same filter and geometry as its calibrated input. It detects remaining problems from working-resolution samples without an added native evidence pass. Partial detector blocks are zero-ranked through the existing prefix/repair stages instead of imposing a native-size pixel-count constraint.

Directional cleanup previously branched from native calibrated layers and reduced its result afterward. That branch would bypass the relocated speckle stage. Color and Monochrome now supply working layers to the same Directional operations. Explicit scale replaces a dimension-ratio estimate that included padding. Filter widths, measurement blocks, pooling radius and alignment offsets follow the divisor. Tile projection retains its required working-pixel seam support.

Native speckle and Directional GPU fixtures pass. Tiny and padded working grids preserve flat fields and immutable inputs. Authored Color and Monochrome graphs give every speckle input the requested grid. Source-free reset, Compact, Color Directional and Monochrome Directional results match fresh renders at divisors two, four, eight, sixteen and sixty-four, including downstream and reconstruction edits. Full float output is bit-identical on one Merrill photograph for Color, Monochrome and Color Directional configurations. Three alternating Off trials with three warm fresh renders each measured a Thumb median of about 226 → 209 ms, excluding source opening and export. The workload monitor observed no other app or compiler work. These are resident decoded-source engine renders, not retained edit or GUI delivery timings.

The same photograph shows a tone-mapped RGB RMS change of about 0.0090, with a 0.350 maximum. The new speckle on/off difference is similar, so the relocated filter accounts for most of that change. At a 512×342 export, the images appear broadly similar, but isolated highlight/detail changes still need owner review. No noise-based bypass has been introduced. Highlight recovery and multiscale filtering still precede reduction, and the first-Thumb target has not been reached.

The rebuilt native app passes the eight-photo cached-switch scenario with a complete loss-free trace. Cached switches and an unchanged re-add issue no payload reads or photographic dispatches, and progress settles. Initial mixed-camera selection still takes about 4.8 seconds in this one Wall trial. It is a correctness check, not a group speed improvement claim, and physical screen presentation is outside its coverage.

The owner reviewed the Thumb before/after comparison in Preview. The new result is somewhat softer but considered plausible for a small-scale speckle filter, rather than incorrect. Full remains the bit-perfect requirement. Fast should have small overall changes, while Draft and Thumb receive visual spot checks. A possible future Smart recipe may omit visually unhelpful Thumb processing, separately from making each node scale-correct.

A further native-size Full float comparison is bit-identical. Three Fast comparisons (hazy coast, parking sign and low-light desk) have nominal 0–1 RGB RMS differences of about 0.0041, 0.00071 and 0.0030. Their 99th-percentile component differences are at most 0.0062, with larger isolated changes up to about 0.66. These are changes in fine-scale filtering, not Fast numerical parity.

## Adaptive structure sampling regression

Owner review isolated a broad Fast-to-Draft darkening to Adaptive Tone. The estimator keeps a 256×256 lattice and already uses Draft/Thumb RGB directly, so an additional overview reduction is not the cause. Its two-native-pixel structure probe rounds to zero on the reviewed GPU at Draft and smaller scales. Both global histogram and regional structure scores become exactly zero on a coherent log-light ramp. The same offsets also govern near-black rescue.

A controlled hazy-coast comparison with Exposure −0.89556 and Natural reproduces the failure. Across fixed scene locations classified by Camera Fast brightness, Draft’s dark-third mean is about 41% lower than Natural Fast, while Camera Draft is slightly brighter than Camera Fast. Fit the fix to representable working-pixel probes rather than replacing the sensor resampler or forcing native analysis.

The corrected operation prepares integer probe radii with a one-working-pixel minimum. Histogram, regional estimation and near-black rescue share these supports. The 256×256 lattice, guide reductions and sensor resampler remain unchanged. The coherent-ramp regression fails before the fix at divisor four and passes at divisors one through sixty-four afterward. Native Full and Fast comparisons remain bit-identical on reviewed Merrill, Classic and Quattro photographs. Quattro Fast with an explicit 512×342 output is already resized before shared finishing, so its previously collapsed application probes also change. That small-output case has RGB RMS difference about 0.000058.

The large darkening disappears on the hazy-coast comparison. Draft and Thumb retain some brightness variation from Fast because their input detail changes. Owner visual review remains necessary.

An older Full guide-reuse assertion fails before and after the sampling fix. A loss-free retained-edit capture identifies the cause: the initial frontier retains large reconstruction inputs, then evicts the unchanged 61 MiB application guide under the 1 GiB local checkpoint limit. Its generation is unchanged when rebuilt on the first Exposure edit. Initial Full frontier priorities remain a cache-cleanup follow-up. The positive reuse test now uses Fast to fit its working set, while the native pressure behavior remains explicitly recorded here. Another older assertion excluded Monochrome Fast and has been updated to the accepted all-mode scale contract.

The cached application guide is currently expanded from half-resolution values into a full-resolution scalar mask. Its 2×2 repeated storage is a possible fourfold memory reduction, separate from the estimator’s half-size analysis mask and 256×256 sample array. No representation or cache-priority change has been made in this slice.

The sampling fix passes signed/zero-cap safety, near-black rescue, photographic/DNG export and bounded guide-reuse checks. Natural, Strong and combined Monochrome retained renders match fresh output at divisors two, four, eight, sixteen and sixty-four. The guide-storage expansion and initial Full cache-priority issue remain unchanged.

The owner approved the Adaptive sampling fix visually and authorized compact brightness-guide storage with bit-perfect validation. Keep normal processing guides compact and materialize the existing full-size mask inspection only on request. Preserve explicit buffer types and framing through the representation change.

A separate owner report identifies at least one green and one magenta hot pixel in the bottom-left of the low-light speaker photograph at Fast, Draft and Thumb, across both despeckle methods. Full does not show those points. Trace the reduction/filter boundary separately from the bit-preserving guide change.


## Compact brightness guides

Application and analysis guides now retain one encoded Float32 per covered 2×2 image block, including odd edges. Numerical consumers address those cells directly. Scalar mask inspections expand only when requested. A distinct graph port type prevents expanded inspection storage from accidentally feeding numerical guide inputs. Execution framing crosses opaque buffers explicitly so expanded masks preserve camera crop and orientation.

Seven photographic before/after float comparisons are bit-identical across Merrill, Classic, Quattro, Monochrome, Adaptive Tone and combined HDR. Five public-mask comparisons preserve every sample and framing descriptor. GPU fixtures cover the numerical consumers and exact expansion on even, odd and single-pixel domains. A 4928×3264 application guide retains about 15.3 MiB instead of 61.4 MiB. This is storage compaction of existing repeated values, not a further analysis reduction. It does not resolve the separate first-Full cache-frontier policy.

The original shared-analysis test also assumed identical Monochrome Full/Fast samples. A frozen pre-compaction executable disproves that assumption after early sensor filtering. Color still prepares identical half-size analysis. Monochrome now filters its requested grid before its own overview, so its sample values and fit may differ. Both modes must replay the inspected RGB and guide into exactly the actual per-quality output.

## Preview hot-pixel diagnosis

The speaker photograph reproduces both colored residuals with the saved +3.35 EV recipe and both detectors. One native red-layer spike is about 6923 codes among neighbors near 10. Full repairs it to about 9. After reduction it spreads across a 2×2 cluster, including values near 980, 336 and 329. Reference repairs the peak to about 81 and Compact to about 82, while neighboring contaminated values remain unchanged and also enter the donor set. A green-layer spike has the same behavior. Further reduction attenuates these residuals without reliably removing them.

This is an operation-order limitation, not an Adaptive Tone or compact-guide regression. The native-repair alternative was paused after the owner asked to investigate resampling and working-resolution detection first. The production recipe remains at its prior reduced-repair boundary while those experiments are reviewed.


## Reduced speckle experiments

The owner chose to investigate lower-resolution processing before adding a native detector or accepting the native cost. Compare the existing overlapping triangular reduction against non-overlapping area means. Test both current despeckle methods with noise thresholds matched to each filter. Independently test a wider clean-donor neighborhood for the triangular footprint. Preserve Full and keep experimental choices out of application defaults. Check the speaker's two photographed defects, flat/noisy fields, thin lines and real small colored details. Compare filtered reduced data against the same reduction of native-repaired data, then review normal photographic continuations. Record processing time separately from readback and source opening.


Area pooling and wider-donor prototypes are isolated from production defaults. Four Merrill photographs cover both methods at divisors two, four and eight. Three photographs also continue through normal photographic processing, with downstream noise estimates matched to each reduction. Numeric comparisons use the same resampler after native repair as an algorithmic comparison, not an export-parity requirement. The owner clarified that UI speed and sufficient culling detail take priority over maximum preview fidelity.

Both alternatives remove the speaker's large colored residuals. For Reference Fast, the red-neighborhood maximum difference from native-repaired reduction falls from about 325 codes with the current triangular path to about 6 with area means. At Thumb it falls from about 31 to about 1. Compact shows the same pattern. Wider rings also remove the defects but alter more surrounding samples. Area means retain the existing detector algorithms and require the box-average noise estimate, 1/divisor, instead of the triangular filter's lower estimate.

Known-input controls preserve long lines but expose the expected loss of tiny colored features. A non-overlapping 2×2 average can make an isolated spike and a genuine two-by-two colored patch produce the same reduced sample. These are preview-detail tradeoffs to review, not a requirement for native-identical output. No reduced-buffer detector can recover distinctions removed by that average alone.

Five completed warm repetitions per scene on two resident native GPU inputs measured reduction plus existing repair, including allocations and completion waits. Draft Reference was about 17.2 ms with triangles and 13.4 ms with area means. Thumb was about 16.0 and 12.9 ms. Compact Draft was about 25.4 and 22.3 ms, and Thumb about 25.7 and 18.6 ms. Compact includes reduction of its sensor-code input. The generic area implementation was slower at Fast: Reference about 19.9 versus 23.1 ms, Compact about 26.4 versus 34.1 ms. These small-stage measurements exclude decode, upstream corrections, downstream reconstruction and UI delivery. They do not establish whole-app latency or the first-Thumb target. Controlled workload monitoring found no competing app or compiler work.

Area means are the simpler candidate for owner review. Do not adopt the experimental wider-ring worker, expose a new resampler choice, or change release defaults from this investigation alone. The temporary test hooks and noise-model override have been removed from the production source. Local comparisons retain the measured numerical results, normal photographic continuations and a frozen opt-in research runner.


After removing the research hooks, workspace tests, strict Clippy, formatting and documentation checks pass. GPU checks cover compact-guide expansion, public-mask replay, Monochrome HDR inspection and authored speckle input grids. Source-free Adaptive and combined Monochrome edits match fresh rendering at divisors two, four, eight, sixteen and sixty-four. These checks validate the retained production work. They do not adopt the area or wider-ring experiments.


## Adopt area pooling

The owner selected non-overlapping means for working sensor buffers. Implement one pooled-signal operation with integrated edge padding, migrate the Merrill and Classic sensor cuts, and retain Quattro's existing means and categorical masks through the same implementation. Keep camera-internal triangular filters unchanged. Derive independent-noise scaling from the sensor pool rather than the internal pyramid. Verify partial cells, signed rounding, locality and immutable inputs. Preserve Full float goldens. Recheck both speaker defects, retained/fresh rendering and preparation latency before rebuilding the app for review. No wider-ring experiment is selected.


The first production Full comparison is bit-exact for native-size Merrill, HDR, Monochrome and Directional cases, Classic SD14, ordinary DP/sd Quattro and Tiger. Natural Adaptive Tone changes because its half-size analysis now follows the selected area sampler. A fixed-grid sample of the hazy-coast Full result has RGB RMS difference about 0.000198 on the normalized output range and mean RGB brightness about 0.055% lower. The owner explicitly selected consistent area means rather than preserving a separate legacy Full analysis sampler. Record that accepted analysis change separately from native image-processing parity.


## Adaptive analysis-size experiment

The owner requested a photographic and timing comparison of the existing analysis policy against Thumb-sized analysis. Keep the main RGB, application guide and final processing at native Full resolution. Change only the estimator's RGB preparation and corresponding analysis guide to the one-eighth sensor grid. Compare Natural and Strong across the local camera/scene set, with additional exposure and monochrome cases. Measure every valid final-image component and report mean brightness, RMS, percentiles and the largest difference. Record completed fresh renders from resident decoded sources separately from retained Exposure edits and synchronized per-node GPU diagnostics. Keep the experimental override out of normal processing until the owner reviews the results.

The owner considers roughly one percent brightness variation acceptable because the automatic response has no single objectively correct brightness. Report mean-brightness variation explicitly and retain local difference measurements so opposing changes in shadows and midtones are visible. This tolerance does not authorize changing the analysis policy before reviewing the comparison.

The owner clarified that Classic Thumb should use the current Draft scale, one-quarter width and height, because these cameras start at lower native resolution. Extend the isolated analysis comparison to that grid on all Classic cases. Keep the earlier one-eighth Classic measurements as evidence for why a uniform divisor is unsuitable. The owner clarified that this applies to the complete Classic Thumb preview pipeline. Resolve it at the quality-selection boundary and use the same camera policy during idle preparation. Downstream nodes continue to receive an explicit numerical scale.


The analysis-size experiment completed 65 paired cases across 25 local photographs, including Natural, Strong, exposure changes and Monochrome. Every comparison keeps native Full RGB and its application guide. The smaller branch reconstructs its analysis RGB at the selected working scale. Each case has one cold and three warm alternating Off repetitions, followed by separate synchronized GPU diagnostics. Readback, pixel comparison, PNG export and source opening are outside completed-render timings. Workload monitoring found no competing app or compiler processes.

With Classic analysis at divisor four and Merrill/Quattro at eight, the median absolute relative change in mean linear RGB is about 0.45 percent. Forty-six of 65 cases are within one percent. The median component RMS difference is about 0.00182 on the nominal zero-to-one range. Larger differences remain: SD9 Strong changes mean RGB about 13.7 percent relative to its dark starting value, and hazy-coast Strong at about minus 0.9 EV changes it about 9.5 percent. These are estimates of output change, not errors against a uniquely correct tone response. Whole-image averages can conceal opposing changes in shadows and midtones.

Warm fresh Full processing from resident decoded sources saves a median paired 92 ms on Merrill and 187 ms on Quattro, about 11 and 15 percent respectively. Classic adds about 5 ms because it previously reused the main RGB and guide instead of preparing a separate small branch. Representative settled Exposure edits change only slightly: hazy coast about 39.1 to 37.6 ms, DP0 Quattro about 73.0 to 69.3 ms, and SD14 about 17.7 to 17.6 ms. The fixed 256-by-256 fit still runs, while unchanged analysis preparation is already cached. Existing Thumb render latency is not improved by changing the analysis size of Full.

A further 32 paired cases test divisor-four analysis on all Classic cases and the earlier outliers. Several differences shrink, but a one-percent bound is still not met across the set. A fixed native divisor is therefore not sufficient evidence for adopting one shared automatic analysis target. At the end of that experiment the normal Adaptive policy remained unchanged pending photographic review. The subsequent approved profile policy is recorded below. Temporary experiment hooks have been removed from production source.

Classic Thumb now resolves to divisor four for the complete frontend preview pipeline. Foreground resolution, idle preparation and named-quality profiling use one camera-capability policy. Explicit numeric scale requests remain unchanged. The worker regression covers Classic, Merrill and Quattro resolution as well as native inspection and comparison purposes. The CLI parser regression distinguishes named Thumb from an explicit divisor-eight request.

For hazy-coast Natural, synchronized GPU diagnostics separate the costs: estimator statistics and fitting take about 16.9 to 15.8 ms, analysis RGB and guide preparation about 108 to 18 ms, and the unchanged native application guide about 79 ms. Those GPU diagnostics are distinct from the three-repetition median completed Full render of about 960 to 839 ms. Workspace tests, strict all-target Clippy, the profiling-example parser tests, formatting, generated indexes and artwork tests pass after the Classic quality-policy change.


## Profile-dependent Adaptive analysis

The owner approved Natural analysis at the camera's Thumb scale and Strong analysis at the next finer power-of-two scale. Cap both at the current working resolution. Merrill/Quattro therefore target divisors eight and four, while Classic targets four and two. This is a shared recipe-construction choice. Numerical nodes receive explicit buffers, geometry and scale. Keep Dehaze's existing camera-owned analysis convention unchanged. Replace the old Full-analysis flag and source/result overview split rather than layering another exception over them. Check profile/quality geometry, whole-photo regional analysis, public-mask replay, retained/fresh agreement and photographic/DNG output before rebuilding for review.

Adaptive and Dehaze now share whole-photo RGB preparation with one geometry boundary. Their scale policies remain distinct. The obsolete Full-analysis flag, source/result grid split and analysis-only overview operation have been removed. The profile-grid fixture now checks both rendering modes at native, half, quarter, eighth, sixteenth and sixty-fourth grids, plus supported regional requests. It verifies shared analysis pixels, guide, parameters and fit at matching scales, exact public-input replay, and buffer identity when the main input already has the selected grid.

All 17 serial GPU fixture runs pass without another app or compiler workload. They cover four camera paths, both profiles and rendering modes, retained/fresh agreement across divisors two through sixty-four, regional/public-input replay, Dehaze analysis reuse, guide invalidation, photographic/DNG behavior, combined Monochrome and the speaker defects. The CPU workspace suite also passes the new target/cap policy check.

The native scenario harness also assumed one scale divisor for the entire selection. It now resolves expected quality per photograph through the same camera policy. Trace validation matches accepted and encoded images against each photo's expected scale and reports all group scales. Nominal quality metadata is explicitly labeled. The mixed-scale Python regression passes alongside all 63 profiling-tool tests.

The rebuilt native app passes a four-photo mixed-camera Thumb warm-edit scenario with Natural and Strong recipes. The complete loss-free trace validates initial selection and both edit rounds at per-photo divisors four/eight, accepted delivery, native encoding and settled progress. Workload monitoring records no competing app or compiler. Final GUI tests and strict Clippy pass, and the app bundle is rebuilt for owner review.


## Working-grid Merrill highlights

The owner approved running highlight recovery after downsampling. Move both standard recovery and Color Field after the shared Color/Monochrome sensor reduction. Keep native Full arithmetic and source-coordinate repairs unchanged. Both recovery paths receive area means of original sensor codes and their calibrated numerical inputs. Keep black-level measurements native. Prepare geometry, mapped repair sites and spatial support beside each operation. Validate Full float goldens, lower-resolution clipping/donor controls, both recovery families, retained/fresh rendering and completed first-preparation timing.

Owner review rejected conservative any-native-clipped propagation for rough previews. Judge working-grid reliability from averaged original sensor codes. Both recovery paths receive area means on their requested grid. Standard recovery derives preview clipping at the sensor white levels and does not expand or upload native saturation maps. Full keeps its original immutable saturation evidence and connectivity rules. Remove the maximum-pooling experiment rather than retaining a second preview policy. Small average-brightness changes are not acceptance failures. Review visible artifacts instead.

The average-only candidate passes seventeen serial GPU checks, including native numerical fixtures, clipping controls, both recovery methods with both speckle detectors, Monochrome, Adaptive Tone and source-free/fresh agreement at divisors two through sixty-four. Color Field retains its measured three-value black table alongside reduced image inputs. Recovery and Compact despeckle share each pooled sensor input. Exposure, WB bias, reconstruction and recovery-setting edits remain source-free. Native correction changes still request the original before dispatch.

Nine main Full photographic float comparisons are byte-identical across three Merrill scenes and all three recovery choices. Full Adaptive Tone changes through its small analysis branch. Hazy-coast Natural and Strong change mean sampled RGB by about 0.79 and 0.28 percent. Repeated Full timing is effectively unchanged on that scene.

Twenty-seven before/after preview cases use three resident-decoded scenes, all three recovery choices and Fast/Draft/Thumb. Each process records one cold and three repeated fresh renders with profiling Off. Source opening, float capture and export are outside processing timings. Median completed processing changes from about 355 to 242 ms at Fast, 280 to 150 ms at Draft and 263 to 132 ms at Thumb. Workload monitoring finds no competing app or compiler. These results measure preparation from decoded source, not retained slider edits or complete UI delivery.

Visual review finds colored speckling on the hazy-coast metal despite small whole-image averages. In a broad metal region, 6,545 pooled cells contain clipped native blue samples. None of their means reaches the sensor ceiling. Their median average is about 67 percent of white, with a median clipped fraction about 14 percent. Averaging conceals lost channel information. A small near-white threshold adjustment cannot restore that evidence. The owner requested an explanation of carrying fractional clipping evidence alongside pooled means. That alternative is not implemented or selected yet. Keep photographic acceptance open rather than treating numerical parity and speed as sufficient.

## Clipping-selective pooling experiment

The owner requested visual tests that omit clipped samples or retain the larger clipped/unclipped group within each block. Compare both whole-RGB-pixel grouping and independent channel grouping against ordinary means and native recovery before pooling. Whole-pixel grouping classifies clipping from any original channel reaching its white level, then samples the same selected locations in all layers. Majority ties favor healthy samples. Entirely clipped blocks retain their samples. Existing downstream highlight recovery remains enabled and unchanged. These are isolated experiments, not an adopted resampler or recovery policy.

An isolated processor snapshot produces 48 comparisons across hazy coast, yellow flowers and red flower, using Standard and Color Field Local at Thumb, with additional Fast/Draft hazy-coast comparisons. The control with selection disabled matches the ordinary-mean float output byte-for-byte. Independent CPU calculations verify 305 GPU blocks per candidate, including mixed blocks and replicated padding, within 0.002 camera codes. Production processing source and the normal profiler executable remain unchanged. Workload monitoring finds no competing app or compiler. Experimental source uploads make these runs unsuitable for performance conclusions.

Whole-pixel selection reduces some metal contamination but leaves visible colored speckles. Independent channel selection makes the yellow contamination more pronounced on that scene. Flower continuations appear broadly similar at overview size, which does not establish detail parity. A local viewer supports matched full-frame and detail comparisons. Photographic selection remains with the owner.

The owner also proposed retaining separate healthy/clipped subgroup averages, recovering the clipped group and recombining by subgroup coverage. That proposal requires donor evidence from the healthy group and preservation of usable channels within the clipped group. The current recovery operations assume one shared donor/target field, so this is not merely an outer wrapper. No split-buffer recovery implementation is selected or added by these exclusion tests.

## Split recovery prototype and clipping-classification correction

The owner authorized an isolated split/recover/weighted-merge prototype for Standard and Color Field recovery. They subsequently limited it to genuinely reduced working grids to avoid extra native-size storage. Native Full keeps its existing single-image recovery. Dense buffers remain allocated even where groups are empty. Sparse allocation would need compact records and indexing, rather than zero-filled images.

Inspection exposed an error in the earlier candidate and exclusion tests: digital white was used as the clipping classifier. On hazy coast, original saturation maps also flag red/green samples around 4048 and blue samples around 4045, below the 4095 ceiling. Consequently, some supposedly healthy groups still contained clipped measurements. The earlier exclusion results and their interpretation must not be used to reject those methods. Pooling still conceals clipping, but incorrect classification was an additional cause of the demonstrated color artifacts.

The corrected prototype partitions whole RGB pixels using the original camera saturation maps. It retains separate calibrated subgroup means and coverage. Clipped-channel evidence is preserved independently of those numerical means. Standard recovery takes healthy donor samples separately from its clipped targets. Color Field adds healthy subgroup evidence to its existing working-grid equations. The recovered clipped contribution is recombined with the healthy contribution by coverage. Corrected whole-pixel exclusion and majority controls use the same classification. An ordinary-mean control with preserved clipping evidence isolates the benefit of separation from that of retaining evidence alone.

Ninety photographic comparisons cover three Merrill scenes, all three recovery choices, Thumb on all scenes and additional Fast/Draft hazy-coast cases. The split and mean-plus-evidence controls both remove the obvious metal speckling, so that improvement cannot be attributed to splitting alone. An independent GPU fixture checks group membership and weighted recombination. Native Full bypasses split maps and matches three native float goldens byte-for-byte in the isolated profiler. Production processing source and the ordinary profiler executable remain unchanged. These are visual experiments with additional diagnostic uploads, not representative latency measurements or an adopted processing policy.

The dense calibrated subgroup maps, coverage and flags occupy about 8.7 MiB at Thumb, 32.7 MiB at Draft and 126.8 MiB at Fast on hazy coast. Original-code maps and recovery workspaces are additional. These are logical allocations, not whole-application or peak-memory figures. At Thumb, only 7,219 of 285,120 padded working cells contain both groups. Compact RGB payload would be about 3.35 MiB, before indices and coverage. No compact sparse implementation has been selected.

The owner requested sunlit-red-flower at minus 1.5 EV. All eighteen of its comparisons have been refreshed at that exposure. Independent complete native Full controls for both flower scenes are available in the same viewer. The owner reported red/blue artifacts in yellow flowers, including the older recovery ordering. That older ordering still reduces before downstream processing and is not a complete Full-pipeline control. The artifact location and cause remain under investigation. No corrective production change is selected from these comparisons yet.

## Selected mean-plus-evidence approach

The owner selected ordinary means with original clipping evidence for previews. Separate subgroup storage, two-map recovery and weighted recombination are not selected for the application. Implement and measure the direct mean/evidence path before claiming a latency improvement over split recovery. Its comparison control currently uses experimental split machinery and is not representative of that simpler implementation's cost. Native Full keeps its existing processing and memory ownership.

The yellow-flower investigation reproduces a strong blue outlier at output pixel (461,291) in the 588×392 Thumb render, alongside a red neighbor. Native Full and the camera preview show neutral/yellow measurements in that region. Disabling highlight recovery or row correction leaves the outlier unchanged. Disabling either speckle repair or reconstruction removes the strong blue excursion, identifying a shared reduced-filter interaction.

Working-grid captures trace the initial bad value to Reference speckle repair at pixel (487,312) in the 648×440 padded sensor grid. It changes calibrated layers from approximately (695.54,518.31,351.47) to (695.54,199.51,-282.25). Subsequent reconstruction/color conversion produces approximately (11934.66,-3033.16,-24026.81). Photographic framing spreads the extreme value into neighboring samples, and the tone curve clamps them into visible blue/red artifacts. The final float buffer already contains the anomaly, so PNG conversion and browser display are not its origin.

Preserve this real photograph and the captured neighborhood as a regression case. Repair must be investigated at its reduced-grid detection/estimation boundary rather than hidden by disabling processing or changing highlight recovery. No new repair formula or threshold policy has been selected yet. Other weaker cool-colored samples remain to be assessed separately. The diagnostic captures and matched camera/Full/Thumb comparisons are isolated from application processing.


## Reduced speckle detection

The owner requested scale-aware threshold comparisons and approved bright-only detection on reduced working grids while retaining native Reference behavior. Area averaging reduces independent noise, but the minimum one-pixel neighborhood covers more natural scene structure as scale increases. Treating both effects as the same threshold multiplier fails on the tested photographs.

An isolated comparison covers yellow flowers, the low-light speaker and the parking sign at Fast, Draft and Thumb. Holding native thresholds or increasing them by the square root of the divisor removes the strongest flower excursion, but leaves other visible false repairs and misses a known speaker hot pixel at Thumb. Bright-only detection retains the current noise thresholds, removes the reported flower excursion and repairs both known speaker defects at all three preview sizes in both methods. Synthetic coverage checks all seven bright layer masks and preserves positive and negative dark structure. Nine Compact GPU regressions and the ordered-versus-parallel repair check also pass in the isolated experiment. These comparisons do not establish that native dark detection is unnecessary.

The implementation keeps the distinction inside speckle parameter preparation and detection. Recipes, repair ordering and scheduling remain unchanged. The owner also requested a coverage audit of dark detections, including native Full, before considering whether to remove that native capability. That audit remains investigative.

The dark-detection audit captures original GPU detector marks and repaired samples on eight Merrill photographs at Full, Fast, Draft and Thumb. It excludes scan-margin candidates from photographic counts and ranks paired output crops by local encoded RGB difference. Camera previews provide scene context rather than matching exposure/tone goldens. Instrumented readbacks make the audit unsuitable for latency claims. Native detections include genuine single-layer deficits: one hazy-coast green sample changes from about -73 to 3047, with neighboring greens around 2700–3300. Removing that native repair exposes a bright magenta point. Native dark detection therefore remains useful even when the visible defect is bright. Detector counts alone must not be treated as a count of genuine defects.

The reduced-only implementation passes all sixteen ignored speckle GPU/photograph checks, including ordered native replay, bank capacity, immutable inputs, Compact highlight/repair cases, the new seven-mask bright/dark scale check, the flower regression and known speaker defects at Fast/Draft/Thumb. Six native Full photographic float comparisons are byte-for-byte unchanged with reconstruction enabled. Workspace tests, strict all-target Clippy, formatting, documentation indexes and artwork checks pass.

A matched-location comparison follows the hazy-coast green deficit through all four working scales. Bright-only leaves a small pink residual at Fast, with negligible difference at Draft/Thumb in that crop. Reduced bright-only is a preview tradeoff rather than native-repair equivalence. Its parameter flag participates in the existing node key and adds no image buffer, graph branch or GPU pass. Owner visual review remains pending.

## Remaining native-prefix costs

A current Merrill Thumb measurement on hazy coast and the low-light speaker separates already-resident decoded-source preparation from disk opening and retained edits. Three warmed ordinary Wall renders per configuration give complete fresh-render medians of about 101–128 ms with Reference noise, 206–232 ms with multiscale rows/columns and 273–289 ms with Directional bands. Workload monitoring finds no competing app or compiler. These two photographs are not an application interaction benchmark.

Separate synchronized GPU diagnostics attribute roughly 1 ms to black measurement, 3.4 ms to calibration and 13–33 ms to camera repair before the first sensor pool. Shield correction adds about 5–6 ms when selected. Native multiscale filtering adds about 66 ms, while the native guarded row estimator used by Directional adds about 161 ms. The first area pool costs about 3.9 ms, with a separate sensor-code pool required by recovery. Source upload/submission takes about 27–30 ms of host time. GPU attribution and host observations overlap and must not be added as an end-to-end total. CPU node-encoding timestamps do not indicate when queued GPU work has completed.

Black measurement is therefore inexpensive relative to the remaining native filters. The selected original-clipping-evidence integration is still incomplete. Native shield/row/multiscale separation remains open, and Classic/Quattro still contain native neighborhood/recovery operations before their pool. The bright-only reduced-speckle fix does not complete the cross-family early-reduction plan.

## Completed camera-cluster scheduling

Independent camera-cluster scheduling is integrated and validated. Its implementation and measurements are recorded in the [completed report](./archived/independent-camera-clusters.md).

## Integrate original clipping evidence

The owner approved continuing with processing before versus after downscaling after the independent-cluster integration.

The selected mean-plus-evidence recovery path is now connected. A shared metadata cache owns original clipping flags independently of recovery thresholds and methods. It distinguishes native and reduced grids. Camera runs/index lists are projected directly onto working cells and padded like numerical pooling, without first allocating a native-size flag bitmap. Standard recovery no longer needs a separate pooled original-code image. Both Color Field solvers retain their averaged-code reliability but force flagged channels to zero trust and include original flags in severe-boundary classification. Native Full keeps its original connectivity, equations and processing order. The persistent photographic-preview signature is advanced for the changed reduced output.

Synthetic tests cover run/index decoding, multi-channel unions, partial cells, replicated halos and power-of-two divisors through the one-cell case. The reduced blue-recovery fixture now verifies that a single clipped native sample remains recoverable after averaging below the clipping threshold. A separate Color Field mask test verifies all eight original flag combinations below that threshold. Metadata-cache tests verify shared consumers, threshold reuse, source identity and distinct native/reduced payloads. Standard native fixtures, Color Field independent-CPU equations and boundary tests pass.

An older retained-preview test fails in the frozen pre-change binary by comparing unrotated physical texture dimensions with the oriented view dimensions. Its assertions now distinguish active photographic storage from camera orientation and native inspection/DNG extents. That correction is independent of clipping. The corrected retained/fresh test passes. Fifteen source-free cases cover reset, both Color Field solvers and both solver/Compact combinations at divisors two, eight and sixty-four. Five edits per case match fresh output exactly, including threshold/selectivity changes after source release. All nine Full before/after float comparisons across three scenes and three recovery methods are byte-identical. Twenty-seven Fast/Draft/Thumb photographic comparisons are available for review. Matched Thumb details remove the obvious metal false color in all three recovery choices while preserving the reviewed flower detail. Workspace tests, strict Clippy, formatting, indexes and artwork checks pass.

The owner clarified that the expensive native noise stages should be discussed before changing their signal units, with Full output kept bit-for-bit equivalent. The current cheap calibration pass remains before pooling. It combines constant gains/black subtraction with spatial gain and radial shading. Constant affine terms commute mathematically with averaging, while spatially varying gains generally do not. Camera repair also consumes calibrated values. Splitting calibration remains permitted when it simplifies a demonstrated need, but moving constant gains alone is not a goal. Multiscale and guarded-row adaptation remain open rather than being relocated with inconsistent input units.

The owner requested removal of obsolete test renders after a large capture exhausted local scratch space. Obsolete generated float/TIFF payloads and older traces were removed while keeping original photographs, fixture/source directories, source backups, current review crops, reproducible binaries and numerical reports. Current comparison captures now retain SHA-256 values and PNG review images, then delete their temporary float/TIFF payloads. Roughly 36 GiB of generated payloads were reclaimed across the immediate scratch and wider obsolete-render cleanup.
