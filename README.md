# Foveonic GPU

A personal Merrill X3F processing app with a GUI and a batch CLI, focused on attractive photographs and further editing latitude.

A shared shader-based image engine builds internal processing graphs from selected settings and algorithms. Separate output recipes produce compressed DNGs with editable samples and embedded looks, or TIFFs with white balance, color, and tone baked in.

Development starts with the shortest useful end-to-end conversion path. Detail recovery, denoising, and highlight reconstruction follow incrementally.

## Project documents

Start at [`AGENTS.md`](./AGENTS.md) for repository instructions, specifications, and progress plans.

Maintain the documentation and Rust source indexes with:

```sh
cargo run -p docs-index
cargo run -p docs-index -- --check
cargo test -p docs-index
```
