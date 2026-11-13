---
summary: "Native app checks keep scenario expectations separate from shared input, capture and observation support."
---

# Native application checks

Follow the parent GUI source instructions and the [behavioral testing plan](../../../../plans/behavioral-testing.md). Keep each feature's actions, readiness and assertions together. Shared support observes application behavior without reproducing processing or scheduling policy. Preserve the opt-in environment, stable scenario names and evidence formats.

Run native checks serially on a quiet host. Keep controlled delivery pauses separate from natural latency measurements. Independent test expectations must remain distinct from the application state they check.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[background.rs](./background.rs)** Drives a foreground exposure edit after a real import batch starts.
- **[browser_journey.rs](./browser_journey.rs)** Exercises browser commands and search against independently declared staged paths.
- **[cache_workflow.rs](./cache_workflow.rs)** Measures Smart detail viewports while two photos receive shared exposure edits.
- **[capture.rs](./capture.rs)** Saves existing smoke checks through the shared capture controller without changing their actions.
- **[choices.rs](./choices.rs)** Drives the shared white-balance menu and Tone bypass through egui input.
- **[collection.rs](./collection.rs)** Exercises bounded editing in a large folder of explicit source aliases.
- **[damaged.rs](./damaged.rs)** Checks recovery after the running app attempts to load an isolated truncated RAW.
- **[journey.rs](./journey.rs)** Tracks independent per-photo Exposure expectations for native journeys.
- **[lifecycle.rs](./lifecycle.rs)** Exercises recipe persistence and import review in the running application.
- **[loupe.rs](./loupe.rs)** Exercises held native-detail gestures, cancellation and transient photo ownership in the real app.
- **[mod.rs](./mod.rs)** Captures opt-in native workspace checks and the recipe-driven README image using isolated real GPU frames.
- **[multi_edit.rs](./multi_edit.rs)** Exercises bounded multi-photo selection, edits and retained image delivery.
- **[rotate.rs](./rotate.rs)** Keeps rotation smoke gestures, readiness and assertions with their capture steps.
- **[scenarios.rs](./scenarios.rs)** Runs declared native diagnostic cases through the existing application and capture controller.
- **[screenshot.rs](./screenshot.rs)** Loads the fixed README screenshot recipe and defines its capture steps.
- **[selection.rs](./selection.rs)** Drives and observes a bounded viewport selection journey during held delivery.
- **[slider.rs](./slider.rs)** Drives and records a real multi-photo slider gesture in a native scenario.
- **[source_loading.rs](./source_loading.rs)** Proves a ready photo can be edited and rendered while another source load is held.
- **[transitions.rs](./transitions.rs)** Runs replayable transitions through the existing native scenario.

<!-- INDEX:END -->
