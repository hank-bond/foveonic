//! Composes saved oriented crop coordinates with camera framing and final output sizing.

use crate::{image::View, source::Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aspect {
    Unconstrained,
    #[default]
    Original,
    Ratio([f64; 2]),
}

/// Bounds are normalized, half-open edges of the complete oriented photograph.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Crop {
    pub bounds: [f64; 4],
    pub aspect: Aspect,
    pub flipped: bool,
}
impl Default for Crop {
    fn default() -> Self {
        Self {
            bounds: [0., 0., 1., 1.],
            aspect: Aspect::Original,
            flipped: false,
        }
    }
}
impl Crop {
    pub fn is_full(self) -> bool {
        self.bounds == [0., 0., 1., 1.]
    }

    pub fn validate(self) -> Result<()> {
        let [l, t, r, b] = self.bounds;
        if self
            .bounds
            .iter()
            .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
            || l >= r
            || t >= b
        {
            return Err("crop must have positive bounds inside the photograph".into());
        }
        if let Aspect::Ratio(ratio) = self.aspect {
            if ratio.iter().any(|v| !v.is_finite() || *v <= 0.)
                || !(ratio[0] / ratio[1]).is_finite()
                || ratio[0] / ratio[1] <= 0.
            {
                return Err("crop ratio dimensions must be finite and positive".into());
            }
        }
        Ok(())
    }

    pub fn ratio(self, extent: [u32; 2]) -> Option<f64> {
        if extent.contains(&0) {
            return None;
        }
        let ratio = match self.aspect {
            Aspect::Unconstrained => return None,
            Aspect::Original => f64::from(extent[0]) / f64::from(extent[1]),
            Aspect::Ratio([w, h]) => w / h,
        };
        Some(if self.flipped { 1. / ratio } else { ratio })
    }

    /// Round once in the destination grid and retain at least one pixel per axis.
    pub fn pixels(self, extent: [u32; 2]) -> [u32; 4] {
        if extent.contains(&0) {
            return [0; 4];
        }
        let start: [u32; 2] = std::array::from_fn(|i| {
            (self.bounds[i] * f64::from(extent[i]))
                .round()
                .clamp(0., f64::from(extent[i] - 1)) as u32
        });
        let end: [u32; 2] = std::array::from_fn(|i| {
            (self.bounds[i + 2] * f64::from(extent[i]))
                .round()
                .clamp(f64::from(start[i] + 1), f64::from(extent[i])) as u32
        });
        [start[0], start[1], end[0], end[1]]
    }

    pub(crate) fn apply(self, mut view: View, output: Option<[u32; 2]>) -> View {
        if !self.is_full() {
            let [l, t, r, b] = view.crop;
            let [w, h] = [r - l, b - t];
            let extent = if matches!(view.rotation, 90 | 270) {
                [h, w]
            } else {
                [w, h]
            };
            let [x0, y0, x1, y1] = self.pixels(extent);
            let local = match view.rotation {
                90 => [y0, h - x1, y1, h - x0],
                180 => [w - x1, h - y1, w - x0, h - y0],
                270 => [w - y1, x0, w - y0, x1],
                _ => [x0, y0, x1, y1],
            };
            view.crop = [l + local[0], t + local[1], l + local[2], t + local[3]];
            let [u0, v0, u1, v1] = self.pixels(view.extent);
            view.extent = [u1 - u0, v1 - v0];
        }
        if let Some(size) = output {
            view.extent = size;
        }
        view
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oriented_crop_composes_with_offset_camera_area_and_every_rotation() {
        let crop = Crop {
            bounds: [0.1, 0.2, 0.6, 0.7],
            ..Default::default()
        };
        for (rotation, physical, extent) in [
            (0, [20, 60, 70, 160], [50, 100]),
            (90, [30, 100, 80, 200], [100, 50]),
            (180, [50, 80, 100, 180], [50, 100]),
            (270, [40, 40, 90, 140], [100, 50]),
        ] {
            let base = View {
                crop: [10, 20, 110, 220],
                rotation,
                extent: if matches!(rotation, 90 | 270) {
                    [200, 100]
                } else {
                    [100, 200]
                },
            };
            assert_eq!(Crop::default().apply(base, None), base);
            let result = crop.apply(base, None);
            assert_eq!(
                result,
                View {
                    crop: physical,
                    rotation,
                    extent
                }
            );
            let resized = crop.apply(base, Some([600, 400]));
            assert_eq!(resized.crop, physical);
            assert_eq!(resized.extent, [600, 400]);
        }
    }

    #[test]
    fn rectangular_sensor_pixels_keep_photographic_crop_dimensions() {
        let crop = Crop {
            bounds: [0.25, 0.25, 0.75, 0.75],
            ..Default::default()
        };
        let view = View {
            crop: [64, 18, 4768, 1586],
            rotation: 90,
            extent: [2176, 3264],
        };
        let result = crop.apply(view, None);
        assert_eq!(result.crop, [1240, 410, 3592, 1194]);
        assert_eq!(result.extent, [1088, 1632]);
        assert_eq!(crop.apply(view, Some([600, 900])).extent, [600, 900]);
    }

    #[test]
    fn small_crops_remain_nonempty_and_invalid_recipes_are_rejected() {
        let crop = Crop {
            bounds: [0.999, 0.999, 1., 1.],
            ..Default::default()
        };
        assert_eq!(crop.pixels([100, 80]), [99, 79, 100, 80]);
        for bounds in [[0., 0., 0., 1.], [-0.1, 0., 1., 1.], [0., 0., f64::NAN, 1.]] {
            assert!(Crop {
                bounds,
                ..Default::default()
            }
            .validate()
            .is_err());
        }
        for ratio in [[0., 1.], [1., f64::INFINITY], [-1., 1.]] {
            assert!(Crop {
                aspect: Aspect::Ratio(ratio),
                ..Default::default()
            }
            .validate()
            .is_err());
        }
        let crop = Crop {
            aspect: Aspect::Ratio([2., 3.]),
            flipped: true,
            ..Default::default()
        };
        assert_eq!(crop.ratio([100, 200]), Some(1.5));
        assert_eq!(
            serde_json::from_slice::<Crop>(&serde_json::to_vec(&crop).unwrap()).unwrap(),
            crop
        );
    }
}
