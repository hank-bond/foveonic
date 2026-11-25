---
summary: "Generate a render run from any revision and compare two runs for pixel changes and timing, and time every behavioral step, so release checks need nothing stored on one machine."
---

# Comparable runs

Agreed with the owner on 2026-11-23. It replaces the stored approved render samples and the hand-picked latency jobs. It follows the first [design principle](../docs/architecture/principles.md#keep-one-owner-and-one-path): one way to produce a run, one way to compare two.

## Why

- The numerical regression check compared against 7.4 GB of approved images that existed only in a release folder on one test machine. Nothing in the repository could recreate them.
- The latency comparison refused every multi-photo job between 0.8.4 and 0.9.0, because it required identical internal work where the user's action and result were the same.
- Each release grew one-off scripts for the cases the maintained catalog lacked: Color Field, HDR, zoomed detail, memory and reversed order.

## Design

One idea: **a run is a list of measured items, and comparing is matching two lists by item name.** A render case and a behavioral step are both items. An item has a name, its times, and for renders a fingerprint of the image. One report layout and one comparer serve both layers.

Nothing is listed by hand that can be derived, so the checks follow development:

| What | Where it comes from | What a developer does |
|---|---|---|
| Photographs | The two tracked manifests: the example collection the owner publishes with the releases, and the pinned external originals | Sync after uploading a new example |
| Recipes | Recipe files saved from the app, each beside the name of the photograph it edits | Save a recipe to cover a new feature |
| Render cases | Every photograph with camera defaults and with each of its recipes, at each size, at Speed and Quality | Nothing |
| Pipeline stages | The renderer's own profile, by the stage names in the code | Nothing |
| Behavioral steps | Every named step of every scenario | Nothing |

### Render runs

- **Record.** `record REVISION` builds that revision's renderer in a cached checkout and runs the current cases through it. The working tree is a revision. The current tool drives every renderer, so a new recipe gets an earlier figure at once. The renderer's small set of options is the only contract between revisions. It is unchanged between 0.8.4 and now.
- **A run keeps fingerprints and times, not images.** Each item has the image fingerprint, the delivered size, warm render times and one profiled render's stage times. A run is a small report.
- **Compare.** `compare A B` matches items by name. For times it reports the change against the spread of both sides' samples. For pixels it reports which fingerprints differ, then renders those cases again from both revisions to measure the difference. Two runs must come from the same machine.
- **Release record.** The release check compares the previous release tag with the candidate. Its comparison report lists every case whose pixels changed, with the size of the change, and every timing change. The owner reviews it with the changelog, which says what was meant to change. The report is uploaded with the release, beside the candidate's run, so the history lives with the releases and not in the repository (owner, 2026-11-23). There is no tracked list of accepted differences.
- **Mid-release.** The same two commands compare any two revisions.

### Behavioral timing

Every named scenario step records the time from its action to its settled result, as part of the ordinary behavioral run. The run's report is then also the application latency report, compared between releases by step name with the same comparer.

The scenarios currently run through several drivers with their own step bookkeeping. Timing belongs in one place, so it lands with the [coherence cleanup](./coherence-cleanup.md) item that moves every scenario onto one driver. Until then the existing latency jobs stay.

### What this removes

- The stored approved images and the stored-baseline mode of the [render matrix](../scripts/profiling/render_matrix.py).
- The `render-baseline` acceptance job.
- After behavioral timing lands: the latency driver, its ten jobs and its separate comparer.

## Steps

0.9.0:

- [x] **1. Record from a revision.** Build a revision's renderer in a cached checkout and run the existing cases through it. Measure how long a run takes.
- [x] **2. Compare by name.** Match items by photograph, recipe, size and quality. Report time changes against sample spread, and measure pixel differences by rendering changed cases again.
- [ ] **5. Release check.** Compare the previous release tag with the candidate and upload the comparison report and the candidate's run with the release. Replace the `render-baseline` job. Update the [release guide](../docs/releases.md) and [test inventory](../docs/testing-coverage.md). The job, its driver and the stored-baseline mode are removed and the guides updated (2026-11-24). The 0.9.0 comparison against `v0.8.4` and its upload remain.

0.10.0, as these need structural change (owner, 2026-11-23). Steps 6 and 7 wait for the single scenario driver:

- [ ] **3. Derived cases.** Track the example collection's manifest. Derive cases from the manifests and recipe files, and move the existing declared cases' recipes into recipe files.
- [ ] **4. Stage times.** Add one profiled render per item and report stages by name.
- [ ] **6. Step timing.** Record action-to-settled time for every named step in the one driver.
- [ ] **7. Compare behavioral reports** with the same comparer and retire the latency jobs.

## Release measurement (0.10.0)

The 0.9.0 tables were produced by hand-run scripts on the test hosts. Make each of these a maintained part of the release check.

- [ ] **Declared groups.** Track the mixed set and one group of eight per camera family in the [test inputs](../scripts/profiling/test_inputs.json), with Merrill noise reduction on. Add Quattro H photographs so it can have a group, and enough Quattro and Classic photographs that none repeats.
- [ ] **One command for the app comparison.** Run the timing trials and the separate memory pass for both apps and every group, and write one report.
- [ ] **Tables from the report.** Generate the release tables from that report: milliseconds, percentage change, and a camera family's row only where the families differ by more than ten percentage points (owner, 2026-11-24).
- [ ] **A full-size row.** Every 0.9.0 row renders at fitted size, so node savings at full size, such as the skipped Merrill pattern step (68 ms of 471 ms), do not appear.
- [ ] **Keep a trial that differs.** The runner stops a job and discards the trial when its workload differs from earlier trials. Record it and continue, so the difference can be read and one launch does not cost the rest.
- [ ] **Windows hosts.** Script the packaged check for the RTX 3090 and the Intel UHD 630 hosts; it was driven through hand-made scheduled tasks. The integrated host has no Python, so the check there ran the app directly. Measure latency and memory on Windows once the runner no longer needs POSIX process monitoring.
- [ ] **Stage names from the render tool.** The app reports the last GPU stages when a device is lost; the headless tool does not.
- [ ] **Render report with the release.** Record the previous tag and the candidate with the current cases and upload the comparison, as [designed](#render-runs). 0.9.0 shipped without it.

## Open

- How long a full render run takes for one revision. Not measured. If it is too long, recipes run at fewer sizes than defaults.
- Whether timings taken during the ordinary behavioral run, with its screenshots, are steady enough. If not, add a second pass without them.
- Stages renamed or split between two revisions are reported as unmatched.
