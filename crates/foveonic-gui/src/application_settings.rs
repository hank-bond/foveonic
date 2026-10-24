//! Configures temporary source and preview disk reuse independently of photographic edits.

use crate::widgets;
use eframe::egui;
use foveonic::decode_cache::{available_decode_threads, Cache, Settings};

const BYTES_PER_GB: f64 = 1_000_000_000.;
const CACHE_BUDGET_GB: std::ops::RangeInclusive<f64> = 0.1..=1000.;
const BUDGET_DRAG_STEP_GB: f64 = 0.5;
const STATUS_REFRESH: std::time::Duration = std::time::Duration::from_millis(250);
const WINDOW_WIDTH: f32 = 360.;
const WINDOW_CENTER_OFFSET: egui::Vec2 = egui::vec2(190., 150.);

pub fn show(ctx: &egui::Context, open: &mut bool, settings: &mut Settings, cache: &Cache) {
    let before = settings.clone();
    egui::Window::new("Settings")
        .open(open)
        .resizable(false)
        .default_width(WINDOW_WIDTH)
        .default_pos(ctx.available_rect().center() - WINDOW_CENTER_OFFSET)
        .show(ctx, |ui| {
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
                    ui.label("Background decode threads");
                    ui.add(
                        egui::DragValue::new(&mut settings.decode_threads)
                            .range(1..=cores)
                            .speed(1.),
                    );
                })
                .response
                .on_hover_text(format!(
                    "Up to {cores} available cores. Decode several photos ahead together. \
                     More threads use more CPU and memory. Active photos take priority."
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
                " · decoding folder"
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
                "The open folder is decoded ahead in the background. ",
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
