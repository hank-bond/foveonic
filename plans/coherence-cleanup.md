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
- [ ] **Warm one-photo edits are 4 to 5 ms faster when every step waits for itself.** Measured 2026-11-24, interleaved on one tree, 40 edits each: waiting every step 34.5 ms, one step in flight 36 to 40 ms, 0.8.4 39 to 40 ms. Fresh renders go the other way (35 s against 30 s over the 144 render cases), so 0.9.0 keeps one step in flight. Find why a short warm edit loses time to the in-flight step before choosing a rule by workload.
- [ ] **One GPU wait per delivered image.** Delivery waits once per finished frame instead of once per tick, which may limit the gain from wide rendering. Not measured.

## Audit

- [ ] Before closing this plan, audit the app against the principle's list and record what is left and why.
