---
summary: "Finish removing duplicated owners and near-duplicate paths in 0.10.0, following the one-owner principle that 0.9.0 started applying."
---

# Coherence cleanup

Implements the first [design principle](../docs/architecture/principles.md#keep-one-owner-and-one-path). 0.9.0 ships wide rendering and the start of this cleanup. 0.10.0 completes it before feature work resumes (owner, 2026-11-22). The work ends when the major sources of duplicated state and near-duplicate paths are gone. Smaller ones are recorded and left when removing them is worth less than it costs.

Each item needs a design discussion and owner approval before implementation.

## Carried from 0.9.0

Details are in the linked plans. Move each item here when its work starts.

- [ ] **Ratings in the photo's sidecar.** [Persisted stores](./view-planning.md#persisted-stores-owner-direction-replaces-step-2-pieces-24) slice 3. Breaking change: `ratings.json` is not carried over.
- [ ] **`AppSettings`.** Slice 4. One file per key and one atomic JSON writer. Breaking change: preferences leave `app.ron`.
- [ ] **Import history edits.** Slice 5.
- [ ] **Automatic values only from renders.** Follows on the store. Editor actions on the whole editing selection landed in 0.9.0.
- [ ] **One readiness state per photo.** [Release-gate items](./view-planning.md#follow-ups), item 1.
- [ ] **Two jobs called "normalize".** Item 2.
- [ ] **One source for the camera family in render requests.** Item 3.
- [ ] **Split `Settings` into recipe and render parameters.** Item 4. Do this before the sidecar record is defined.
- [ ] **Test harness.** Item 5. Direct smoke targets move onto the scenario driver. Scenario waits name their condition.
- [ ] **Scheduler.** [Render scheduler](./render-scheduler.md) step 9 leftovers, step 10 (export and CLI on the scheduler), step 11 and the GUI naming follow-up.

## Found later

Record duplicated paths found after 2026-11-22 here, with where they are and what disagrees.

- [ ] **Remove the selected photo as a source of edits and choices.** Owner, 2026-11-23: the last-selected path is not useful and is buggy. `App::selected` still drives crop and rotation edits (`edit_selected`), the Editing menu's camera family and preset list, the values shown for prevent clipping and auto compensation with several photos, the navigator source and paste normalisation. Copy settings, Save preset and Set as default are disabled with several photos in 0.9.0. Crop presets per photo are not built.
- [ ] **No ignored tests left unsorted.** Owner, 2026-11-24: committed for 0.10.0. The `#[ignore]` flag currently covers 495 maintained GPU checks in ten groups and 151 excluded tests. Sort every one:
  - Each maintained test names the photograph it needs from the tracked manifests, so one command runs them all. This removes the environment variants that aim one variable at different files, and with them most of the [planner](../scripts/profiling/rust_test_inventory.py). Only the five checks against the maker's software keep their own process.
  - The 129 excluded tests that compare against private captured data: recreate the data from tracked photographs, or delete the test. Nothing may depend on files held on one machine.
  - The 18 benchmarks and timing experiments leave the test flag for the [comparable runs](./comparable-runs.md) tool or are deleted.
  - The four remaining exclusions (missing originals, capture-specific values) are decided one by one.
  - A renamed test must fail a local check. In 0.9.0 a rename left a stale inventory name that only the test machine found.
- [ ] **One source of a photograph's defaults for the app and the command line.** Owner, 2026-11-24, for 0.10.0. The app opens a photograph through the camera's capture defaults (`camera_settings`), so an unedited Merrill photograph has noise reduction on. The command line and the render tool start from `Settings::new`, which leaves it off, and the [spec](../docs/specs/despeckle.md) describes that as "Reconstruction remains opt-in". The two disagree about what a default render is. Consequences found in 0.9.0: the render cases named camera defaults did not run Merrill noise reduction, a crash in it on integrated graphics was missed, and the release tables were first measured with it off. Give both one path, then drop the `-noise` render cases, which exist only to cover the gap. A saved recipe that omits noise reduction still means off; decide whether that is right for recipes written before the field existed.
- [ ] **Noise node: skip margin pixels nobody reads.** The node filters 16 tiles with a 64-pixel margin each, 29% more pixels than the image. Find which margin pixels later steps read before narrowing any step. Output must stay bit-identical.
- [ ] **The memory budget follows the GPU's reported memory.** Owner, 2026-11-24, for 0.10.0. The app reads dedicated and shared GPU memory at startup and logs them but does not act on them. An integrated GPU borrows system RAM, so the app's memory and its GPU memory are one pool, and a small discrete card (a user has 2 GB) has a hard limit. Lower the budget and render one photo at a time when the adapter is small or shared. Aim: the app works, slowly if need be, on a 2 GB card and on integrated graphics with 16 GB. No 2 GB card is available for testing.
- [ ] **Steps in flight follow the memory budget.** Owner, 2026-11-24: replaces the fixed one-step rule in 0.10.0. Submit steps ahead while their storage fits the spare budget the scheduler already uses for width, and wait otherwise. Measured for one photo over 144 render cases: no limit (0.8.4) 29.6 s and a typical full-size peak of 2.19 GiB; two steps level with 0.8.4 in both; one step 30.1 s and 1.55 GiB, with many-step renders 10 to 40 ms slower.
  - First prove a check that releases finished storage without blocking. Without one, unbounded submission reached 5.9 GB on a full-size Quattro render.
  - Each wait costs 0.4 to 1 ms, which likely also explains the warm-edit item below. Measure both together.
- [ ] **Warm one-photo edits are 4 to 5 ms faster when every step waits for itself.** Measured 2026-11-24, interleaved on one tree, 40 edits each: waiting every step 34.5 ms, one step in flight 36 to 40 ms, 0.8.4 39 to 40 ms. Fresh renders go the other way (35 s against 30 s over the 144 render cases), so 0.9.0 keeps one step in flight. Find why a short warm edit loses time to the in-flight step before choosing a rule by workload.
- [ ] **Stage profile beside the trace.** Owner, 2026-11-24. Perfetto is the one trace recorder, but each render also keeps its own list of stage names and times (`Profile`), which tests, the profiling example and the comparable-runs stage times read. Find what the profile records that a trace does not, and what needs it with tracing off. Then either read stages from the trace or state why the profile stays.
- [ ] **One GPU wait per delivered image.** Delivery waits once per finished frame instead of once per tick, which may limit the gain from wide rendering. Not measured.

## Audit

- [ ] Before closing this plan, audit the app against the principle's list and record what is left and why.
