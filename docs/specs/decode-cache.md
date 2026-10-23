---
summary: "A bounded temporary disk LRU reuses immutable decoded sources and display previews with per-family workspace controls."
kind: reference
status: accepted
implementation: conforming
---

# Decoded source cache

The app must offer a settings panel for decoded-source caching. The cache defaults to enabled with a 10 GB decimal disk budget. Merrill and Quattro default on, Classic off. Each family has its own checkbox. These are workspace preferences, separate from photo editing sidecars.

Storage must stay under the platform temporary directory. Cache identity includes canonical source location, modification time and file length, plus the decoded-format version. A moved or modified original must not reuse another source's decoded samples. The cache must retain the original sensor layout, sample values, camera matrix encodings and property-list order. Photo edits do not invalidate decoded data.

Successful reuse updates LRU recency. Eviction removes least recently used entries until the configured budget is met. Cache writes and maintenance run in bounded background work and must not delay image delivery. Failed or incomplete cache entries fall back to original decoding. Cache failures must not prevent ordinary rendering. Platform reboot boundaries invalidate retained entries. The app must not offer an arbitrary cache location outside temporary storage.

The settings panel must offer the master switch, disk budget, architecture checkboxes, cache usage and a clear-cache action. Turning caching off retains existing disposable files until eviction, clearing or reboot, but prevents their use and further insertion. [Display previews](./browser-previews.md) share this temporary directory, source identity, architecture policy, LRU eviction, and byte budget. Clearing removes both kinds of entry. Usage includes preview bytes while the photograph count describes decoded sources. No multi-photo decoded RAM cache is introduced.

The app must predecode eligible files in the currently opened directory using one background worker and a bounded pending write. Prioritize remaining files near the current selection. Respect all architecture switches for prefetch as well as foreground reuse. Queued and active photo rendering and scopes take priority. Background reading, decoding and insertion must yield at bounded checkpoints, with lower CPU and I/O priority on macOS. Other decoded photographs must not accumulate in RAM.

Grid and list previews must show small muted status dots beside filenames. In the grid, place the dot at the bottom right in the caption area, outside the photograph. Gray means disabled or unsupported caching, yellow means queued or being prepared, green means ready, and red means supported but currently uncached. Red may follow eviction, clearing, a failed insertion or an entry larger than the budget. Hover text must explain the state. Unknown eligibility may remain yellow while metadata checking is queued. The badge must follow source identity, policy, clearing and eviction. Readiness scanning runs off the UI thread. A corrupt entry still falls back to original decoding when opened. Clearing the cache stops the current prefetch pass. A folder larger than the budget must not cause an endless decode/evict loop. Revisiting the directory or changing policy may start another pass.
