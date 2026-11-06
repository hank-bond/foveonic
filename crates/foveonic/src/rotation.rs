//! Composes saved quarter turns and fine rotation with an interior photographic crop.

use crate::{crop::Crop, image::View, source::Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Rotation {
    pub turns: u8,
    /// Clockwise fine angle, independent of accumulated quarter turns.
    pub degrees: f64,
}
impl Rotation {
    pub fn is_neutral(self) -> bool {
        self.turns == 0 && self.degrees == 0.
    }
    pub fn validate(self) -> Result<()> {
        if self.turns > 3 || !self.degrees.is_finite() || !(-45. ..=45.).contains(&self.degrees) {
            return Err(
                "rotation requires 0–3 quarter turns and a fine angle in -45..45 degrees".into(),
            );
        }
        Ok(())
    }
    pub fn turn(&mut self, clockwise: bool, crop: &mut Crop) {
        self.turns = (self.turns + if clockwise { 1 } else { 3 }) % 4;
        let [l, t, r, b] = crop.bounds;
        crop.bounds = if clockwise {
            [1. - b, l, 1. - t, r]
        } else {
            [t, 1. - r, b, 1. - l]
        };
        if matches!(crop.aspect, crate::crop::Aspect::Ratio(_)) {
            crop.flipped = !crop.flipped;
        }
    }
    pub fn reset(&mut self, crop: &mut Crop) {
        while self.turns != 0 {
            self.turn(false, crop);
        }
        self.degrees = 0.;
    }
    /// Apply authored quarter turns to camera-oriented dimensions, before fine-angle trimming.
    pub fn oriented_extent(self, original: [u32; 2]) -> [u32; 2] {
        if self.turns.is_multiple_of(2) {
            original
        } else {
            [original[1], original[0]]
        }
    }
    pub fn extent(self, original: [u32; 2]) -> [u32; 2] {
        let base = self.oriented_extent(original);
        let scale = self.interior_scale(base);
        base.map(|v| (f64::from(v) * scale).floor().max(1.) as u32)
    }
    /// Saved framing within the untrimmed, quarter-turned editing view.
    pub fn framing_bounds(self, original: [u32; 2], crop: Crop) -> [f64; 4] {
        let oriented = self.oriented_extent(original);
        let scale = self.interior_scale(oriented);
        crop.bounds.map(|v| 0.5 + (v - 0.5) * scale)
    }
    fn interior_scale(self, extent: [u32; 2]) -> f64 {
        let (s, c) = self.degrees.to_radians().sin_cos();
        let [w, h] = extent.map(f64::from);
        (w / (c.abs() * w + s.abs() * h)).min(h / (s.abs() * w + c.abs() * h))
    }
    pub(crate) fn view(self, mut view: View, crop: Crop, output: Option<[u32; 2]>) -> View {
        view.rotation = (view.rotation + u32::from(self.turns) * 90) % 360;
        if !self.turns.is_multiple_of(2) {
            view.extent.swap(0, 1);
        }
        if self.degrees == 0. {
            return crop.apply(view, output);
        }
        let base = view.extent;
        let scale = self.interior_scale(base);
        let natural = base.map(|v| (f64::from(v) * scale).floor().max(1.) as u32);
        let [x0, y0, x1, y1] = crop.pixels(natural);
        view.extent = output.unwrap_or([x1 - x0, y1 - y0]);
        view
    }
    /// Map normalized output coordinates directly to physical storage. The fourth slot enables mapping.
    pub(crate) fn mapping(self, mut view: View, crop: Crop) -> [[f32; 4]; 2] {
        if self.degrees == 0. {
            return [[0.; 4]; 2];
        }
        view.rotation = (view.rotation + u32::from(self.turns) * 90) % 360;
        let size = self.oriented_extent(view.extent).map(f64::from);
        let [l, t, r, b] = view.crop;
        let storage = if view.rotation.is_multiple_of(180) {
            [r - l, b - t]
        } else {
            [b - t, r - l]
        }
        .map(f64::from);
        let pixel_scale = [storage[0] / size[0], storage[1] / size[1]];
        let scale = self.interior_scale(size.map(|v| v as u32));
        let (s, c) = self.degrees.to_radians().sin_cos();
        let point = |u: f64, v: f64| {
            let q = [
                (u * (crop.bounds[2] - crop.bounds[0]) + crop.bounds[0] - 0.5) * size[0] * scale,
                (v * (crop.bounds[3] - crop.bounds[1]) + crop.bounds[1] - 0.5) * size[1] * scale,
            ];
            let p = [
                (c * q[0] + s * q[1]) * pixel_scale[0] + (storage[0] - 1.) * 0.5,
                (-s * q[0] + c * q[1]) * pixel_scale[1] + (storage[1] - 1.) * 0.5,
            ];
            let p = match view.rotation {
                90 => [p[1], f64::from(b - t - 1) - p[0]],
                180 => [f64::from(r - l - 1) - p[0], f64::from(b - t - 1) - p[1]],
                270 => [f64::from(r - l - 1) - p[1], p[0]],
                _ => p,
            };
            [p[0] + f64::from(l), p[1] + f64::from(t)]
        };
        let origin = point(0., 0.);
        let x = point(1., 0.);
        let y = point(0., 1.);
        std::array::from_fn(|i| {
            [
                (x[i] - origin[i]) as f32,
                (y[i] - origin[i]) as f32,
                origin[i] as f32,
                1.,
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn turns_preserve_fine_angle_and_crop_content_and_cycle_back() {
        let mut rotation = Rotation {
            degrees: 12.,
            ..Default::default()
        };
        let mut crop = Crop {
            bounds: [0.1, 0.2, 0.8, 0.9],
            ..Default::default()
        };
        let original = crop;
        let settings = crate::render::Settings {
            rotation,
            crop,
            ..Default::default()
        };
        settings.validate_values().unwrap();
        assert_eq!(
            serde_json::from_slice::<crate::render::Settings>(
                &serde_json::to_vec(&settings).unwrap()
            )
            .unwrap(),
            settings
        );
        for _ in 0..4 {
            rotation.turn(true, &mut crop);
            assert_eq!(rotation.degrees, 12.);
        }
        assert_eq!(rotation.turns, 0);
        for (a, b) in crop.bounds.into_iter().zip(original.bounds) {
            assert!((a - b).abs() < 1e-12);
        }
    }
    #[test]
    fn fine_rotation_mapping_stays_inside_camera_bounds_for_every_orientation() {
        for turns in 0..4 {
            for degrees in [-45., -13., 0.1, 22., 45.] {
                let rotation = Rotation { turns, degrees };
                let mapping = rotation.mapping(
                    View {
                        crop: [10, 20, 1010, 1520],
                        rotation: 90,
                        extent: [1500, 1000],
                    },
                    Crop::default(),
                );
                for x in [0., 1.] {
                    for y in [0., 1.] {
                        let point: [f64; 2] = std::array::from_fn(|i| {
                            f64::from(mapping[i][0]) * x
                                + f64::from(mapping[i][1]) * y
                                + f64::from(mapping[i][2])
                        });
                        assert!(
                            point[0] >= 9.49
                                && point[0] <= 1010.01
                                && point[1] >= 19.49
                                && point[1] <= 1520.01,
                            "{rotation:?} {point:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn fine_rotation_uses_photographic_angles_with_rectangular_sensor_pixels() {
        let view = View {
            crop: [64, 18, 4768, 1586],
            rotation: 90,
            extent: [2176, 3264],
        };
        for turns in 0..4 {
            let rotation = Rotation {
                turns,
                degrees: 17.,
            };
            let size = rotation.extent(view.extent);
            assert_eq!(rotation.view(view, Crop::default(), None).extent, size);
            let mapping = rotation.mapping(view, Crop::default());
            let point = |u: f32, v: f32| -> [f64; 2] {
                std::array::from_fn(|i| {
                    f64::from(mapping[i][0] * u + mapping[i][1] * v + mapping[i][2])
                })
            };
            let left = point(0., 0.5);
            let right = point(1., 0.5);
            // Measure the mapped horizontal axis back in photographic pixels.
            let dx = (right[0] - left[0]) * 3264. / 4704.;
            let dy = (right[1] - left[1]) * 2176. / 1568.;
            let angle = dy.atan2(dx).to_degrees();
            let expected = -90. + f64::from(turns) * -90. - 17.;
            let error = (angle - expected + 180.).rem_euclid(360.) - 180.;
            assert!(error.abs() < 0.001, "{turns}: {angle} vs {expected}");
            for u in [0., 1.] {
                for v in [0., 1.] {
                    let [x, y] = point(u, v);
                    assert!((63.49..=4768.01).contains(&x));
                    assert!((17.49..=1586.01).contains(&y));
                }
            }
        }
    }
}
