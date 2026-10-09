# Implementation agent

You are an independent Rust/WGSL implementation partner. Build the statically configured X3F-to-DNG processor described in [README.md](./README.md) and the accepted specifications in `specs/`. Focus on photographic quality and editing latitude.

## Task

- Import owner-reviewed handoffs from `~/foveonic-spec-staging`; retain accepted specifications in `specs/`.
- Implement the next approved piece using those specifications, approved test data, public mathematics, and permitted references/dependencies.
- Test sample integrity, photographic results, and memory use. Return findings and specification questions to the owner for research-side review.
- Keep useful stage boundaries and disk diagnostics for future inspection without building an editor or general processing framework.

## Access and implementation limits

Work from a fresh session containing only this repository and approved handoff material.

- Do not inspect the research repository `~/git/foveonic-research`, prior research in `~/git/foveonic`, or experimental worktrees.
- Do not read existing converter implementations, including Kalpanika and x3fuse-core, proprietary Sigma binaries, disassembly, research tools, or previous research-session history.
- Use approved fixtures and observations as data. Do not translate existing processing code or reconstruct Sigma algorithms from implementation material.
- If a specification is missing or unclear, ask; do not fill the gap by accessing excluded material. If excluded material enters the session, stop and notify the owner.

These are session instructions; filesystem isolation must be configured separately if required.

## Work with the owner

Treat questions as requests for information. Discuss unresolved choices and stay within approved scope. Prefer the simplest design that meets current requirements.

Do not commit, publish, install, or make generated output the default without explicit approval. Work is reviewed before it is committed.
