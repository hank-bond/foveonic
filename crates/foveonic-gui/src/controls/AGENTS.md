---
summary: "Family panels compose shared editing widgets while the processor defines supported controls."
---

# Editing controls

Choose the processing family once in the parent dispatcher. Each family panel owns its layout and includes the controls that describe its processing. Shared rows own widget behavior and photographic controls. Keep photographic processing and capability/default resolution in `foveonic`.

Construct numeric controls through `widgets::slider` and `widgets::drag_value`. These install the shared display policy of at most three significant digits and two decimal places. Do not instantiate raw egui numeric widgets or override their formatter in production call sites. A source-boundary test enforces this. Preserve stored precision and measure the same shared text when reserving value-field width. Both themes use the policy and suppress built-in full-precision hints.

Ordinary icon controls use `widgets::icon_button`, `widgets::icon_menu_button` and `widgets::icon_rect` for shared sizing, padding and states. Compact header actions retain their dedicated layout.

Editing tool order lives in application preferences. Family panels declare supported tools, and the shared section arranger supplies stable tool identities and accepts typed editing-tool drops. Reordering must not report an image edit or alter pipeline order. Tool headers share the scaled workspace grip.

Camera metadata supplies white-balance and color-mode choices. Normalize unsupported settings through the processor's control contract when metadata arrives. Adding a family should add a panel without adding family switches to shared widgets.

<!-- INDEX:START -->

<!-- Generated from source summaries and document frontmatter. Do not edit by hand. Run `cargo run -p docs-index`. -->

### Source files

- **[advanced_color.rs](./advanced_color.rs)** Composes the Advanced range wheel and saved adjustment list with shared controls.
- **[classic.rs](./classic.rs)** Composes Classic controls using the camera's built-in reconstruction and corrections.
- **[color_editor.rs](./color_editor.rs)** Selects color ranges for shared linear Hue, Saturation and Lightness adjustments.
- **[color_wheel.rs](./color_wheel.rs)** Draws and edits hue and saturation bounds with continuous circular handle gestures.
- **[common.rs](./common.rs)** Shares editing rows and photographic controls used by family panels.
- **[dehaze.rs](./dehaze.rs)** Presents Dehaze strength, automatic shadow tone, a color swatch and on-image picking.
- **[merrill.rs](./merrill.rs)** Composes Merrill color, monochrome, reconstruction and correction controls.
- **[merrill_noise.rs](./merrill_noise.rs)** Composes Merrill denoising, speckle, shadow-neutrality and independent banding controls.
- **[quattro.rs](./quattro.rs)** Composes Quattro color and monochrome controls using file-derived defaults.
- **[saturation.rs](./saturation.rs)** Composes independent Reference and Universal saturation with visible tonal amounts.
- **[sections.rs](./sections.rs)** Arranges supported editing tools without changing their processing or saved image settings.

<!-- INDEX:END -->
