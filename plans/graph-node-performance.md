---
summary: "Measured graph and shader experiments track rendering latency, memory use and output equivalence across Merrill and Quattro."
---

# Graph node performance

Measure fresh completed processing with the [render-run benchmark](../docs/profiling.md#render-runs-from-any-revision), process footprint separately with the resource recorder, and visible delivery with the native cache workflow. The MacBook baseline is 9015979 on an Apple M1 Pro. Its headless timings are medians of seven warm samples after two warm-ups with profiling Off and the workload monitor enabled. Recipes use Sunlight and Standard. Compare processing policy and image resolution independently.

## Manual Dehaze

The [Dehaze node](../crates/foveonic/src/nodes/finish/dehaze.rs) now builds the three brightness histograms directly in manual mode. It shares sample quantization and percentile calculation with Auto. On the blue parking sign at Amount 35, full Quality fell from 591 to 193 ms and Draft Quality from 129 to 105 ms. Full Speed measured 231 ms in both runs. All four results, including Draft Speed, matched the original floating-point buffers exactly.

The two-render resource run requested 229 MiB less GPU storage per full Quality render. Physical footprint after the first render was 766 versus 521 MiB. After the second render both measured about 520 MiB. The lifetime peak rounded to 1.3 GiB for both. The benchmark's 100 ms sampler reported 1.25 versus 1.63 GiB at full Quality. The smaller allocation is reproducible. A general process peak reduction is unproven.

## Speed highlight donors

For Merrill Classic highlight restoration, bound the scene samples considered for the 256-bin donor table to 1,048,576 by increasing the sampling step by powers of two on large working grids. Keep the clipping evidence and correction at the requested resolution. The [processing quality criteria](../docs/specs/processing-quality.md) require visual review. The owner prefers Speed to look about 95–99% similar to Quality and permits larger numerical differences where they have little visible effect.

| Native full Speed, ms | Baseline | Sampled donors |
| --- | ---: | ---: |
| Blue parking sign | 177 | 168 |
| Low-light speaker and desk | 190 | 167 |
| Hazy coast | 209 | 171 |
| Sunlit metal latch | 264 | 174 |
| Sunlit red flower | 286 | 170 |

The coast, latch and flower medians moved outside their original sample ranges. The Quality floats matched exactly on all five. The combined review build reproduced the isolated experiment's pixels in all ten full-resolution cases.

Compare the visible crop in SDR sRGB after converting image primaries and applying the sRGB transfer function. Across the five native renders at least 99.986% of visible pixels matched the old Speed render exactly at eight bits per channel. The largest channel change was 182 codes in a small metal reflection. On the flower and speaker the largest change was one code. A synthetic periodic donor pattern can change blue substantially, so these five images do not establish a universal similarity bound.

In the native two-photo cache workflow, the latch and flower began fitted at divisor four, then zoomed to 200% in Speed. Across three fresh launches per build, median time to admit the first native patch fell from 764 to 467 ms and both patches from 1,504 to 808 ms. The workload keys matched with a 2 GiB memory budget, disabled disk cache and a 1440 × 1000 point window at 2×. The 100 ms process sampler observed peaks of 2,076–2,087 MiB on the baseline and 1,804–1,967 MiB on the candidate. In a separate run, both final displayed images matched fresh renders. Physical screen presentation was outside the measurement.

## Cleanup verification

The owner visually reviewed the cleaned Mac build with the test photographs and accepted the rendering results. On the sunlit metal latch, the owner could see a difference between Speed and Quality and judged both renderings equally good. The cleaned renderer matched the measured candidate on all 14 MacBook float comparisons. The ten full-resolution policy cases and four manual Dehaze cases recorded no timing change beyond their sample spread. Core tests, highlight and Dehaze GPU tests, and original-file checks passed, including Auto-to-manual freezing and cached and export paths.

On the Windows RTX 3090, seven warm samples per case gave 496 to 126 ms for full Quality manual Dehaze, 158 to 110 ms for Speed on the latch, and 172 to 110 ms for Speed on the flower. The Quality latch control measured 158 ms in both builds. All four cleaned outputs matched the pre-cleanup candidate byte for byte on that adapter. In a separate Speed latch run, peak process commit fell from 2,231 to 1,961 MiB.

On Windows Intel UHD 630, full Quality manual Dehaze fell from 2,350 to 1,667 ms and Draft Quality from 478 to 449 ms. Both outputs matched the baseline hashes. Speed fell from 1,854 to 1,498 ms on the latch and 1,798 to 1,506 ms on the flower. The full Quality latch control rose from 1,741 to 1,793 ms (3%) with identical pixels and allocation traffic. The Speed latch float buffer matched the pre-cleanup candidate exactly. In separate full Quality Dehaze runs the peak working set was about 2,105 MiB in both builds, despite 229 MiB less requested GPU storage. Three other Foveonic GUI processes remained open on this machine throughout, so treat its latency comparisons as exploratory.

## Adaptive Tone at reduced Fit resolutions

The [adaptive recipe](../crates/foveonic/src/render/recipes/linear_edits.rs) previously built a second camera RGB input when Speed requested a smaller whole-photo analysis grid. Two experimental renderers used the existing post-Dehaze, pre-tone RGB instead. One reused the working RGB and its brightness guide directly with Speed's existing 256 × 256 estimator. The other used the existing GPU area mean once to produce the selected analysis dimensions, then prepared its guide. The experimental change applied only to reduced working grids. The [recipe preparation path](../crates/foveonic/src/render/recipes/mod.rs) resolves reduced regional requests to whole-photo renders.

Apple M1 Pro fresh-processing medians in milliseconds with profiling off, after two warm-ups. The leaves used its saved Natural recipe. The latch exercised Strong and the coast exercised Natural with manual Dehaze at 35. The leaves had seven timed samples per build and the other photographs had five.

| Photograph and working scale | Current Speed | Reuse RGB and guide | One area reduction | Quality |
|---|---:|---:|---:|---:|
| Glossy leaves and buds, half | 224 | 195 | 195 | 215 |
| Glossy leaves and buds, quarter | 150 | 129 | 125 | 141 |
| Sunlit metal latch, half | 252 | 207 | 210 | 250 |
| Hazy coast, quarter | 148 | 126 | 128 | 148 |

Compare final visible pixels after conversion to 8-bit SDR sRGB. The one-pass reduction kept every channel within one code of existing Speed across the four cases. On the leaves, direct reuse brought Fit within one code of Quality but changed some channels by up to seven codes from existing Speed. To estimate the Fit-to-native appearance change, area-average the active part of the current native Speed render to Fit dimensions, convert both to SDR and average the largest absolute channel difference per pixel. At half resolution this was 1.12 codes for current Speed, 1.14 for one area reduction and 2.80 for direct reuse. These comparisons cover complete buffers. Visible time to deliver the first native patch after Fit still needs a GUI measurement.

An unconditional 2× reduction changed half-resolution leaves by up to five SDR codes from current Speed. At quarter resolution the Strong latch already shared the working guide. Halving it again introduced 39 dispatches and raised the observed median from 132 to 136 ms. A node should be able to request a factor of one.

The implemented recipe uses the existing GPU area mean for the requested reduction relative to working RGB. The mean starts at the valid photo origin and counts the covered pixels in the final cells. The guide radius and structure probes use the actual working sampling divided by the reduction factor. The recipe reuses the application guide when the grids coincide. An original-file replay check verifies that the pooled analysis consumes the exact pre-tone RGB supplied to the response, including upstream white balance and Dehaze. Camera-reconstructed overviews and pooled RGB keep their distinct cache identities.

The bounded implementation measured 197 and 126 ms for half and quarter leaves, 211 ms for the half latch and 129 ms for the quarter coast under the conditions above. The largest channel change from previous Speed was two 8-bit SDR codes across the four comparisons. On the leaves the two reduced Quality controls and the native Speed control retained identical float-buffer hashes. The reduced leaves renders each requested about 71 MiB less cumulative GPU storage.

In three uncontended Speed GUI runs on the M1 Pro, initial two-photo Fit completion fell from a median 778 to 697 ms. After quarter-resolution Fit, accepting the first native patch on the leaves took 1,153 versus 1,167 ms, with overlapping ranges. The later pan test exceeded its 520 ms target in both builds (578 versus 595 ms median). An independent replay of both final candidate Fit captures matched the fresh float buffers and metadata exactly. Running the image comparison even when a later timing assertion fails would preserve both benchmark results in a single report.

On the Intel UHD 630, seven warm samples of the leaves measured 2,402 to 2,239 ms at half Speed and 996 to 835 ms at quarter Speed. Both sample ranges separated. Half Quality measured 2,526 to 2,512 ms with overlapping ranges and an identical SHA-256 digest. Three other GUI processes were present throughout.

All 696 enabled core tests and nine pooling tests passed. Nine self-contained highlight restoration checks passed. Two research comparisons required unavailable external fixtures. Original-file replay passed on two Merrill photographs, the Classic DP1 and the DP2 Quattro. The Merrill cold-patch, pan and whole-photo continuation checks passed. An additional reduced render produced the requested 853 × 1279 portrait view. Formatting, Clippy and documentation checks passed.

## First native patch and pan

At commit `12ea274`, replay the Speed cache workflow on the M1 Pro with the leaves and latch, a 2 GiB working set, disk cache disabled and the leaves initially at quarter resolution. Zoom the leaves to 200%. The first patch covers 550 × 986 native pixels including overscan. After four exposure updates, the drag moves that patch 300 native pixels horizontally without a further edit. The pan milestone starts at the last pointer movement and includes the configured 120 ms settling delay. Zoom ends at UI acceptance. Pan ends when the app observes the accepted current patch.

The Wall trace shows a 130 ms native source load at first zoom. Its graph executed 56 nodes and 354 dispatches with no retained graph outputs available. Measured GPU passes accounted for 160 ms on the patch input, 31 ms on the eighth-resolution analysis branch, 103 ms on the half-resolution application-guide branch, and 24 ms on Adaptive Tone and finishing. Within the two overview branches, the brightness filters themselves accounted for 3 and 19 ms respectively. The other passes produced their input RGB.

On the pan, the graph reused the despeckled sensor layers, scene-analysis RGB and guide, and the application guide. It ran 16 nodes and 59 dispatches. Adaptive Tone repeated the scene fit. Its combined operation took 38 ms of measured GPU passes, compared with 36 ms for Banding and 18 ms for NoiseBase. The Wall trace and GPU timestamps came from independent runs. Node GPU durations exclude copies, unmeasured work and other queue users.

Timed diagnostic executables kept the same render graph and changed the number of required operations a native patch could encode before waiting. Three fresh application launches per setting used identical recipes, display geometry and workload keys on an uncontended host. With one operation per tick the render worker spent a median 908 ms inside tick completion on first zoom and 404 ms on pan. These waits include actual GPU execution. Polling, scheduling and other work sharing the queue cannot be separated from that number.

| Required patch operations per tick | First native patch, median ms | Pan, median ms | Pan trials within 520 ms |
|---|---:|---:|---:|
| 1 (current) | 1170 | 578 | 0 of 3 |
| 2 | 1115 | 516 | 2 of 3 |
| 4 | 1015 | 419 | 3 of 3 |

Four operations per tick reduced median time inside tick completion to 715 ms at zoom and 217 ms at pan. The median of the longest worker tick during each pan rose from 87 ms with one operation to 121 ms with two and 166 ms with four. A separate pair of process-memory runs per setting sampled physical footprint every 100 ms across the workflow. The observed peaks were 1,962–2,035 MiB for one operation, 1,955–2,026 MiB for two and 1,882–1,900 MiB for four. Sampling can miss transient peaks.

The experimental group sizes change the [scheduler's bound on unfinished work](../docs/specs/render-scheduling.md). Before selecting a production policy, measure the time from interruption to delivery of a replacement image during rapid pans and compare memory under pressure. The current scheduler uses one required operation per tick. For future traces, record the task identity when a resumable graph starts and the elapsed tick-completion wait. The existing report needed a join by graph ID and action interval for the two patch requests, and the wait durations needed a diagnostic build.

## Fixed patch operations per tick

Compare caps of one, two and four required graph operations per scheduler tick on native patches at `12ea274`. Whole-image jobs keep their usual step size. Each Mac trial starts the native cache workflow in a fresh process on the 32 GiB M1 Pro with profiling Off, disk cache disabled, two photos and a 1440 × 1000 point window at 2×. Rotate cap order within blocks. The app's working-set target is separate from physical RAM. The pan endpoint is a current covering patch in the app, measured from the last movement including the 120 ms debounce.

| MacBook Speed workflow | Fresh launches per cap | One, ms | Two, ms | Four, ms |
|---|---:|---:|---:|---:|
| Leaves first native patch, 2 GiB target | 6 | 1193 | 1115 | 1012 |
| Leaves settled pan, 2 GiB target | 6 | 555 | 508 | 438 |
| Latch settled pan, 2 GiB target | 3 | 280 | 256 | 211 |
| Leaves settled pan, 768 MiB target | 3 | 638 | 650 | 496 |
| Both native patches accepted after zoom, 2 GiB target | 3 | 1510 | 1439 | 1376 |

All values are medians. Every ordinary trial completed the workflow without detected competing workloads. On the leaves at 2 GiB, one, two and four met the 520 ms pan target in zero, five and six of six launches respectively. Across the twelve single-zoom blocks, four beat both other caps on every pan. The smaller cap offered little consistency at 768 MiB. With one photo zoomed at 2 GiB, the median warm group exposure update was 143, 150 and 152 ms respectively. The settled pan improvement therefore does not generalize to every action within the workflow. The ordinary comparison used the earlier binaries that log each worker tick. Three separate launches of the uninstrumented one-operation build gave a 570 ms median pan on the leaves.

### Replacement after a second pan

A separate diagnostic build uses one binary for all caps and records events in memory. Move the viewport again after the first patch submits GPU work. Keep the pointer held until a newly accepted patch covers the second view. Repeat the sequence in the return direction. Compare the two destinations separately, since the second returns to an area visited earlier in the same process. Before interruption, the same binary reproduced the ordinary leaves pan at 556, 495 and 423 ms across the three caps.

| Leaves at 2 GiB, six Speed launches per cap | One, ms | Two, ms | Four, ms |
|---|---:|---:|---:|
| First replacement, viewport movement to UI acceptance | 1023 | 1030 | 917 |
| First replacement, settled view to first worker graph step | 240 | 188 | 227 |
| Return replacement, viewport movement to UI acceptance | 755 | 822 | 774 |
| Return replacement, settled view to first worker graph step | 106 | 214 | 276 |

The earlier graph was still active at settlement in all 36 replacements. Four beat one on the first replacement in six of six blocks. On the return it lost in five of six. Its longer wait to start partly offset its faster execution. Four of six older graphs completed successfully on that return with cap four, versus zero of six with one. Region, coverage, worker steps and UI admissions were checked independently for each replacement. The worker interval includes scheduler work and retirement of prior submissions.

Repeating the replacement test with the diagnostic driver explicitly selecting Quality over three launches per cap gave first replacement medians of 989, 996 and 922 ms. On the return the medians were 921, 979 and 942 ms. The benefit again depended on the destination. Separate readback runs produced byte-identical final Speed patch buffers across all three caps on the Mac.

### Physical and logical memory

Run memory separately from latency. The Mac recorder samples the owned process's physical footprint every 100 ms and reads the OS lifetime maximum at each sample. In three launches per cap and target after two interrupted pairs, the end-of-hold logical working set was 765.1 MiB at 2 GiB and 710.9 MiB at 768 MiB for every cap. Median lifetime process high water at 2 GiB was 2087, 2073 and 2071 MiB for one, two and four. At 768 MiB it was 2083, 2074 and 2069 MiB. Within the pan interval at 2 GiB, sampled physical peaks were 1816, 1800 and 1874 MiB. The whole-workflow peak can therefore conceal a smaller increase during panning.

The OS still released memory during the initial 1.8 s hold. Run eight interrupted pairs in a fresh process and hold the final native view idle for 7.5 s. Use the median physical footprint over the final two seconds of each launch, then the midpoint of two launches per setting. At 2 GiB, physical footprint was 1357, 1374 and 1405 MiB for one, two and four. At 768 MiB it was 1316, 1308 and 1337 MiB. Four was above one in each matched block. The two cap-four trials at 2 GiB were 1383 and 1427 MiB, so the size of that difference remains uncertain. Logical bytes remained the same across caps at both budgets.

### Intel integrated graphics

Use the logged-in Windows desktop with the Intel UHD 630, 16 GB of RAM and DX12. The same window size is 1× on this host. It selected one-eighth Fit and a 340 × 558 native patch. The Mac selected quarter Fit and a 550 × 986 patch. Compare caps within each host. Three existing GUI processes stayed open on Intel. Each trial records their activity.

In three fresh Speed launches per cap at a 2 GiB target, ordinary pan medians were 2319, 2318 and 2305 ms. The trial ranges overlapped. First zoom was 5705, 5639 and 5541 ms. A separate interrupted pan queued a second movement after the first submitted work. The replacement reached UI acceptance in 2946, 2951 and 3380 ms from enqueue, or 2754, 2698 and 3033 ms from input consumption. All nine delivered the final region. GPU setup time precedes these milestones. Separate readbacks of the settled pan produced identical native float buffers at all three caps on Intel.

Separate Windows memory trials sampled private bytes during a 1.5 s hold on the settled native patch. At a 2 GiB target, median settled private bytes were 2022, 2091 and 2027 MiB. Sampled peak private bytes were 2320, 2461 and 2464 MiB. At a 768 MiB target, median settled private bytes were 2080, 2024 and 2343 MiB. The corresponding WDDM shared GPU usage was 1742, 1685 and 1940 MiB. The counters overlap. Three launches per cap and target gave substantial spread, including overlapping settled ranges for one and four. At the tighter target four retained 8.6 MiB more logical memory on return to Fit than one. The ordinary pan on this Windows workload gained 14 ms at four versus one.

### Implications for scheduling and measurement

Four is worth investigating further for settled MacBook patches. These results do not establish a common cap for both hosts, a reliable compromise at two, or a need for per-node allocation declarations. The immediate question is whether the Mac can keep its settled-pan gain while shortening the delay when a newer patch is ready.

The interruption probe records the input, settled request, first graph step and UI acceptance against the actual region. Keeping those milestones in the maintained workflow would expose slow replacement even when ordinary pan delivery improves. Pair a native-view idle marker with the OS footprint sampler and a lifetime high-water counter. Report timed assertions separately from image checks so a missed target preserves the numerical result.

## Patch handoff during panning

Test whether the new patch can start sooner while preserving sharp detail as the view moves. Use the same MacBook, recipes, quarter Fit and native viewport geometry. Compare one operation per tick, four with the existing one-tick lookahead, four with a wait for the current batch, and four with an interruption check before making a display texture for a superseded patch. The last option preserves the graph's completed checkpoints. Keep every change in the diagnostic build.

In four counterbalanced Speed launches per option on the leaves, waiting for the current batch reduced the median first replacement's delay from settlement to worker start from 247 ms with ordinary four to 35 ms. Movement to fresh patch acceptance nevertheless increased from 975 to 1018 ms. In the return direction the wait fell from 254 to 114 ms and acceptance took 786 versus 827 ms. The ordinary pan increased from 460 to 551 ms. Waiting more often gave up most of the throughput benefit. Skipping obsolete display delivery brought the median return wait down to 171 ms and the accepted replacement to 782 ms. The one-operation control delivered that return in 751 ms.

A separate 1.94 s scripted drag uses elapsed time to move, reverse into overscan, pause, change direction and release. Its timing is independent of rendering progress. Count the proportion of the visible viewport covered by the selected native patch on each app frame, weighted by time. The existing Fit image fills the remainder. Record coverage during movement and from release to acceptance of the final covering patch. Application-frame observations do not establish physical screen presentation.

| Background-window leaves drag, four processes per option | Last movement to current patch, ms | Mean native area during drag | Mean native area waiting after release |
|---|---:|---:|---:|
| One operation | 2479 | 62% | 3% |
| Four operations | 2425 | 78% | 67% |
| Four, discard obsolete display | 2005 | 62% | 3% |
| Four, wait for the current batch | 2288 | 62% | 3% |

The table gives medians across launches. The discarded patches contained detail the viewport could still use. To account for different wait lengths, integrate the fraction of the viewport showing the lower-resolution overview from the start of motion until the final patch arrives. Express that as the equivalent time the whole viewport would have shown only the overview. It was 3.15 s with one, 1.23 s with four, 2.69 s with discarded obsolete displays and 2.97 s when waiting for each batch.

On the latch, three background-window launches per option gave final pan medians of 1866, 1179, 1217 and 1528 ms respectively. The discard check never fired on the latch. On Intel at 1×, ordinary pan medians were 2336, 2309, 2297 and 2336 ms in three launches per option. The longest observed app-frame gap reached 1009 ms and none of the twelve timed gestures visited all the short stops. Each reached the final rectangle, but their delivery times do not compare identical movement histories. The discard check never fired in the Intel timed trials or in the two controlled replacements that exercised that option. In the controlled return the prior graph had already finished by settlement for all four options.

In a foreground repeat, test a fourth way to handle an obsolete display. Submit its texture conversion to the GPU queue shared with the UI, then let the worker proceed without waiting for that obsolete texture. Current results still take the original GPU wait. Record a completion callback for every patch texture and credit native coverage only after its callback. This counts completion conservatively and keeps early UI acceptance of unfinished textures out of the result.

Across four launches per option, final foreground delivery was 2624 ms with one operation, 2346 ms with four, 2061 ms with discarded obsolete textures and 2035 ms when the worker continued after submitting an obsolete texture. Native detail covered 3%, 68%, 3% and 39% of the viewport respectively while waiting after release. The shared-queue option kept one of the two obsolete images in use per launch. Relative to ordinary four it shortened delivery by 310 ms and increased the time-integrated lower-resolution area from 1.17 to 1.98 equivalent whole-viewport seconds. Every current final texture had completed on the GPU before UI acceptance.

Separate final-patch readbacks after two controlled replacements produced the same native float-buffer hash in all six launches across one, four and the shared-queue option. One readback exercised the early worker release. In separate 2 GiB memory runs, all four processes retained 765 MiB of logical graph state after two controlled replacements. Median physical footprints over the final two seconds of a 7.5 s hold were 1397 and 1412 MiB with four, and 1399 and 1545 MiB with the shared-queue option. Footprint still varied by about 88 MiB within each sampling window. Neither shared-queue memory run exercised the early release, so these readings only describe baseline variation.

### Panning with background completion enabled

The patch-only workflow disables full-image completion for the final edit. Restore the normal enabled preference and repeat the foreground gesture immediately following the edit workflow. No launch had a current whole native image at the start or end of the timed drag. The table reports three launches per setting. Time starts at release and ends at the first observed frame covering the final view with GPU-ready native pixels.

| App memory target | Patch strategy | Final wait | Native area while waiting |
|---|---|---:|---:|
| 2 GiB | One | 3694 ms | 3% |
| 2 GiB | Four | 3194 ms | 36.5% |
| 2 GiB | Four, worker continues after obsolete texture submission | 3006 ms | 3% |
| 768 MiB | One | 2601 ms | 3% |
| 768 MiB | Four | 2418 ms | 67% |
| 768 MiB | Four, worker continues after obsolete texture submission | 2150 ms | 40% |

At 2 GiB only one of the three shared-queue launches took the alternative delivery path. At 768 MiB it ran twice in every launch. Fourteen of the eighteen ordinary pans preceding the timed gesture missed the existing 520 ms target, including all nine at 2 GiB. The difference between budgets merits separate investigation of background work if panning after an edit becomes a priority.

Allow two seconds of quiet after the last edit's visible result, then make the same ordinary pan and timed drag. At 2 GiB the full native image was ready before motion in all four launches, two each with one and four operations. Native coverage stayed at 100% throughout. The timed drag submitted zero new patches and the app observed the final view approximately 2 ms after release. The maintained workflow flags the missing new patch in all four runs because its usual fixture disables background completion. At 768 MiB all four launches still used patches and submitted three during the timed gesture. The median final wait was 2765 ms with one and 2479 ms with four. Native detail covered 3% and 67% respectively during that wait.

Keep the existing production handoff. Early release helped the final patch arrive sooner in the tight-budget Mac test, but users panning through overlapping regions would spend longer looking at the Fit image. After background completion the handoff had no work to do. A simpler candidate for future testing is ordinary batches of four on Mac patch renders. Intel latency and physical memory still matter before changing the default. For the maintained panning benchmark, record the fraction of the viewport covered by ready native pixels over time, count full-image reuse as successful coverage, and keep a separate patch-only case for cancellation.

## Reducing GPU submissions inside nodes

### Reusing the whole-photo Adaptive Tone fit

A native patch needs the fitted scene response and the local gain application. The current [Adaptive Tone operation](../crates/foveonic/src/nodes/finish/adaptive_tone.rs) produces both together. Moving the patch changes its graph key even when the upstream whole RGB and brightness guide are still resident. In a traced pan, 37.8 of the operation’s 38.1 ms of GPU pass time went to the scene fit.

Use a diagnostic lookup to establish when the same fit serves a new patch. Match whole RGB and guide allocation identities and write generations, analysis geometry, tone curve and fitting controls. Keep the same lookup overhead in both arms. A hit retains three five-float responses on the GPU. The application still runs at the requested patch geometry. A production version could expose a scene-fit result to the existing session graph as Dehaze does for its estimate.

On the additional M1 Pro with 16 GiB, time the native request through GPU completion for five different warm 551 × 987 patches of Glossy Leaves and Buds. Two uncontended launches per policy and arm contributed ten timed natural-profile pans. Check the entire native float buffer outside timing. Each exposure or profile change recalculated the fit. The aligned outputs across all ten pan and edit states matched byte for byte within each policy.

| Natural Adaptive Tone | Completed patch | GPU dispatches | Queue submissions | Reduction in temporary storage requested per hit |
|---|---:|---:|---:|---:|
| Speed | 114 → 99 ms | 63 → 48 | 66 → 49 | 1.1 MiB |
| Quality | 119 → 65 ms | 51 → 36 | 54 → 37 | 225.2 MiB |

Quality analyzes every pixel in the whole-image analysis grid. Speed uses a 256 × 256 sample grid. The retained numerical payload is 60 bytes in either mode. The avoided GPU storage requests are cumulative allocation counters for each render. In two separate uncontended pairs on the 16 GiB Mac, sampled pan peak was 1452.4 versus 1452.5 MiB and median final-idle footprint was 950.9 versus 951.0 MiB. Measured session-cache GPU memory was 580.58 MiB in both.

The foreground GUI on the primary MacBook used a 2 GiB app target and one operation per tick. In four Speed process pairs, ordinary pan delivery improved from 576 to 526 ms. The fit matched in every pan. The existing 520 ms target was missed in four of four control runs and two of four reuse runs. A second cohort initialized the preference to Quality but the scenario at the measured revision forced Speed at startup. Its pan improved from 569 to 502 ms. Count this second cohort as Speed. The benchmark fix removes this override and checks the delivered frame policy against the declared request. Earlier reports lack enough intermediate evidence to certify the measured policy. Fresh maintained GUI trials with the latch first and the leaves second captured 44 frame observations per policy, and all current frames had the requested policy. Both final photos in each run matched independent fresh-render readbacks. The measured pan policy now participates in the comparison key.

With the actual GUI policy verified at the drag and dense analysis verified in the shader parameters, four matched Quality pairs brought median pan delivery from 619 to 466 ms. Every control missed the 520 ms target and every reuse run met it. First zoom was 2021 and 2025 ms respectively. With normal background completion enabled, the pan immediately after editing still needed a patch in all six further runs. Median Quality delivery in those three pairs improved from 1364 to 1227 ms. All six missed the 520 ms target. On Intel UHD 630, two launches per arm completed the five warm Quality pans in a median 2341 versus 1635 ms headless. All forty Intel outputs matched across corresponding pan and edit states. These comparisons cover one recipe and foreground work after an edit. A graph-owned implementation adds a fit operation on the first render and should be measured for cold latency separately.

### Reusing Detail blur maps during Amount edits

Merrill Color Detail extracts a luminance residual and runs eight blur passes. Fine and Coarse Amount enter only in the final mix. A diagnostic build retained the two blur maps for each working grid over successive Amount edits. It matched input buffer identities and write generations, tile parameters, Luma Noise strength and blur sizes. The existing renderer and the candidate ran in the same release binary. The graph retained the same number of GPU bytes in both arms. The diagnostic store held up to three additional grids outside the graph budget. A production graph would have to charge and evict these maps.

On the 32 GiB M1 Pro MacBook, render Glossy Leaves and Buds with Natural Adaptive Tone. Keep the view fixed and alternate six distinct Fine and Coarse Amount edits after the first non-neutral render. Each cell below is the median of four fresh process medians with counterbalanced launch order. Timing ends at GPU completion. Fit uses divisor four (784 × 1176), the native patch is 551 × 987, and the native image is 3136 × 4704.

| View | Quality render, ms | Quality maps, MiB | Speed render, ms | Speed maps, MiB |
|---|---:|---:|---:|---:|
| Quarter Fit | 47.5 → 45.8 | 9.0 | 31.2 → 28.4 | 9.0 |
| Native patch | 240.7 → 190.5 | 195.9 | 73.9 → 59.1 | 75.9 |
| Whole native | 234.4 → 197.4 | 156.7 | 189.7 → 142.3 | 193.4 |

A hit skips nine GPU submissions per grid. The earlier pan trace’s 19.6 MiB covered two maps for one padded tile window. This patch crossed more tile boundaries and used 39.2 MiB locally. Quality also retained 156.7 MiB for the whole-image analysis. Speed used two smaller analysis grids totaling 36.7 MiB. On the first warm patch edit, the regional maps hit. The engine recomputed the whole-image inputs in new GPU buffers, so their maps missed. The remaining five edits reused every grid. Returning both Amounts to neutral released the maps in these fixed-view runs. All 78 paired output buffers matched byte for byte, including edits to Fine Size, Coarse Size and Luma Noise that forced recomputation.

Measure physical footprint in three separate process pairs per view and policy. Sample every 10 ms through six edits, keep the result open for 7.5 seconds and take the median of the final two seconds. The edit peak is the largest sampled total process footprint during the six warm edits, including retained maps. The table reports median paired differences. Full-native Quality’s settled cost varied from +112 to +157 MiB across pairs and its sampled peak increase ranged from +60 to +299 MiB. In Speed Fit the three settled differences were +9.9, +10.0 and −45.1 MiB with the same 9.0 MiB of maps. Allocator and driver retention also affect the physical readings.

| View | Quality settled change, MiB | Quality edit peak change, MiB | Speed settled change, MiB | Speed edit peak change, MiB |
|---|---:|---:|---:|---:|
| Quarter Fit | +9.6 | +7.6 | +9.9 | +10.7 |
| Native patch | +195.6 | +196.1 | +75.8 | +75.7 |
| Whole native | +112.5 | +219.0 | +191.7 | +227.5 |

Drag the real Fine Amount control from 1.12 to 1.77 with Coarse at its default. Measure from release to the first app frame drawing the edited image across the viewport. A 1440 × 1000 point window at 2× selected half-resolution Fit and a 992 × 1010 native patch at 200% zoom. The table gives medians by arm on the primary MacBook. All six native pairs improved. Speed Fit improved in three of four pairs. The GUI and headless cases use different geometries, and physical memory was measured only in the headless cases.

| Primary GUI | Pairs per view | Fit, ms | 200% zoom, ms |
|---|---:|---:|---:|
| Quality | 2 | 1013 → 931 | 1988 → 1907 |
| Speed | 4 | 904 → 852 | 1207 → 1139 |

On a second M1 Pro with 16 GiB, three Quality pairs gave 134 → 177 ms in Fit and 725 → 691 ms at 200%. Both views gave mixed results across pairs. In two additional Fit pairs starting with neutral Fine Amount, the release wait fell from 269 to 100 ms. The scenario sends one pointer movement per app frame, so gesture duration can vary. Separate Quality readbacks of the actual GUI image matched across builds before and after the drag at both zooms.

The owner declined the two-map cache given its settled cost. For the maintained slider benchmark, include the drawn layer, working divisor and visible native bounds in the workload identifier, and record release to the first covering edited frame.

### Skipping the broad blur at neutral Coarse Amount

When Merrill Color Detail’s Coarse Amount is 1, the broadest blur cancels out of the final mix. The node now prepares the number of blur steps once. Fine-only edits end the loop at the Fine boundary and bind its output in both map slots. This saves four dispatches on every evaluated Detail grid and needs no retained maps. Coarse Amounts other than 1 use the full filter. Two neutral Amounts already bypass the spatial filter.

Render Glossy Leaves and Buds on the 32 GiB M1 Pro with the same Natural Adaptive Tone recipe and geometries as the two-map experiment. Start neutral, change Fine to 1.12, then make five more Fine edits above and below 1, including zero. Time through GPU completion with readbacks disabled. The table gives the median paired saving over four counterbalanced process pairs per view and policy. The later-edit result uses the median of five edits within each process.

| View | Quality first Fine edit | Quality later edits | Speed first Fine edit | Speed later edits |
|---|---:|---:|---:|---:|
| Quarter Fit | 0.7 ms | 1.1 ms | 0.2 ms | 1.3 ms |
| Native patch | 23.7 ms | 21.0 ms | 7.2 ms | 6.9 ms |
| Whole native | 21.0 ms | 14.0 ms | 23.1 ms | 23.6 ms |

The whole-native Quality edit requested 313.4 MiB less temporary GPU storage and Speed requested 386.9 MiB less. These counters sum allocation requests over a render. Graph checkpoint bytes matched at every edit in every pair.

In separate physical-memory trials, hold the final edit open for 7.5 seconds. The settled process footprint fell by 18–37 MiB at full-native Quality, about 38 MiB on the Quality patch and about 10 MiB on the Speed patch across two pairs each. Full-native Speed went the other way. The four paired differences were +39, +38, +2 and +40 MiB. Both arms had similar footprints before editing. In the two extended trials the final footprint stayed at its 7.5-second level through 25 seconds. Quarter-resolution Speed Fit settled within 1.2 MiB of baseline in two pairs. The shortcut adds no persistent graph allocation. Full-native Speed frequently left more physical memory in use despite comparable neutral starting footprints.

Quality Fit had two distinct starting footprints with the same neutral render, GPU work and graph checkpoint sizes. All four processes that started at 381–382 MiB settled at 388–394 MiB after the Fine edits. All four that started at 398–400 MiB settled at 445–447 MiB. Both implementations appeared in both groups. Three paired comparisons put a higher-starting shortcut process against a lower-starting control. The fourth reversed that pairing. The resulting median increase of 13.1% is a poor estimate of the shortcut’s cost. Within a starting group the data include only one control or one shortcut run. The responsible allocation and any smaller effect remain unresolved.

Repeat the quarter-resolution Quality Fit test headlessly on the same 32 GiB M1 Pro. In all 35 fresh launches the same binary rendered neutral first. Measure the last 0.8 seconds of the neutral hold, then randomly assign two baseline and two shortcut runs per block within each 6 MiB starting-footprint band. Two baseline labels run identical code to measure run-to-run variation. Each process makes the same six Fine edits and holds the final result for 7.5 seconds. All launches passed the workload guard. The eight complete blocks cover 32 launches.

Mean settled footprint in the complete blocks was 423.4 MiB on the baseline and 421.8 MiB with the shortcut. The mean within-block change was −1.6 MiB (−0.37%), with an approximate 95% interval of −23.1 to +19.9 MiB (−5.4% to +4.7%). Subtracting each process’s neutral footprint gave −1.2 MiB. Including incomplete blocks where both paths occurred gave −1.4 MiB across 34 launches. The sampled edit peak changed by +1.5 MiB. Each Fine edit saved four GPU submissions, and retained graph bytes matched at every step.

Two baseline runs within one starting band could settle 54 MiB apart in either direction. Six of 35 launches departed from the earlier low-to-low or high-to-high pattern. Seventeen released at least 32 MiB after the last edit completed. This experiment gives no evidence of a 13% average settled-memory penalty for Quality Fit. A smaller effect remains possible, and the process measurements cannot identify the owner of the released memory.

Repeat the same workflow on the Intel UHD 630 with 16 GiB, using four counterbalanced pairs per view. Quarter Fit in Quality went from 354 to 320 ms for the first edit and 354 to 324 ms for later edits. At native patch resolution in Speed the first edit went from 2628 to 2390 ms and later edits from 1213 to 976 ms. All eight pairs improved. Three existing GUI processes stayed open on the Intel host during both arms. In separate Intel readbacks, 21 pixel observations changed across the six Fine edits in Fit and 22 across the six patch edits. Each change was one SDR code, and all eight neutral or non-neutral-Coarse control buffers were identical.

Compare all visible pixels on the leaves in each geometry, plus whole-native renders of the low-light speaker and sunlit latch, in both policies. All 40 control comparisons with neutral Fine and Coarse or with non-neutral Coarse matched the entire output buffer byte for byte. Across 60 Fine-only comparisons, 3,741 of 548,652,636 visible pixel observations changed after conversion to 8-bit SDR sRGB. The maximum change was one code per channel. The largest affected fraction in one frame was 17 of 543,837 pixels (0.00313%). The floating-point output can differ because the mix adds and subtracts different intermediate values. Eleven focused texture tests passed on Metal and DirectX 12, including the brightness-change inspection at neutral Coarse.

Drag the real Fine slider from neutral at default Coarse. On the primary MacBook, two counterbalanced background-window pairs per case measured release to the first edited frame covering the viewport. Speed at 200% moved from 1271 to 1139 ms, with paired savings of 150 and 116 ms. Quality pairs differed in direction in both views. At 200% the wait went from 1863 to 1855 ms, and in half-resolution Fit it went from 986 to 994 ms. Earlier foreground trials had a different workload identity. In three foreground Speed pairs the native wait went from 1242 to 1184 ms. On the secondary M1 Pro the two background Quality Fit pairs got slower and the native Quality pairs differed in direction. Keep the background window flag explicit in automated runs.

### Coarse Size at neutral Coarse Amount

After the Fine-only shortcut, Coarse Size still changes the graph key when the shader skips the coarse blur. A diagnostic build excluded unused strides from the node key. On Glossy Leaves and Buds with Fine Amount 0.75 and Coarse Amount 1, a warm Coarse Size edit fell from 75 dispatches to one in quarter-resolution Quality and from 141 to one on the native Speed patch. Requested GPU storage per edit fell from 154 to 11 MiB and from 743 to 6 MiB respectively. All 36 paired final-image hashes matched. The authored size still took effect when Coarse Amount changed.

The patch retained 129 MiB less graph data until the next active edit. Changing Coarse Amount then used 289 dispatches versus 153 as the deferred checkpoints rebuilt. Subsequent active edits used 153 in both arms. The secondary M1 Pro had concurrent work, so these GPU counts establish the change in work more reliably than the exploratory timings.

A separate Merrill Color Detail prototype alternated existing scalar buffers across the two blur directions. It allocated a third plane only when it had to preserve the Fine boundary for a later Coarse pass. Native Quality saved 235 MiB of cumulative storage requests with Fine alone and 470 MiB with Coarse. Both complete float buffers matched. On the secondary M1 Pro, the Fine-only median fell from 707 to 593 ms across three pairs. The Coarse median fell from 742 to 694 ms, but one pair slowed by 204 ms. Other rendering work was active. In two separate paired process holds per control, lifetime peak dropped by 3–120 MiB and settled differences varied in direction. On the uncontended primary M1 Pro all six native Quality timing pairs improved. The three-pair median fell from 465 to 453 ms with Fine alone and from 483 to 467 ms with Coarse alone. In two separate process pairs for each control, all differences in peak and held footprint were under 1 MiB. This change reduced allocation traffic and latency with little measurable effect on physical memory.

### Adaptive Tone sample analysis

The histogram and regional-sample kernels read the same analysis grid and calculate brightness, peak ratio and gradient coherence separately. A diagnostic build used one kernel to write both outputs. Each fresh completed render of Glossy Leaves and Buds saved one dispatch and one submission. Across four counterbalanced process pairs on the M1 Pro MacBook, the median quarter-resolution render moved from 129 to 130 ms in Speed and from 144 to 143 ms in Quality. At native resolution the corresponding medians were 571 to 569 ms and 558 to 558 ms. Each process contributed four measured samples after two discarded renders. In four native-app pairs the median Speed pan moved from 561 to 587 ms and first zoom from 1181 to 1168 ms. All eight pan trials exceeded the existing 520 ms target.

Both Speed float readbacks matched exactly. The two Quality outputs changed reproducibly. In 8-bit SDR sRGB, 8 of 921,984 pixels changed by one code at quarter resolution and 144 of 14,751,744 at native. The remainder matched. The measurements do not justify changing the rendering path. On Intel UHD 630, three paired launches gave 2481 to 2447 ms at half-resolution Quality and 818 to 818 ms at quarter-resolution Speed. A separate Intel Quality capture matched all regional sample and summary floats. Two count transfers among four histogram bins changed the fitted coefficients by at most 5.96 × 10⁻⁸ and the final stored tone-mapped channels by at most 2.98 × 10⁻⁷. The gaming GPU produced identical intermediate and final results on the same check.

### Stable highlight donor grouping

Merrill Classic highlight restoration first compacts eligible pixel indices, then sorts them into 256 correction-table bins. A diagnostic alternative classifies candidates directly into bins, preserving each bin’s original pixel order. This removes an intermediate donor list. A first attempt scanned all candidate blocks serially per bin. On the low-light speaker only 4.1% of candidates qualified and the bin-prefix GPU interval grew from 1.17 to 29.29 ms. Divide that scan into bounded segments and combine their integer totals to keep stable ordering. The segmented version saves two queue submissions per highlight execution.

Three counterbalanced paired MacBook launches per photograph and policy gave these medians for native whole renders. Each of the six final float readbacks matched the baseline exactly. Twelve focused GPU tests also passed.

| Photograph | Quality baseline → segmented | Speed baseline → segmented |
|---|---:|---:|
| Blue parking sign | 173 → 177 ms | 166 → 163 ms |
| Low-light speaker and desk | 188 → 180 ms | 171 → 169 ms |
| Sunlit metal latch | 265 → 252 ms | 175 → 177 ms |

Native Quality requested 61.4 MiB less temporary GPU storage per highlight execution. In two separate matched runs on the speaker, process lifetime peak physical footprint was 1353 and 1354 MiB with the original grouping, and 1190 and 1191 MiB with the segmented grouping. With the completed output held, the corresponding footprints were 521 MiB in both control runs and 519 MiB in both segmented runs. The macOS allocator and driver contribute to these physical readings. In two Intel UHD 630 process pairs per scene, native Quality improved from 1620 to 1566 ms on the blue sign and from 1792 to 1752 ms on the latch. One blue-sign Speed pair measured 1474 and 1471 ms. Every Intel output matched within its policy and every comparison issued two fewer submissions. Windows physical memory was outside this test.

### Reusing Clarity scratch across six blur levels

With Universal Tone and Clarity enabled, each of six horizontal and vertical blur pairs allocates two full-image workspaces. A first experiment reused three two-float planes across the levels. It reduced native Quality rendering from 279 to 218 ms across four paired launches on the 32 GiB M1 Pro. In separate native Quality holds it raised the median lifetime process peak from 1351 to 1501 MiB, despite almost identical settled footprints.

A second version uses one two-float state per pixel for brightness and accumulated detail, plus one scalar horizontal mean. The horizontal pass reads neighboring state. The vertical pass reads neighboring means and updates its own pixel’s state. Queue order keeps the passes separate. All six levels use the existing blur arithmetic. The node requests eleven fewer storage allocations and submits the same fourteen dispatches. At native resolution on Glossy Leaves and Buds that saves 1411 MiB of cumulative GPU allocation requests. This quantity counts bytes requested over a render.

Compare the original implementation directly with the two-buffer version on the broken-screen 32 GiB M1 Pro. Use Universal Tone and Clarity +0.5, or −0.5 in quarter-resolution Speed. Each arm runs in a fresh process with three warm completed renders. The values below are medians of three counterbalanced launch pairs. All final float-buffer hashes matched within their respective workloads.

| Completed render | Original | Two buffers |
|---|---:|---:|
| Full native Quality | 285 ms | 202 ms |
| Full native Speed | 288 ms | 200 ms |
| Quarter-resolution Quality | 100 ms | 104 ms |
| Quarter-resolution Speed | 105 ms | 106 ms |

In two counterbalanced background GUI pairs, drag Clarity from 0.20 to 0.63 on the same photo. At 200% zoom the release-to-visible Quality wait fell from 254 to 212 ms and from 230 to 176 ms for the 992 × 1010 native patch. Half-resolution Fit was mixed at 310 to 319 ms and 404 to 360 ms. Every trial delivered the requested settings at the starting viewport and scale.

Three separate native Quality process pairs held the final image and retained session for 1.6 seconds after GPU completion. The original and two-buffer median settled footprints were 1072 and 1071 MiB. Lifetime peaks were 1350 and 1351 MiB, with all paired differences within 2 MiB. At quarter resolution the settled median was 351 versus 350 MiB. Fit peak increased by 2, 8 and 35 MiB in the three pairs. The original and candidate retained identical graph bytes at each scale. One additional pair per view held the result for 7.5 seconds. Settled native footprint was 1064.9 versus 1065.6 MiB and quarter-resolution Fit was 351.3 versus 350.1 MiB. In those pairs the candidate peak was 34 MiB higher at native and 78 MiB higher at Fit. The higher Fit peaks prompted the controlled follow-up below.

On the Intel UHD 630 with 16 GiB of shared memory, three rotated launch blocks compared all three implementations. Full native Quality went from 2959 ms originally to 2634 ms with three buffers and 2073 ms with two. Quarter-resolution Speed went from 483 to 469 to 436 ms. All three arms passed exact full-float comparisons at both resolutions, and dispatch and transfer counts matched. Three existing GUI processes stayed open on Windows. Windows process footprint was outside the comparison.

A separate 37 × 29 textured RGB fixture crossed both axes’ workgroup boundaries. All 28,971 float samples matched byte for byte across native, half and quarter strides at three Clarity amounts, including signed inputs. All nine focused photographic tests passed on Metal.

### Locating the quarter-resolution Fit peak

On the broken-screen 32 GiB M1 Pro, every process rendered the same quarter-resolution Quality view of Glossy Leaves and Buds with Universal Tone and Clarity +0.5. Launches used an idle-host guard. In 24 balanced, shuffled blocks, run the original executable twice under different labels and the two-buffer executable once. The mean OS lifetime peak was 910.2 MiB for the original and 936.8 MiB for the candidate. The mean within-block difference was +26.6 MiB (95% block-bootstrap interval +11.1 to +41.2). Two launches of the identical original executable differed by up to 139 MiB. Held medians were 353.8 and 352.8 MiB. Choosing builds did reproduce a peak difference in this cohort.

To locate the peak, read the OS lifetime counter at engine setup, source opening, kernel submissions, Clarity entry, completion and the retained hold. Across 120 phase-marked processes, the final peak was already recorded before the first Clarity allocation in every run. One control used the same executable to select either normal shader at engine initialization, preserving the production pipeline count. Across 54 launches the candidate’s mean peak difference was −1.3 MiB (95% block-bootstrap interval −21.4 to +17.0). In 51 runs the peak had been reached by the first working-grid pooling submission and in the other three by the following saturation-flags submission. At Clarity entry the two arms typically held 313–314 MiB. Immediately after Clarity submission they held 348 MiB with the original and 326 MiB with two buffers.

The native sensor and repair buffers can still be in flight when Fit first pools them down. In a diagnostic run, waiting for submitted GPU work around camera repair narrowed the lifetime peak range from 822–968 MiB across 54 launches to 855–858 MiB across 18 launches. This locates the variable peak at upstream repair and downsampling. The exact Metal allocations responsible for the difference between individual launches remain unknown. The rendering path keeps its asynchronous submission order.

A final independent comparison used the maintained timing probe’s replay request in the corrected memory probe. All 12 counterbalanced pairs returned divisor four and the expected 784 × 1176 view. Mean peaks were 928.4 MiB originally and 901.3 MiB with two buffers. The paired difference was −27.2 MiB (95% block-bootstrap interval −59.3 to +5.5). Mean held footprint was 351.7 versus 350.8 MiB. The direction of the original separate-build peak difference did not repeat.

The working renderer now uses the two-buffer implementation in both policies. Four more Metal comparisons covered the native low-light speaker, half-scale latch, quarter-scale tree shadows and a 643 × 571 native patch on the leaves. Both the complete final floats and the linear Clarity inspection matched their respective baseline in all four cases.

## Quattro reconstruction

Three Quality captures on the 32 GiB M1 Pro covered DP2, DP3 at ISO 800 and sd Quattro. At native resolution the scalar blue bilateral filter took 64–66 ms. The two RGB range-filter passes together took 85–96 ms. Quarter-resolution profiles differed by photograph. Ordered RGB defect repair took 30 ms on DP2, while two recursive-mean passes took 26 ms combined on the higher-ISO DP3. These are individual GPU timestamps from a separate instrumented render. Earlier Quattro work includes [repair scheduling and preview experiments](archived/quattro-performance-and-fast-preview.md).

### Proposed three-change build

The proposed build reuses RGB scratch after its last read, shares the input of identity shading corrections and increases the blue bilateral workgroup size. These changes add seven net lines across five node files. The combined candidate keeps the original residual-finishing passes. Four alternating-order pairs per photograph compared this build directly with 96ece3c, with profiling off and two warm renders per launch. Each case passed a complete float-buffer comparison.

| Primary MacBook workflow | Original | Proposed | Change | GPU storage requests avoided |
| --- | ---: | ---: | ---: | ---: |
| DP3 native Quality, Auto WB | 772.7 ms | 731.8 ms | −5.3% | 515.5 MiB |
| DP2 native Quality, Sunlight WB | 809.4 ms | 794.4 ms | −1.9% | 309.3 MiB |
| sd Quattro quarter Speed, Sunlight WB | 163.5 ms | 154.2 ms | −5.7% | 118.7 MiB |

All eight native comparisons favored the proposed build. The quarter-resolution candidate was faster in three pairs and slower by 16 ms in one. Figures for storage requests are cumulative over one render. Six additional native DP2 process pairs using the same replayed request measured median lifetime peak 2172.87 → 2172.84 MiB and held footprint 1549.47 → 1549.42 MiB. The largest paired peak difference was 0.5 MiB. See the separate retained DP3 experiment below for memory saved during editing.

The final five-file checkout passed 696 default core tests and seven focused GPU and original-file checks. Three additional comparisons covered complete final and inspection buffers on DP0 quarter Quality color, DP3 ISO 800 native Quality color and sd Quattro quarter Speed monochrome. All matched byte for byte. The selected combination was measured on Metal. The Intel results below cover individual experiments.

### RGB scratch and residual finishing

The RGB prefilter has four private destinations per scale whose last reads have already been submitted. Moving those buffers into their next role preserves shader order and avoids eight allocations across the two scales. The residual-finishing shader computes neighborhood detail from its immutable inputs. The next pass reads that detail only at its own pixel, so the two passes can share one invocation. Both the RGB and signed detail remain available to graph inspectors.

Compare four builds in a balanced rotation on the same MacBook. Each cell has four fresh process launches per build, two completed warm renders per launch, and a separate byte comparison of the final float image. The RGB scratch reuse and residual fusion together removed nine allocations, one submission and 391.8 MiB of cumulative storage requests on the native DP2. At quarter resolution they removed 124.0 MiB. Final floats matched across all four builds in all six cases.

| Completed render on M1 Pro | Original | RGB scratch reuse | Residual fusion | Both |
| --- | ---: | ---: | ---: | ---: |
| DP2 native Quality | 814 ms | 809 ms | 812 ms | 808 ms |
| DP3 ISO 800 native Quality | 780 ms | 782 ms | 777 ms | 776 ms |
| sd Quattro native Quality | 679 ms | 676 ms | 677 ms | 670 ms |
| DP2 native Speed | 810 ms | 814 ms | 808 ms | 803 ms |
| DP2 quarter Quality | 215 ms | 205 ms | 210 ms | 211 ms |
| sd Quattro quarter Speed | 162 ms | 165 ms | 157 ms | 158 ms |

The combined build was faster in all sixteen same-block native comparisons. The quarter-resolution paired differences varied in direction. Local exact intermediate comparisons covered both RGB noise outputs on the higher-ISO DP3, the signed detail on a quarter-resolution DP0 and monochrome sd Quattro, and residual RGB on native DP3.

Process footprint told a different story from allocation traffic. In three separate DP2 Quality holds, the baseline and combined scratch/fusion build settled at 1548.5 and 1548.7 MiB for native, and 657.7 and 654.8 MiB for quarter resolution. In a follow-up with eight balanced blocks, the median native OS lifetime peaks were 2172.3 MiB for baseline, 2172.7 for scratch reuse, 2174.1 for fusion, and 2197.5 for both. The combined pairwise differences ranged from +1.5 to +53.3 MiB. In a separate six-pair cohort its native peak medians were 2172.6 → 2219.0 MiB. At quarter resolution the differences changed sign, with peak medians of 1074.0 and 1073.0 MiB.

Four diagnostic pairs read the OS lifetime counter again before each GPU dispatch. Native baseline reached its final peak by residual detail or the packed Lab conversion. The combined variant reached its final peak later in all four runs, at contrast lightness extraction in three and sharpening in one. The extra high-water growth occurs downstream of reconstruction. The individual driver allocations responsible remain unknown. Keep residual fusion as a separate experiment. The prefilter reuse alone stayed within 1 MiB of baseline in all eight native comparisons.

On the Intel UHD 630, the two changes together reduced native DP0 Quality from 10,328 to 10,106 ms and quarter-resolution Speed from 1,880 to 1,838 ms. All eight paired launches favored the candidate. Complete final floats matched the baseline on that adapter.

### Bilateral workgroup size

A separate experiment increased the square workgroup of the quantized scalar bilateral filter from 8 × 8 to 16 × 16. With an eleven-wide window, an 18 × 18 shared tile produces 64 outputs in the original shader, while a 26 × 26 tile produces 256 outputs in the candidate. Each pixel visits the same samples in the same order. Shared workgroup storage grows from 2,592 to 5,408 bytes. Two native GPU timing pairs on the primary MacBook measured 64.1 → 53.0 ms for DP2, 66.5 → 53.8 ms for DP3 ISO 800 and 65.9 → 54.3 ms for sd Quattro. Across four off-profile launch pairs, each with two warm renders, median completed times were 810 → 804, 787 → 774 and 677 → 666 ms respectively. Complete final floats matched on all three. The quarter-resolution route uses a different filter.

The Intel UHD 630 kept identical native DP0 floats with the larger workgroup. Across two reversed-order GPU timing pairs its scalar filter increased from 638.7 to 643.4 ms, about 0.7% of the filter and under 0.05% of the ten-second frame.

### Calibration gains of one

An independent six-line change lets the two calibration nodes return a read guard on their input when every decoded gain is exactly one. One of five tested DP captures met that criterion with its capture white balance. On that DP3, a retained session started at half resolution, edited exposure and Smooth/Crisp, then rendered a 992 × 1002 native patch and panned. Chroma, luminance and banding were set to two. Across four balanced headless pairs on the primary M1 Pro, the first patch took 588 → 562 ms. Three GPU dispatches disappeared and the retained checkpoint allocation fell from 911.0 to 828.6 MiB through subsequent pans. All ten phase outputs matched byte for byte on a second M1 Pro.

A separate trial used six balanced permutations of the baseline, blue-plane bypass alone, and both bypasses in the same executable with one-second holds. On the primary MacBook the median settled process footprints after the first patch were 1423.4, 1340.8 and 1341.0 MiB respectively. Fit stayed at 831.5–831.6 MiB. Median OS lifetime peaks were 2081.6, 2081.5 and 2081.6 MiB, all reached during the first patch. The second M1 Pro repeated the held saving and measured 2135.5, 2134.7 and 2134.2 MiB lifetime peaks. Its 100 ms samples alone gave the misleading impression that the two-node bypass raised peak by over 100 MiB.

### Request signed detail only when a consumer uses it

Quattro residual finishing used to generate signed detail alongside RGB for every request. Universal sharpening and disabled sharpening only read RGB. The prototype gives each output its own graph node. Reference sharpening and explicit detail inspection request the detail node, which damps the signed value in the same dispatch. The graph can then prune the unused detail branch and its enlarged highlight mask. Both branches retain the original pixel arithmetic.

On the primary M1 Pro, four alternating pairs of full native DP2 Quality renders with Universal sharpening reduced settled process footprint from 1773 to 1670 MiB, and retained GPU cache from 1088 to 985 MiB. The GPU requested 247 MiB less storage. The OS lifetime peak increased from 2172 to 2317 MiB in all four pairs. Quarter-resolution Fit saved 8 MiB settled. A fresh 992 × 1002 native Speed patch saved 39 MiB settled. In independent uninstrumented timings, full DP2 Universal went from 811 to 785 ms and sd Quattro Universal from 681 to 652 ms. Fit, patch and full DP2 Reference timing differences were small or mixed.

In a retained DP2 session on a second M1 Pro, half-Fit and the subsequent native pan retained 39 and 59 MiB less GPU cache respectively. The first switch from Universal to Reference after the pan rebuilt the missing mask and took 190 ms versus 75 ms. In a separate repeat-switch trace the second switch took 66 ms and the next Reference pan took 67 ms. Check both directions of a mode switch when measuring work removed from a preview path.

#### Reuse the two RGB buffers along that path

The native Universal peak moved downstream to the inverse packed Lab conversion after wavelet contrast. Four interleaved builds separated the original, the signed-detail change, vertical-guide reuse, and both changes. The guide buffer has exactly the byte size of reconstructed RGB, and its final read is submitted before reconstruction writes RGB. Reusing it saved 247 MiB of allocation traffic on the native DP2. With both changes, the native Reference peak dropped from 2172 to 2091 MiB. Native Universal still peaked at 2317 MiB.

Each packed Lab conversion reads and writes only its own pixel. The final prototype moves the input into the conversion after its last graph read. When another requested output or cache entry owns the input, the conversion copies it first. Retained-session tests included the upstream RGB and final image together and verified both against fresh renders. Requesting the pair from an empty native session caused one extra 247 MiB device copy compared with the first two changes. An ordinary pan and an already cached pair incurred no extra copy. In the four local trials the fresh shared-inspection latency difference was within the sample spread.

In four matched pairs on the primary M1 Pro, the final build reduced full native DP2 Universal latency from 815 to 768 ms, settled footprint from 1773 to 1670 MiB, and lifetime peak from 2172 to 2162 MiB. Its 990 MiB reduction in requested GPU storage counts allocations throughout the render. Full native DP2 Reference went from 800 to 782 ms and peaked at 2005 MiB versus 2172 MiB, with the same settled footprint. Native sd Quattro Universal went from 683 to 637 ms. The ISO 800 DP3 with sharpening off went from 763 to 737 ms and settled 103 MiB lower. The native DP2 Speed patch went from 584 to 577 ms and settled 39 MiB lower.

Across twelve balanced quarter-Fit pairs, the median paired changes were +0.6 ms latency and −0.1 MiB peak, with both signs observed. Settled memory fell by about 8 MiB in every pair. A second four-pair retained-session comparison measured the first Universal-to-Reference switch after a pan at 138 to 254 ms on the local M1 Pro, and a repeat switch at 112 to 121 ms.

Exact final-image comparisons on the primary Mac covered the DP2, DP3 and sd Quattro. The Windows RTX 3090 reproduced the original native DP2 final image, signed detail, residual RGB before packed Lab, and RGB after inverse conversion on its own backend. A synthetic GPU test checks the same conversion with an inspection root held across its last ordinary read.

### Batch independent blue defect clusters

Quattro blue cluster repair used to submit each cluster separately. Every workgroup still repairs its cluster in catalogue point order and breaks equal scores by donor order. Preparation assigns a later batch whenever a cluster writes into an earlier cluster's read bounds or reads an earlier write. Include the full donor patch in those bounds. The first candidate center is one pixel inside the search rectangle and its taps reach two pixels in either direction.

Four matched MacBook pairs against `eecf8eb` measured the same snapshot in separate processes with profiling Off and two warmed renders per launch. Native DP2 Universal fell from 769 to 646 ms (16%), and a fresh 992 × 1002 DP2 Speed patch fell from 573 to 455 ms (21%). Both requests run cluster repair before the crop. The ISO 800 DP3 native render fell from 732 to 666 ms (9%), and the sd Quattro native render fell from 636 to 618 ms (3%). The node's dispatch count fell from 60 to 1 on DP2, 39 to 2 on DP3 and 10 to 1 on sd Quattro. The DP2 instrumented node took about 4 ms versus 119 ms in the earlier trace. The quarter-Fit control used the same 89 dispatches in both builds and had no blue cluster node. All five final float buffers matched exactly.

Four matched process pairs reduced DP2 Universal native peak from 2162 to 2041 MiB and settled footprint from 1670 to 1638 MiB. The native DP2 patch settled 31 MiB lower. On sd Quattro, ten pairs increased median peak from 2165 to 2256 MiB, with an increase in nine pairs. The settled footprint changed from 1753 to 1751 MiB and retained GPU bytes matched exactly. Dispatch-level probes found the high-water mark later in wavelet contrast or crop. The probes did not identify which driver allocations account for the difference.

The intermediate blue plane matched byte for byte on the DP2, ISO 800 DP3 and sd Quattro, with a separate DP2 Reference sharpening check. The synthetic catalogue covers intersecting donor reads, both boundaries of the donor halo, and a cluster that can run independently.

#### Try reducing the sd Quattro peak after cluster batching

Moving the wavelet contrast output allocation to the last pass gave mixed paired peak differences in six sd Quattro trials. The median moved from 2262 to 2257 MiB. A separate prototype passed the packed Lab image by ownership to the wavelet node. Its final pass updates only lightness inside the crop. A shared input is copied before that pass. This avoided one 254 MiB storage request in the ordinary native sd render, with the same device-copy traffic and retained GPU cache. In six fresh process pairs against the build before batching, the combined prototype still peaked at 2250 versus 2166 MiB. The avoided allocation did not resolve the sd process peak. All four instrumented reuse trials reached the high-water mark by the crop dispatch. Four separate Off-profile pairs against batching alone improved sd native from 614 to 598 ms and DP2 native from 642 to 630 ms. All repaired render outputs matched the existing float goldens. The GPU test also verifies that a shared input keeps its original bytes and that the exclusively owned path avoids a copy.

Three sd Quattro wall-profile pairs had identical live graph, scratch and cache bytes at every executed node boundary. A separate probe counted unique live application buffers, including those inside nodes, and sampled the macOS graphics footprint ledger. In a pair with the larger gap, batched and serial cluster dispatch had the same application bytes before the inverse Lab pass, but the graphics ledger was 86 MiB higher for batching. The internal-memory ledger was within 1 MiB. That points to overlapping graphics backing whose release depends on submission timing. The job ordinarily waits for the previous tick, so a node can prepare and submit while its predecessor is still in flight.

Four matched diagnostic runs with the same executable and scene lowered batched peak from 2226 to 2160 MiB by waiting immediately before forward RGB-to-Lab conversion. Waiting only before the inverse conversion had a smaller effect. A nonblocking poll before wavelet contrast left the peak near the batched baseline. These timings include the overhead of memory tracing. The probe identifies a queue boundary where the backing can retire. It does not identify the individual Metal allocation.

The larger change came from waiting just before residual RGB, after blue-guided reconstruction had submitted. In the diagnostic build sd peak went from 2224 to 2035 MiB. Waiting before reconstruction increased the peak, and a nonblocking poll before forward Lab did little. Four additional staged process pairs with diagnostics off compared the same batched build with and without the wait before residual RGB. For native sd Quattro, peak fell from 2260 to 2075 MiB and completion moved from 697 to 711 ms. For native DP2, peak fell from 2042 to 1969 MiB and completion moved from 695 to 705 ms. The fresh DP2 patch showed no consistent peak improvement. Settled memory and retained checkpoints stayed close. The findings identify overlap between blue-guided reconstruction and the following work as the place to intervene. The production wait that followed is described next.

#### Retire full-native reconstruction before residual RGB

The Quattro recipe requests a GPU settle before residual RGB for full native doubled reconstruction with Universal sharpening or sharpening disabled. The executor checks both reconstruction dependencies for cache hits. If either dependency had to be generated, it settles outstanding GPU work and checks for cancellation before allocating RGB. The wait has no effect on pixel identity. Fit and true regional graphs keep their existing lookahead. Reference already does mask and detail work before residual RGB. Four pairs found no memory saving from an explicit wait on that path.

Four matched pairs on the 79b98e7 base compared the batched renderer before and after adding the handoff. Native sd Quattro went from 2175 to 2001 MiB lifetime peak and from 621 to 637 ms. Native DP2 went from 1966 to 1898 MiB and from 646 to 663 ms. The ISO 800 DP3 with sharpening disabled went from 2159 to 1892 MiB and from 669 to 678 ms. The native DP2 patch kept the same median peak and finished in about 460 ms. Final float outputs matched on all six workflows.

#### Probe a regional low-ISO RGB filter

A fresh 992 × 1002 Speed patch on DP2 spent about 115 ms of GPU time in the whole-image two-scale RGB filter. The ordinary path below estimated ISO 400 has bounded support. A diagnostic GPU test used DP2 calibration and a textured 512 × 384 input. Both output planes matched byte for byte between the full filter and aligned crops with a 40-color-cell halo. The test included one center crop and two crops touching real image edges. The full recipe still needs to preserve the original pyramid phase and shading-grid coordinates. Retained full-image checkpoints already serve repeat pans and should be measured separately from a fresh patch.

The complete recipe prototype crops the calibrated low-resolution RGB before denoise and trims it back before reconstruction. It preserves the pyramid phase, keeps absolute shading coordinates, and adds the raw-color chroma limiter's neighborhood to the halo. Full-frame checkpoints can still satisfy the request. The prototype leaves the adaptive filter at estimated ISO 400 and up on its whole-image route.

On a local 16 GiB M1 Pro, four independent DP2 comparisons moved median fresh Speed patches from 456 to 342 ms, Quality from 462 to 348 ms, and a Speed patch touching the sensor edge from 445 to 314 ms. The final float buffers matched exactly, as did the low-ISO sd and ISO 800 DP3 controls. In a retained Fit-to-zoom session the first DP2 patch went from 469 to 346 ms and retained GPU cache from 796 to 657 MiB. The next pan across a prepared boundary went from 55 to 95 ms. Returning to the earlier patch also reran the filter, and the following whole-image render went from 209 to 363 ms. Results matched at every step. Window alignment at 128 or 256 color cells spent more time filtering without avoiding these misses. Keep the whole-image checkpoints in the candidate stack.

Four separate process pairs measured peak and held footprint. DP2 Speed fell from 1442 to 1227 MiB peak and from 1002 to 863 MiB held. On sd Quattro the corresponding figures were 1578 to 1284 MiB and 1097 to 953 MiB. The ISO 800 DP3 control matched on both measures.

#### Measure the native blue filter budget

A Standard-mode probe with explicit reconstruction controls changed only Luminance from two to zero. Across two warmed renders per setting the native DP2 went from 662 to 606 ms and the native sd Quattro from 642 to 572 ms. The native DP2 patch went from 454 to 446 ms. The ISO 800 DP3 at Standard went from 671 to 628 ms and showed larger float differences, with mean absolute error 0.00395 versus 0.00121 on DP2. The probe sets a ceiling for a faster Speed filter. A reduced tap pattern should compare fine texture, chroma noise and overlapping patches against the current filter.

#### Reuse reconstructed RGB for residual gain

Residual gain reads each pixel's own RGB and gain. The candidate transfers the RGB buffer when the graph reaches its last consumer and the buffer has no other owner. The out-of-place pass preserves the original RGB for cached and requested intermediates. Both kernels share the original gain and quantization arithmetic.

Four local M1 Pro pairs on the native handoff build avoided a 254 MiB GPU allocation request on sd Quattro and 247 MiB on DP2. Bypassing the handoff wait when storage could be reused moved native completion from 616 to 604 ms on sd Quattro and 632 to 620 ms on DP2. The DP2 and sd process peaks stayed close. On the ISO 800 DP3, paired peak changes ranged from −84 to +140 MiB, so the wait needs an independent comparison. The process held footprint and retained cache did not show a consistent rise. Final float output matched across all six workloads, and the GPU ownership test compared both kernel paths on a partial workgroup.

The stack keeps the full-native handoff wait on both RGB paths. In six ISO 800 DP3 pairs the waited peaks stayed at 1883–1910 MiB. The no-wait arm ranged from 1824 to 2047 MiB with a lower median. Keeping the wait traded the highest observed no-wait peak for more repeatable footprint. With that policy, four pairs against the original handoff still improved native sd Quattro from 617 to 604 ms, native DP2 from 636 to 622 ms, and native ISO 800 DP3 from 669 to 659 ms. The DP2 and sd Quattro peak differences remained small.

#### Shared RGB range tile on Metal

An exact RGB range-filter prototype used an 8 × 4 workgroup and a packed Float32 shared tile. The radius-15 tile fits within 16 KiB. Halo samples use the same edge clamps and the filter visits taps in the original order. The first quiet A/B pair on a local M1 Pro matched final floats for native DP2, a native DP2 patch and quarter-Fit. Native went from 638 to 4997 ms, the patch from 455 to 4811 ms, and quarter-Fit from 183 to 1757 ms. Stop this tile shape. The earlier scalar blue tile result did not transfer to this RGB kernel. A future tile should time its own kernel and report workgroup storage and occupancy on the backend.

#### Shorten the Speed blue-filter window

Below capture ISO 800, Speed caps the quantized histogram bilateral radius at three, so the native window is 7 × 7 instead of 11 × 11. The same blend and range weights are used, and the radius is already part of the cache key. Quality and the scalar blue filter keep their existing support. A reduced histogram window that already fits the cap also keeps its support.

Four quiet pairs on a local M1 Pro compared the radius change alone against the native handoff build. DP2 native Speed went from 634 to 604 ms, sd Quattro native from 618 to 581 ms, ISO 800 DP3 from 672 to 639 ms, and a 992 × 1002 DP2 patch from 453 to 442 ms. Quarter-Fit matched byte for byte and its timing shift was within a few milliseconds. On the two low-ISO native cases, more than 99.6% of visible pixels differed by at most one 8-bit SDR sRGB code per channel. For ISO 800, 92.75% were within one code and 99.94% within three. The largest difference was nine codes. The highest-difference ISO 800 area shows a little more grain with the shorter window, so keep this comparison alongside the timing results.

With in-place residual RGB and the native wait retained, four interleaved Speed pairs moved DP2 native from 647 to 592 ms, sd Quattro from 617 to 579 ms, ISO 800 DP3 from 670 to 626 ms, and the DP2 patch from 455 to 445 ms. Two native Quattro H pairs went from 912 to 845 ms, with 99.91% of visible pixels within one code. Five Quality smoke comparisons were exact, and the stacked Speed outputs matched the shorter-radius-only goldens in the DP2, DP3 and sd cases. The unrestricted radius cap reproduced the high-ISO peak in six further runs. All six candidate peaks were 2045–2048 MiB. Five baseline runs were around 1905 MiB, with one at 2048 MiB. Keep the full window at capture ISO 800 and above to preserve high-ISO grain and avoid the repeatable rise on this file. In the final gated build the ISO 800 Speed output matched the handoff baseline byte for byte. Across four fresh pairs, three gated peaks were 1889–1906 MiB and one reached 2047 MiB. Two baseline peaks were 1904 and 1911 MiB. The other two were about 2048 MiB. This reproduces the broader fluctuation seen before the unrestricted filter. The stack held native SD and DP2 measured peaks within the handoff build’s sample spread. A paired native DP2 inspection also returned the same reconstructed and residual RGB as two independent renders, preserving the original when both are requested.

#### Simplify the stack

The shared Stage2B constructor now chooses the Speed histogram window after applying the grid scale. Color and monochrome use their own blend and residual controls with that constructor. Residual finishing uses the same ownership handoff as packed Lab. The executor transfers the last graph read and the node checks storage exclusivity before selecting its kernel.

Four process pairs compared the native queue-wide settle with waiting only for the previous tick's completion marker. Native DP2 and sd Quattro peaks stayed within roughly 20 MiB. On ISO 800 DP3 the marker-only wait gave 1884, 1902, 1984 and 2050 MiB. The queue-wide wait stayed between 1876 and 1901 MiB. Held footprint stayed within 3 MiB. Keep the queue-wide settle and require outstanding lookahead before requesting it.

#### Shorten the Speed chroma filter window

Stage1's coarse RGB pass uses a 31 × 31 range-filter window at the native grid. Speed caps that pass at radius 10 below estimated ISO 400, outside the long-exposure route. The maximum window becomes 21 × 21. The cap follows the existing working-grid scaling. The fine pass, adaptive filter, long-exposure route and already smaller pooled windows keep their existing support. Radius is part of the existing prepared filter key.

Three local M1 Pro pairs per case compared the cleaned stack at the old and new Speed radii. Median native completion moved from 596 to 571 ms on DP2, 617 to 586 ms on DP0, 581 to 551 ms on DP3 and 587 to 553 ms on sd Quattro. The DP2 Speed patch went from 454 to 424 ms. One sd Quattro H pilot went from 860 to 809 ms. A radius-12 pilot saved less time. These timings exclude validation readback.

At radius 10, 99.0–99.9% of visible pixels on the changed photos were within one 8-bit SDR sRGB code per channel of Quality. The largest difference was six codes. Worst-area crops showed a slight change in fine grain. The DP2 Speed patch matched its crop of the whole Speed render byte for byte. DP2 half- and quarter-Fit and the ISO 800 DP3 matched their original output exactly.

Four separate memory pairs compared the candidate against the original stack. The retained GPU cache matched exactly in all four workloads. Median settled footprint stayed within 3 MiB. Native DP2 peak was 1911 to 1912 MiB, native sd Quattro 2008 to 2001 MiB, and the native DP2 patch 1400 to 1398 MiB. The ISO 800 DP3 retained its bimodal peak, reaching about 2048 MiB in both builds. Native Quality, quarter-Fit Quality, Reference sharpening, and native and half-resolution monochrome matched the original stack byte for byte.

#### Convert the limited chroma directly to RGB

Quattro's raw-color limiter previously wrote clamped YUV and immediately read it back for a pointwise inverse conversion. The fused shader reads the same Float32 YUV, tracks the second extrema for U and V together, and applies the existing inverse conversion to the limited chroma in the same invocation. The shader keeps the 7 × 7 footprint, edge replication, and duplicate samples. The public raw-color inspection output matched byte for byte on native DP2.

Four repeated local M1 Pro comparisons of the full-window prototype moved DP2 native Speed from 566 to 556 ms, the DP2 Speed patch from 417 to 410 ms, and sd Quattro native Speed from 542 to 535 ms. DP3 ISO 800 moved from 661 to 656 ms and quarter-Fit from 181 to 178 ms. The image digests matched all five cases. Native DP2 and its patch each avoided a 62 MiB GPU allocation request and one dispatch. The saved allocation is on the full color grid even for the native patch.

Three paired runs of the reviewed build on the primary MacBook saved a median 10.7 ms per pair on the DP2 Speed patch and 12.5 ms on native sd Quattro. Full-native DP2 varied from 13.7 ms faster to 14.2 ms slower, with a median paired change of +0.7 ms. A further DP2 series was postponed when release validation resumed. The completed primary memory pairs numbered three each for native DP2 and its patch and two for native sd Quattro. They retained identical GPU cache bytes and shifted median peak and held process footprint by at most 4 MiB. Three local memory pairs per workload gave the same retained sizes and similarly small footprint differences.

A 5 × 5 Speed window saved little beyond fusion and had a local error of 37 codes at a DP0 edge relative to the fused Speed image. Skipping the whole limiter and its conversions left 99.0% of visible native DP3 pixels within one code of Quality, but changed individual pixels around a saturated specular edge by up to 163 codes. The ISO 800 DP3 reached 238 codes. Keep the 7 × 7 limiter and report high-difference crops alongside the whole-image distribution when evaluating similar shortcuts.

#### Sample alternating taps in the Speed chroma filter

The native Speed coarse RGB pass uses 221 of 441 taps below estimated ISO 400 with long exposure off. The offsets stay symmetric around the center and preserve the radius and phase between patches and Whole. The fine pass, pooled grids and Quality retain their existing taps.

On the primary M1 Pro MacBook, all four pairs per workload improved. Median DP2 Quattro native Speed went from 635 to 622 ms and its native patch from 451 to 441 ms. Native sd Quattro went from 608 to 586 ms. Quality, ISO 800 Speed, Fit and monochrome controls were bit-identical. Across five original Quattro photos and a DP2 patch, at least 98.6% of visible pixels were within one 8-bit SDR code per channel of Quality. The largest difference was six codes. The owner accepted the DP2 comparison crop. The DP2 center and corner patches matched their Whole crops exactly. Process footprint was flat on DP2. The sd Quattro median paired peak rose 8 MiB with mixed direction and retained cache bytes stayed equal. Regular fine patterns at the coarse grid frequency can be phase sensitive.

#### Smaller margins for Quattro box contrast

Native color patches using box contrast with Reference sharpening off use a 32-pixel camera margin. The 128-pixel phase alignment and shared-edit margin still apply. The bound covers doubled reconstruction, both ordered box passes and the camera crop. The same full upstream checkpoints can serve subsequent pans and Whole.

On the primary M1 Pro, four retained ISO 800 DP3 Quattro pairs per mode measured 565 to 540 ms on first native Speed and 567 to 543 ms in Quality. Warm pan processing went from 60 to 30 ms in Speed and from 59 to 33 ms in Quality. Median paired settled footprint fell by about 40 MiB in three fresh-patch process pairs per mode. Median peak was flat. Output matched exactly in the measured center and corner patches, including across Fit, pans and Whole continuation.

#### Recheck both changes on the reviewed stack

The combined build on `956d27f` reproduced the approved sparse Speed digests for native DP2, the DP2 patch and native sd Quattro. In three pairs, DP2 patch medians were 413 to 407 ms and native sd was 543 to 538 ms. Native DP2 whole was flat at 576 ms. The Quality, Fit and ISO 800 unaffected controls matched their baseline digests.

Six retained ISO 800 pairs per mode put the first native patch near 520 to 496 ms and the first warm pan near 54 to 30 ms. The median paired Whole continuation took 6 ms more in Speed and 4 ms more in Quality, with the same GPU activity and cached bytes. Three separate fresh patch footprint pairs per mode showed about 36 MiB less median held memory and 40 MiB less retained GPU cache. Speed peak varied across pairs and Quality peak was flat. Replaying Fit through Whole and a subsequent pan matched every baseline output in both modes. The maintained regional test passed all 36 exact crop comparisons and all ten Whole continuations.

### Profiling Quattro edits

A fresh patch benchmark can miss work that moves to later requests. Replay Fit, first native patch, a pan across a processing boundary, a return pan and whole-image continuation in one retained session. Report each step's completed time, dispatch count and retained bytes. Sample process footprint in a separate replay and keep float readback outside the latency samples.

Two profiler changes would help trace Quattro edits. The checkpoint probe rejects a Smooth/Crisp patch when the photo starts with its optional reconstruction settings unset. It could validate the patch against that control group’s typed defaults. Also, a wall-profile node can inherit a completion wait from earlier submissions. On a 992 × 1002 DP0 pan, a roughly 30 ms interval was attributed to mask doubling. GPU timestamps put the mask itself under 2 ms. Ranking measured kernel timestamps separately from queue waits would make that distinction visible.

## Integration cleanup for 0.9.3

The candidate combines the reviewed Dehaze, Adaptive Tone, Merrill highlight sampling, Clarity and Quattro changes on the 0.9.2 source at `48551cb`. The added Dehaze kernels use the existing lazy pipeline table. Manual Dehaze calls the brightness histogram directly. Serial selection lives in the test oracle, and the automatic path still selects the existing neighborhood minimum by radius. Adaptive Tone shares analysis-guide naming across both sources. Merrill highlight sampling and flag validation use one working extent. Quattro reuses the resolved sharpening controls and skips empty repair clusters after geometry validation.

On the local M1 Pro, 27 matching requests to the imported and cleaned builds produced the same complete float digests, geometry, GPU dispatches, submissions and storage requests. The cases cover native and pooled grids, both processing policies, native patches, sharpening methods, monochrome, and the affected finishing controls. An independent capture of Adaptive Tone's analysis RGB also matched byte for byte.

Five retimed pairs with active half-resolution Speed Adaptive Tone measured 152.7 to 151.2 ms, with both signs in the paired differences. Three memory pairs each for native DP2, Manual Dehaze and Clarity retained identical cache bytes. Median held footprint moved by at most 0.6 MiB. The core suite passed 707 tests. The 41 focused GPU checks and both original-file checks passed, including replay of Adaptive Tone across working grids and Dehaze in cached, full-preview and DNG routes.

## Other experiments and tools

Bypassing overlap calculations inside Merrill reconstruction tiles retained exact output and moved full render time from 411 to 411 ms. Leave the existing projection in place. Computing exact highlight donor prefixes in parallel reduced one diagnostic GPU interval from 15.3 to 0.9 ms on the blue sign. None of the ten full-render medians moved outside sample spread, so keep that experiment separate.

First foreground admission asks for a retained-memory inventory even when it admits the job unconditionally. On the M1 Pro, the Wall trace interval containing that inventory, allocation planning and the start of the second job took 32–54 microseconds across four warm two-photo exposure edits. Both photos became current in 111–160 ms. Removing even the entire interval would have saved less than 0.05% in those edits.

Copying authored photo settings from a Fit timing run once produced a native resource run. The maintained timing and memory probes now share a replay request with the complete render-settings snapshot and named output. A captured request includes the observed grid, which the memory probe verifies before it records completion. See [resource profiling](../docs/profiling.md#resource-overlap).

Add a display-space crop around the largest pixel change alongside raw float comparisons when evaluating Speed. Record lifetime high water alongside the 100 ms samples. Verify adapter identity in cross-run comparisons. Windows needs separate process-memory counter support.
