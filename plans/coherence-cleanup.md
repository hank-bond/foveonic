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
- [ ] **One GPU wait per delivered image.** Delivery waits once per finished frame instead of once per tick, which may limit the gain from wide rendering. Not measured.

## Audit

- [ ] Before closing this plan, audit the app against the principle's list and record what is left and why.
