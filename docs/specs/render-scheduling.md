---
summary: "One core Scheduler owns the prioritized job queue and the working set, admits jobs against the user memory target and advances them together one step per tick."
kind: reference
status: accepted
implementation: incomplete
---

# Render scheduling

The owner agreed this design on 2026-11-17 and 2026-11-18 after reviewing the 0.9.0 wide-rendering candidate. It replaces the separate serial and wide execution paths with one way to run every render. Names follow the [shared vocabulary](../architecture/concepts.md#scheduling-vocabulary). The [completion plan](../../plans/render-scheduler.md) tracks implementation. Application request ownership and result handling remain in [render orchestration](./render-orchestration.md).

## Goals

The user sets one memory target. It is the steady-state memory the app aims to hold, planned in whole jobs from estimates, not a hard cap. Peak use should stay close to it. The target has no artificial upper bound, so larger machines can raise it freely. The app uses it to balance speed and memory without exposing queue sizes, widths or other execution settings.

Running two to four photographs together while their working photos stay in memory is the fastest case. Rebuilding an evicted working photo costs a decode (about 25–400 ms on an M1 Pro, depending on the camera family) plus its graph (about 110 ms for Classic to 1.2 s for Quattro H at full resolution), while running wider gains roughly 100 ms. Avoiding thrashing of what the editing targets need therefore takes precedence over running wider: their working photos are never released to widen a group. Cache is different. It is released so the group can render together, because nothing on screen is waiting on it.

There must be one place and one way to do each thing. Prefer simple rules whose behavior follows from the target over special cases and defensive fallbacks.

## Jobs and the queue

- Every render **must** be a job submitted to one queue owned by the Scheduler. This includes viewport images, auxiliary images, browser thumbnails, color picks, scopes, exports, CLI conversions and speculative work.
- Each job **must** carry a purpose and a target. The purpose sets the job's priority and how its use counts toward keeping memory. A target identifies the photo and the requested view, so a requester can resubmit without retaining job state.
- Submitting a job for a target that is already queued **must** replace the queued job and take the new job's priority. Within a tier, targets are served in the order they joined the queue. Replacing a queued job **must not** reset its place. A newer job for a target that is running **must** join the back, so that when new jobs arrive faster than every target can run, each target is served in turn and none is served repeatedly while others wait.
- A requester **must** be able to cancel by job or by target. A job **must** end as finished, failed or cancelled. User-requested exports are not cancelled by replacement.
- Replacing a running job **must** cancel it unless finishing it remains useful. It remains useful when the newer request changes only the view, or only parameters the running job already recomputes. Otherwise its intermediate results would be invalid anyway.
- A job that needs sensor data **must** wait for it without occupying execution. Other jobs continue meanwhile. Waiting does not settle or cancel the job. [Render orchestration](./render-orchestration.md#communication-and-completion) covers how sources are acquired.

## Priority

One `Priority` type in core **must** define the order and govern both the queue and the shared cross-thread gate. Decode threads, file reads and render execution **must** use the same tiers. No other component may define its own priority ordering.

| Tier | Work |
| --- | --- |
| Viewport | Images shown in the editor's photo viewports, including zoomed regions and Before/After. |
| Auxiliary | Navigator images and scopes. |
| Thumbnail | Browser thumbnails needed to use the app. |
| Export | User-requested file output from the app or CLI. |
| Speculative | Run-ahead work: decoding neighbors into the disk cache, preparing reduced working photos ahead of use and full-image completion after a zoomed region. |

Higher tiers come first. Export ranks above Speculative so that, once interaction stops, exports run at roughly command-line speed. Speculative work uses leftover compute. Requested decoding for a job keeps that job's tier. Only run-ahead decoding is Speculative. Work in a lower tier **must** yield to new higher-tier demand at its next step or I/O checkpoint and return to the queue without reporting failure. Its stored intermediate results remain in its working photo, so a retry repeats little work. Earlier finer orderings, such as prefetching visible photos before invisible thumbnails, may return later as ordering within a tier.

## Working set

Memory serves the photos being edited. The rules below follow from seven statements:

1. Photos in the editing selection come first. Their working photos, the stages their latest edit restarts from and every image on screen are kept and are never released to make room for anything else.
2. Room is the target less what the first statement keeps and less one render of every editing target together.
3. Everything else is cache. It stays only in that room and is released oldest first. This includes working photos of displayed photos outside the editing selection, Native working photos left after zooming out, and photos no longer displayed.
4. A stage is held in one version. A newer version replaces the older at once.
5. Within an editing target, only the stages its latest edit restarts from are kept. Its other valid stages are cache.
6. A group renders as wide as it can. Cache is released whenever that lets another editing target render alongside the rest. Targets that still cannot render together take turns, and none is served twice while another waits.
7. Disk is the tier underneath. The [decode cache](./decode-cache.md) already holds decoded sensor data where it is enabled, so releasing a working photo costs a read to rebuild, or a decode where no disk copy exists.

- The Scheduler **must** own the working set. The working set holds everything rendering keeps in memory: working photos and cached images.
- A working photo **must** hold one photo at one resolution: Native or Reduced. Native keeps full-resolution sensor data. Reduced keeps sensor data downsampled to its working scale, so the file is no longer needed. Quality and Speed are a separate processing choice applied to either resolution.
- A job **must** hold a guard on its working photo while it runs. A guarded entry **must not** be dropped or replaced. Only one admitted job **must** use a photo at a time. Jobs for the same photo run in turn.
- Every time a job uses a working photo, the working set **must** record the use with the job's purpose. One table in the working set decides what each use refreshes:

  | Purpose | Priority | Refreshes |
  | --- | --- | --- |
  | Edit: the edit shown in a viewport | Viewport | The working photo and its edit's restart inputs |
  | View: other viewport work, such as Before images and color picks | Viewport | The working photo |
  | Auxiliary: navigator images and scopes | Auxiliary | The working photo |
  | Thumbnail | Thumbnail | The working photo |
  | Export | Export | Nothing |
  | Speculative | Speculative | Nothing |

- A working photo created only for an export **must** be released when the export finishes unless other use retains it.
- There is no single active photo. Recency, guards and one trimming order **must** give one photo and several photos the same protection.
- Trimming **must** return memory to the target through one ordering. It drops cached images, optional checkpoints, and preparation no retained photo owns first. Then one recency pass drops whichever was used longest ago: an edit's restart inputs or a whole working photo that is not eviction protected. A photo's own restart inputs go before the photo, so its sensor data, which needs a decode to rebuild, is dropped last.
- A render that completes **must** release every version that no image's latest render planned, so earlier values of a control are never kept. An edit **must** also record the stages its image restarts from, and each image keeps its own. A working photo can produce several images: a detail patch, the whole image, and a render that is not an edit such as a Before image. Each restarts from its own stages, because a patch computes them over its region alone and a Before image uses other settings. Each image therefore holds a stage in one version, and none replaces another's. Only an edit decides which stages the photo restarts from, so a Before image cannot displace the current edit's inputs. A photo's restart inputs carry the use of its latest edit, which orders them against other photos' inputs and working photos. In multi-edit, one change's inputs get adjacent recency across photos, so they leave close together. Valid stages the latest edit does not restart from, such as noise stages after a tone edit, are optional checkpoints and are dropped before any restart input.
- A working photo keeps its sensor data until it is evicted.
- Eviction-protected working photos **must not** be evicted, and each keeps the restart inputs of its latest edit. A working photo is eviction protected while the latest plan has unfinished work from it, or its photo is in the editing selection. No photo is protected for being viewed or selected most recently. Any other photo shown only with finished images, or kept as history, may lose its working photo whenever the editing targets need the room, and its next render reloads it. Protection is per working photo, so a Native photo left over from zooming in ages out normally. When eviction-protected photos alone exceed the target, the overage is reported instead of rebuilding them after every render.

## Cached images

- An image the GUI may show again but does not draw, such as a Before image after returning to After, **must** move into the image cache. The GUI **must** hold only a weak handle to it and take it back when it shows it again.
- Cached images **must** be dropped before any checkpoint or working photo. When the GUI finds a weak handle empty, it resubmits the job.
- The GUI **must not** report its own retained memory separately. The working set accounts for all of it.

## Admission and width

- The first non-speculative job in a tick **must** always run. Trimming after the tick returns memory to the target.
- Another job **must** be admitted only when its estimated peak fits the target alongside retained entries and admitted jobs. To make a Viewport job fit, admission **must** release cache: cached images first, then whatever the working set holds that no admitted job and no eviction-protected photo needs, in the trimming order. Admission is what makes the room of the second statement: the trim after a render returns memory to the target, and a job running alone may peak above it by its own render. Admission **must not** release cache that cannot make the room. When an upper estimate of all of it is less than the job lacks, the cache stays and the job waits its turn: a drag retries admission after every render step, and releasing each time made a six-photo drag show a third fewer updates. Work in other tiers runs wider only in free memory.
- A job that must load or prepare its working photo **must** start only when no other job is admitted. A job that does not fit or must load alone retries after another job finishes or retires, after trimming frees memory, or when the target changes, not on every tick.
- Speculative jobs **must never** cause eviction. They run only when their delivered result fits in free memory. The target describes the steady state, so their transient peak is not counted.
- Width **must** be the number of jobs admitted under these rules. Users **must not** see a width or concurrency setting. Performance tests and profiling tools may cap width through one Scheduler setting that is off by default.
- One tick **must** contain jobs from one priority tier only.
- Every job **must** produce a memory estimate for every camera family, resolution, region, quality and diagnostic mode. The estimate is the peak new memory the remaining steps need: outputs accumulate, released inputs become reusable scratch, and each step's temporaries add while it runs. Diagnostics and profiling **must not** change estimates or admission. Until a case is covered, that job runs alone. Treat the gap as a defect to close, not a fallback.
- Between ticks, if retained memory has grown and admitted jobs no longer fit, the Scheduler **must** release cache as admission does and then return the latest-submitted jobs to the queue.

## Execution

- Every render **must** use one execution path. A job steps through its graph: traversal skips cached nodes and runs the next uncached node. Each tick, every admitted job takes one step, followed by one GPU wait. A single job is a tick of one.
- A tick with one job **must** wait for the previous tick's work, so one step stays in flight and the GPU does not idle while the next step is prepared. This bounds unfinished GPU work, and the storage it holds, to one step. A tick with several jobs, a job's last step and an error **must** wait for all submitted work: several jobs keep the GPU busy by themselves, and their steps would otherwise all stay in flight. A newer or higher-priority job can therefore start behind at most one step of a job that was running alone.
- Export **must** be an ordinary job: the same graph, stepped by ticks like any other, with file writing in place of display output when it finishes. The CLI **must** submit export jobs to a Scheduler with a target it chooses. Neither keeps its own execution path.
- Progress **must** be reported after the tick's GPU wait, so a step's progress can show one tick before its work has finished.
- Failure or cancellation of one job **must** retire its submitted work before releasing its guard. Peers continue and settle independently.
- Output **must** be identical at every width. Every camera family **must** be eligible to run with others.
- GPU profiling **must** work at any width.

## Validation

- Compare outputs at width one and at wider widths for Classic, Merrill, Quattro and Quattro H, including Native regions and Reduced views, Quality and Speed.
- A slider drag while speculative work runs **must not** cause the next step to recompute upstream of the changed parameter.
- A Before render **must not** remove the current edit's restart inputs.
- Measure latency and memory against published 0.8.4 under the [release performance checks](../releases.md#compare-performance-before-publication).
