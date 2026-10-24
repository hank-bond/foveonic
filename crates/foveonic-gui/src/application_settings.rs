//! Configures application display and temporary disk reuse independently of photographic edits.

use crate::widgets;
use eframe::egui;
use foveonic::decode_cache::{available_decode_threads, Cache, Settings};

const BYTES_PER_GB: f64 = 1_000_000_000.;
const CACHE_BUDGET_GB: std::ops::RangeInclusive<f64> = 0.1..=1000.;
const BUDGET_DRAG_STEP_GB: f64 = 0.5;
const STATUS_REFRESH: std::time::Duration = std::time::Duration::from_millis(250);
const WINDOW_WIDTH: f32 = 360.;
const DISPLAY_GAMUT_WIDTH: f32 = 180.;
const WINDOW_CENTER_OFFSET: egui::Vec2 = egui::vec2(190., 240.);

pub fn show(
    ctx: &egui::Context,
    open: &mut bool,
    settings: &mut Settings,
    cache: &Cache,
    gamut: &mut crate::preferences::DisplayGamut,
    managed: bool,
) {
    let before = settings.clone();
    egui::Window::new("Settings")
        .open(open)
        .resizable(false)
        .default_width(WINDOW_WIDTH)
        .default_pos(ctx.available_rect().center() - WINDOW_CENTER_OFFSET)
        .show(ctx, |ui| {
            ui.heading("Display");
            ui.horizontal(|ui| {
                ui.label("Gamut");
                let combo = egui::ComboBox::from_id_salt("display-gamut")
                    .width(DISPLAY_GAMUT_WIDTH).selected_text(gamut.label());
                widgets::dropdown(ui, combo, |ui| {
                    for option in crate::preferences::DisplayGamut::ALL {
                        ui.selectable_value(gamut, option, option.label());
                    }
                });
            });
            ui.weak("Monitor uses the display's active color profile.");
            ui.weak("The other choices limit photo colors before display.")
                .on_hover_text("Applies to the viewport, navigator and browser previews. Saved edits and TIFF/DNG output are unchanged.");
            if !managed { ui.weak("This window uses the sRGB display fallback."); }
            ui.separator();
            ui.heading("Image cache");
            widgets::checkbox(ui, &mut settings.enabled, "Enable disk cache");
            ui.add_enabled_ui(settings.enabled, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Maximum space");
                    let mut gb = settings.max_bytes as f64 / BYTES_PER_GB;
                    let budget = egui::DragValue::new(&mut gb)
                        .range(CACHE_BUDGET_GB)
                        .speed(BUDGET_DRAG_STEP_GB)
                        .suffix(" GB");
                    if ui.add(budget).changed() {
                        settings.max_bytes = (gb * BYTES_PER_GB).round() as u64;
                    }
                });
                let cores = available_decode_threads();
                ui.horizontal(|ui| {
                    ui.label("Background decode workers");
                    ui.add(
                        egui::DragValue::new(&mut settings.decode_threads)
                            .range(1..=cores)
                            .speed(1.),
                    );
                })
                .response
                .on_hover_text(format!(
                    "Up to {cores} available cores. Share workers across RAW layers, nearest photos first. \
                     More threads use more CPU and memory. Active photos take priority."
                ));
                ui.horizontal(|ui| {
                    ui.label("Decode neighbors");
                    ui.label("Behind");
                    ui.add(
                        egui::DragValue::new(&mut settings.prefetch_behind)
                            .range(0..=usize::MAX)
                            .speed(1.),
                    );
                    ui.label("Ahead");
                    ui.add(
                        egui::DragValue::new(&mut settings.prefetch_ahead)
                            .range(0..=usize::MAX)
                            .speed(1.),
                    );
                })
                .response
                .on_hover_text(concat!(
                    "Keep these nearby photos decoded around the selected image, ",
                    "following the visible browser order. Zero disables prefetch in that direction.",
                ));
                widgets::checkbox(ui, &mut settings.classic, "Classic");
                widgets::checkbox(ui, &mut settings.merrill, "Merrill");
                widgets::checkbox(ui, &mut settings.quattro, "Quattro");
            });
            ui.separator();
            let status = cache.status();
            let noun = if status.entries == 1 {
                "photograph"
            } else {
                "photographs"
            };
            let activity = if !status.prefetching.is_empty() {
                " · decoding neighbors"
            } else if status.writing {
                " · writing"
            } else {
                ""
            };
            ui.label(format!(
                "{:.2} GB · {} decoded {noun}{activity}",
                status.bytes as f64 / BYTES_PER_GB,
                status.entries,
            ));
            if widgets::button(ui, "Clear cache", false, egui::Vec2::ZERO).clicked() {
                cache.clear();
            }
            ui.weak("Temporary storage only. Entries expire after reboot.")
                .on_hover_text(cache.directory().display().to_string());
            ui.weak("Decoded sources and display previews share this space limit.");
            ui.weak(concat!(
                "Nearby photos are decoded in the background. ",
                "Dots beside filenames show cache readiness.",
            ));
        });
    if &before != settings {
        cache.configure(settings.clone());
    }
    if *open {
        ctx.request_repaint_after(STATUS_REFRESH);
    }
}
