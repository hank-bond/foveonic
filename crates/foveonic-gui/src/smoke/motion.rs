//! Drives a timed pointer path across native input frames without slowing the path when drawing stalls.

use eframe::egui;
use foveonic::diagnostics::{self as trace, Category};
use serde::Serialize;
use std::time::{Duration, Instant};

#[derive(Serialize)]
struct Sample {
    seconds: f64,
    position: [f32; 2],
}

pub(super) struct Motion {
    name: &'static str,
    center: egui::Pos2,
    offset: egui::Vec2,
    duration: Duration,
    started: Option<Instant>,
    ended: Option<Instant>,
    samples: Vec<Sample>,
}

impl Motion {
    pub(super) fn new(name: &'static str, center: egui::Pos2, offset: egui::Vec2) -> Self {
        Self {
            name,
            center,
            offset,
            duration: Duration::from_secs(3),
            started: None,
            ended: None,
            samples: Vec::new(),
        }
    }

    pub(super) fn finished(&self) -> bool {
        self.ended.is_some()
    }

    pub(super) fn inject(&mut self, input: &mut egui::RawInput, ctx: &egui::Context) {
        self.inject_at(input, ctx, Instant::now());
    }

    fn inject_at(&mut self, input: &mut egui::RawInput, ctx: &egui::Context, now: Instant) {
        if self.finished() {
            return;
        }
        let started = *self.started.get_or_insert(now);
        let elapsed = now.duration_since(started);
        let fraction = (elapsed.as_secs_f64() / self.duration.as_secs_f64()).min(1.);
        let distance = (fraction * std::f64::consts::TAU).sin() as f32;
        let point = self.center + self.offset * distance;
        input.events.push(egui::Event::PointerMoved(point));
        self.samples.push(Sample {
            seconds: elapsed.as_secs_f64(),
            position: [point.x, point.y],
        });
        if elapsed >= self.duration {
            self.ended = Some(now);
        }
        ctx.request_repaint();
    }

    pub(super) fn evidence(self) -> serde_json::Value {
        let started = self.started.expect("motion was injected");
        let ended = self.ended.expect("motion finished before observation");
        if let (Some(start), Some(end)) = (trace::timestamp_at(started), trace::timestamp_at(ended))
        {
            trace::interval(Category::Scenario, "loupe.motion", start, end, |fields| {
                fields
                    .str("motion", self.name)
                    .u64("samples", self.samples.len() as u64)
                    .str("measurement", "in-process input cadence");
            });
        }
        serde_json::json!({"motion":self.name,"duration_seconds":ended.duration_since(started).as_secs_f64(),
            "coverage":"egui-input","samples":self.samples})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stalled_frames_skip_to_the_current_time_instead_of_replaying_old_positions() {
        let ctx = egui::Context::default();
        let mut motion = Motion::new("test", egui::pos2(20., 30.), egui::vec2(8., 0.));
        let start = Instant::now();
        for (seconds, expected) in [(0., 20.), (0.75, 28.), (2.25, 12.), (3.5, 20.)] {
            let mut input = egui::RawInput::default();
            motion.inject_at(&mut input, &ctx, start + Duration::from_secs_f64(seconds));
            assert_eq!(input.events.len(), 1);
            let egui::Event::PointerMoved(point) = input.events[0] else {
                panic!("missing motion")
            };
            assert!((point.x - expected).abs() < 0.0001);
            assert_eq!(point.y, 30.);
        }
        assert!(motion.finished());
        let mut input = egui::RawInput::default();
        motion.inject_at(&mut input, &ctx, start + Duration::from_secs(4));
        assert!(input.events.is_empty());
        assert_eq!(motion.evidence()["samples"].as_array().unwrap().len(), 4);
    }
}
