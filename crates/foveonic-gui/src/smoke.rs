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

const NOISE_TARGETS: &[&str] = &[
    "preview",
    "final",
    "reference",
    "noise.layers",
    "noise.edges",
    "detail.ratio",
    "color.rgb",
    "reconstruction.rgb",
    "detail.signal",
    "detail-zero",
    "noise-low",
    "noise-high",
    "color-dq",
    "reference-gain",
    "parade",
    "waveform",
    "zoom",
    "final-neutral",
];

const EDITOR_TARGETS: &[&str] = &[
    "preview",
    "final",
    "reference",
    "manual-decay",
    "parent-changed",
    "relinked",
    "rgb-blue",
    "layers-two",
    "repair-auto",
    "repair-zero",
    "repair-zoom",
    "bolt",
    "white-edge",
    "final-neutral",
];

const SPECKLE_TARGETS: &[&str] = &[
    "preview",
    "final",
    "reference",
    "camera-only",
    "bolt-before",
    "white-before",
    "bolt-after",
    "white-after",
    "despeckle.layers",
    "final-disabled",
    "final-default",
];

const REPAIR_TARGETS: &[&str] = &[
    "preview",
    "final",
    "leaf-before",
    "leaf-after",
    "reference",
    "final-default",
];

pub fn editor_review_active() -> bool {
    std::env::var_os("FOVEONIC_GUI_SMOKE_DIR").is_some()
        && std::env::var_os("FOVEONIC_GUI_SMOKE_EDITOR").is_some()
}

pub struct Smoke {
    directory: PathBuf,
    targets: &'static [&'static str],
    index: usize,
    awaiting: bool,
}
impl Smoke {
    pub fn from_env() -> Option<Self> {
        std::env::var_os("FOVEONIC_GUI_SMOKE_DIR").map(|directory| Self {
            directory: directory.into(),
            targets: if std::env::var_os("FOVEONIC_GUI_SMOKE_REPAIR").is_some() {
                REPAIR_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_DESPECKLE").is_some() {
                SPECKLE_TARGETS
            } else if editor_review_active() {
                EDITOR_TARGETS
            } else if std::env::var_os("FOVEONIC_GUI_SMOKE_NOISE").is_some() {
                NOISE_TARGETS
            } else {
                &TARGETS
            },
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
                    .join(format!("{}.png", smoke.targets[smoke.index])),
                &bytes,
                image.width() as u32,
                image.height() as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            smoke.index += 1;
            smoke.awaiting = false;
            if smoke.index == smoke.targets.len() {
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
            match smoke.targets[smoke.index] {
                "final" => {}
                "leaf-before" | "leaf-after" => {
                    let enabled = smoke.targets[smoke.index] == "leaf-after";
                    self.settings.repair =
                        enabled.then_some(foveonic::render::RepairThreshold::Camera);
                    self.settings.despeckle = enabled;
                    self.settings.reconstruction = None;
                    self.output = "final".into();
                    self.channel = -1;
                    self.viewport.one_to_one = false;
                    self.viewport.zoom = 48.;
                    self.viewport
                        .pan_to(egui::vec2(1582. / 3136., 2737. / 4704.));
                    self.submit(None);
                }
                "camera-only" => {
                    self.settings.reconstruction = Some(foveonic::noise::Controls {
                        chroma: 0,
                        luminance: 0,
                        ..Default::default()
                    });
                    self.settings.despeckle = false;
                    self.viewport.one_to_one = true;
                    self.submit(None);
                }
                "bolt-before" | "bolt-after" => {
                    self.output = "final".into();
                    self.channel = -1;
                    self.settings.despeckle = smoke.targets[smoke.index] == "bolt-after";
                    if smoke.targets[smoke.index] == "bolt-before" {
                        self.viewport.zoom *= 4.;
                    }
                    self.viewport.pan_to(egui::vec2(0.79, 0.05));
                    self.submit(None);
                }
                "white-before" | "white-after" => self.viewport.pan_to(egui::vec2(0.29, 0.757)),
                "final-disabled" | "final-default" => {
                    let enabled = smoke.targets[smoke.index] == "final-default";
                    self.settings.despeckle = enabled;
                    self.settings.repair =
                        enabled.then_some(foveonic::render::RepairThreshold::Camera);
                    self.settings.reconstruction = None;
                    self.output = "final".into();
                    self.channel = -1;
                    self.viewport.reset();
                    self.submit(None);
                }
                "reference" => {
                    if editor_review_active() {
                        self.preferences.collapsed = [Section::Navigator, Section::Scopes].into();
                    }
                    self.settings.reconstruction = Some(Default::default());
                    self.submit(None);
                }
                "manual-decay" => {
                    self.settings
                        .reconstruction
                        .as_mut()
                        .unwrap()
                        .overrides
                        .set(foveonic::noise::Parameter::MeanDecay, Some(0.5));
                    self.submit(None);
                }
                "parent-changed" => {
                    let controls = self.settings.reconstruction.as_mut().unwrap();
                    controls.chroma = 0;
                    controls.luminance = 4;
                    self.submit(None);
                }
                "relinked" => {
                    self.settings
                        .reconstruction
                        .as_mut()
                        .unwrap()
                        .overrides
                        .set(foveonic::noise::Parameter::MeanDecay, None);
                    self.submit(None);
                }
                "rgb-blue" => self.channel = 2,
                "layers-two" => {
                    self.output = "noise.layers".into();
                    self.channel = 1;
                    self.submit(None);
                }
                "repair-auto" | "repair-zero" => {
                    self.settings.reconstruction = Some(Default::default());
                    self.settings.repair = Some(if smoke.targets[smoke.index] == "repair-auto" {
                        foveonic::render::RepairThreshold::Camera
                    } else {
                        foveonic::render::RepairThreshold::Manual(0)
                    });
                    self.output = "final".into();
                    self.channel = -1;
                    self.submit(None);
                }
                "repair-zoom" => self.viewport.one_to_one = true,
                "bolt" => {
                    self.viewport.zoom *= 4.;
                    self.viewport.pan_to(egui::vec2(0.79, 0.05));
                }
                "white-edge" => self.viewport.pan_to(egui::vec2(0.335, 0.75)),
                "detail-zero" | "noise-low" | "noise-high" | "color-dq" | "reference-gain"
                | "final-neutral" => {
                    let mut controls = foveonic::noise::Controls::default();
                    match smoke.targets[smoke.index] {
                        "detail-zero" => controls.detail = 0.,
                        "noise-low" => {
                            controls.chroma = 0;
                            controls.luminance = 0;
                        }
                        "noise-high" => {
                            controls.chroma = 4;
                            controls.luminance = 4;
                        }
                        "color-dq" => self.settings.color_dq_gain = Some(1.),
                        "reference-gain" => {
                            self.settings.color_dq_gain = Some(0.);
                            self.settings.sensor_gain = 1.3;
                        }
                        "final-neutral" => {
                            self.settings.sensor_gain = 1.;
                            self.settings.repair = None;
                            self.settings.despeckle = false;
                            self.channel = -1;
                        }
                        _ => unreachable!(),
                    }
                    self.settings.reconstruction =
                        (smoke.targets[smoke.index] != "final-neutral").then_some(controls);
                    self.output = "final".into();
                    self.viewport.reset();
                    self.submit(None);
                }
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
                    let last_folder = self.preferences.last_folder.clone();
                    self.preferences = Default::default();
                    self.preferences.last_folder = last_folder;
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
