//! Captures opt-in native workspace checks using isolated preferences and real egui/wgpu frames.

use crate::{
    preferences::{Organizer, Scope, Section},
    App,
};
use eframe::egui;
use std::path::PathBuf;

const TARGETS: [&str; 12] = [
    "preview",
    "final",
    "sensor-gain",
    "gain.layers",
    "list",
    "parade",
    "waveform",
    "zoom",
    "reordered",
    "collapsed",
    "sensor.layers",
    "calibration.layers",
];

pub struct Smoke {
    directory: PathBuf,
    index: usize,
    awaiting: bool,
}
impl Smoke {
    pub fn from_env() -> Option<Self> {
        std::env::var_os("FOVEONIC_GUI_SMOKE_DIR").map(|directory| Self {
            directory: directory.into(),
            index: 0,
            awaiting: false,
        })
    }
    /// Hold processing replies until the embedded preview has been captured.
    pub fn preview_pending(&self) -> bool {
        self.index == 0
    }
}

impl App {
    pub(super) fn smoke_tick(&mut self, ctx: &egui::Context) {
        let Some(smoke) = &mut self.smoke else {
            return;
        };
        let screenshot = ctx.input(|i| {
            i.events.iter().find_map(|e| {
                if let egui::Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = screenshot {
            std::fs::create_dir_all(&smoke.directory).unwrap();
            let bytes: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            image::save_buffer(
                smoke
                    .directory
                    .join(format!("{}.png", TARGETS[smoke.index])),
                &bytes,
                image.width() as u32,
                image.height() as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            smoke.index += 1;
            smoke.awaiting = false;
            if smoke.index == TARGETS.len() {
                // Exercise native preference restoration on a subsequent isolated launch.
                self.preferences.organizer = Organizer::List;
                self.preferences.scope = Scope::Waveform;
                self.preferences
                    .move_section(Section::Metadata, Section::Navigator, false);
                self.preferences.collapsed = [Section::Editing].into();
                self.settings.exposure = 1.25; // deliberately session-only
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            match TARGETS[smoke.index] {
                "final" => {}
                "sensor-gain" => {
                    self.settings.sensor_gain = 1.3;
                    self.submit(None);
                }
                "gain.layers" => {
                    self.output = "gain.layers".into();
                    self.submit(None);
                }
                "list" => {
                    self.preferences.organizer = Organizer::List;
                    self.output = "final".into();
                    self.settings.sensor_gain = 1.;
                    self.submit(None);
                }
                "parade" => self.preferences.scope = Scope::Parade,
                "waveform" => self.preferences.scope = Scope::Waveform,
                "zoom" => self.viewport.one_to_one = true,
                "reordered" => {
                    self.viewport.pan_to(egui::vec2(0.65, 0.5));
                    self.preferences
                        .move_section(Section::Metadata, Section::Navigator, false);
                }
                "collapsed" => self.preferences.toggle(Section::Editing),
                name => {
                    self.viewport.reset();
                    self.preferences = Default::default();
                    self.output = name.into();
                    self.submit(None);
                }
            }
        } else if !smoke.awaiting {
            let ready = if smoke.preview_pending() {
                self.selected
                    .as_ref()
                    .is_some_and(|p| self.browser.thumbnail(p).is_some())
            } else {
                !self.busy && self.frame.is_some()
            };
            if ready {
                smoke.awaiting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
        }
    }
}
