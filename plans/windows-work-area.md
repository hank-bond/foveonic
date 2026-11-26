---
summary: "Finish and release the 0.9.x fix that keeps the restored window inside the Windows work area, clear of the taskbar."
---

# Windows work-area fix

Handoff written 2026-11-26. This work is separate from 0.10.0. The requirement is in [interactive rendering](../docs/specs/interactive-rendering.md).

## Report

A user reported that the app ignores the Windows taskbar location. With the taskbar at the top, the window is drawn behind it. The reporter's Windows version, resolution, scaling and monitor count are unknown.

## Cause

Eframe 0.32.3, egui-winit and winit 0.30.13 place windows against the whole monitor. None reads the work area, which is the monitor area the taskbar leaves free.

- On first launch the app centers a 1440×1000 point workspace on the whole monitor.
- A remembered position is clamped by eframe to the whole monitor, so a position under a top taskbar is kept.
- The resize sent after the loading window was not clamped at all. On 1920×1080 at 150% the workspace was larger than the screen.
- Maximizing already respects the work area.

## Change

Branch `fix/windows-work-area` holds four commits on `v0.9.2`. The same change is uncommitted in the working tree of the branch where 0.10.0 planning happened.

- `Window::restore` in [`startup.rs`](../crates/foveonic-gui/src/startup.rs) fits the workspace's outer rectangle into the work area of the nearest monitor before it sends its viewport commands. A workspace larger than the area shrinks.
- `work_area` asks Windows through `MonitorFromRect` and `GetMonitorInfoW`. This adds the `Win32_Graphics_Gdi` feature of `windows-sys`. Other platforms return no area and are unchanged.
- Unit tests supply their own area. The Windows query is compiled out of tests, because two older placement tests failed on CI when they picked up the runner's monitor.

## Evidence

- **Unit tests.** Five new tests cover a taskbar on each edge, a monitor left of the primary one, a workspace larger than the free area, and the unchanged cases. All startup tests pass on macOS.
- **Independent review.** Nothing blocking. Its small findings were applied. Its open findings are listed under remaining work.
- **Windows desktop.** The app built from the first commit ran on the Windows 11 test host with the taskbar at the top, 1920×1080 at 150%. The work area was (0, 72) to (1920, 1080) in physical pixels. With the remembered 0.9.x position (232, 21) and on a first launch, the outer rectangle equalled the work area exactly. The visible frame sat 9 pixels inside on the left, right and bottom, which is the invisible resize border.
- **Method.** A script ran as a scheduled task with the logged-on user's interactive token, because an SSH session cannot see the desktop. It launched the packaged app, waited for the full window, and recorded the window rectangle, the visible frame from DWM, and the monitor's work area, all read with per-monitor DPI awareness. It backed up and restored the saved window state.

- **Crash at fractional scales, fixed.** At 175% with the top taskbar, the build with the lowered minimum panicked on launch in `fit_within`: `clamp` received crossed bounds (`min = 68.57143, max = 68.57141`), because a window shrunk to fill the area gives `area.max - size` a rounding error below `area.min`. This was in the original fit, not the minimum change; 150% happened to be exact. The fit now takes the top-left on a conflict. A unit test with the 175% values reproduces the old panic and passes with the fix.

## Remaining work

- [x] The Windows test workflow passes on the second commit, `f0318d3`. The first run failed only in the two placement tests described above. A test in the support log group prints an intended child panic, which is not a failure.
- [x] Owner decision (2026-11-26): where the work area is smaller than the 1000×600 point minimum window size, for example 1366×768 at 125%, the minimum gives way to the work area so the whole window stays above the taskbar. On the test host at 1920×1080 and 150%, a 1000×600 point window was usable, and still usable after three Ctrl− interface-scale steps. `Window::fit` caps the minimum inner size at the free area; a unit test covers 1366×768 at 125% and an unchanged roomy area. The specification records the rule. On the test host at 1920×1080 and 175% with the top taskbar (a free area of 1097×569 points), the build at `f81679b` opened with its outer rectangle from (172, 84) to (1920, 1080) physical pixels: the full free height below the taskbar, at the 1000-point minimum width. The owner checked it on the desktop.
- [ ] Check on the desktop a workspace that fits and only needs moving. Both desktop cases shrank, so only unit tests cover the move.
- [ ] Check bottom, left and right taskbars on the desktop where the Windows version allows them.
- [ ] Decide whether to test or document mixed scaling across monitors. The review inferred that a remembered position on a monitor with a different scale may be sized for the wrong one. A second monitor was not available.
- [ ] The review predicted an overshoot of about 10 pixels, because the frame is measured on the loading window, which is not resizable. The desktop run at 150% showed none. Recheck at 100% before closing this.
- [ ] Ask the reporter for their Windows version, resolution, scaling, taskbar position and auto-hide, monitor count, and whether the problem survives deleting the saved window state. Windows 11 does not offer a top taskbar without other tools.
- [ ] Release as a 0.9.x patch following the [release guide](../docs/releases.md): version, changelog, packaged checks on both platforms and the performance comparison. Bring the change to the main line as well.

## Notes for the next agent

- Access to the Windows test host comes from the owner. It has no Rust toolchain, so builds come from the Windows test workflow.
- The owner changed the host's taskbar position and scaling for this test and may change them again. Ask before changing system settings.
- The test build was left in a scratch folder under the host's temporary directory. The scheduled task was removed and the saved window state was restored.
- Do not commit, tag or publish without owner approval.
