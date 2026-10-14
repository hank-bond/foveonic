//! Keeps the selected DNG appearance separate from its editable camera samples.
use crate::{
    calibration::{self, ColorSpace},
    color_mode,
    render::Settings,
    source::{Metadata, Result},
    tone,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Matrix(pub [f64; 9]);
impl Matrix {
    pub fn apply(self, x: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|r| (0..3).map(|c| self.0[r * 3 + c] * x[c]).sum())
    }
    /// Apply this matrix first, then `next`. With column vectors, reversing
    /// that order changes the result, so the product is `next * self`.
    pub fn then(self, next: Self) -> Self {
        Self(std::array::from_fn(|i| {
            (0..3)
                .map(|k| next.0[i / 3 * 3 + k] * self.0[k * 3 + i % 3])
                .sum()
        }))
    }
    pub fn gain(self, x: f64) -> Self {
        Self(self.0.map(|v| v * x))
    }
    pub fn diagonal(x: [f64; 3]) -> Self {
        Self(std::array::from_fn(|i| {
            if i / 3 == i % 3 {
                x[i / 3]
            } else {
                0.
            }
        }))
    }
    pub fn inverse(self) -> Result<Self> {
        let mut a = [[0.; 6]; 3];
        for (r, row) in a.iter_mut().enumerate() {
            row[..3].copy_from_slice(&self.0[r * 3..r * 3 + 3]);
            row[r + 3] = 1.;
        }
        for i in 0..3 {
            let pivot = (i..3)
                .max_by(|&r, &s| a[r][i].abs().total_cmp(&a[s][i].abs()))
                .unwrap();
            a.swap(i, pivot);
            let divisor = a[i][i];
            if !divisor.is_finite() || divisor.abs() < 1e-12 {
                return Err("singular DNG color matrix".into());
            }
            for x in &mut a[i] {
                *x /= divisor;
            }
            for r in 0..3 {
                if r != i {
                    let v = a[r][i];
                    let pivot_row = a[i];
                    for (value, pivot) in a[r].iter_mut().zip(pivot_row) {
                        *value -= v * pivot;
                    }
                }
            }
        }
        let result = Self(std::array::from_fn(|i| a[i / 3][i % 3 + 3]));
        if result.0.iter().any(|v| !v.is_finite()) {
            return Err("invalid DNG inverse matrix".into());
        }
        Ok(result)
    }
    fn white_normalized(self) -> Result<Self> {
        let white = self.apply([1.; 3]);
        if white.iter().any(|x| !x.is_finite() || *x <= 0.) {
            return Err("invalid DNG profile white".into());
        }
        Ok(Self(std::array::from_fn(|i| {
            self.0[i] * WHITE[i / 3] / white[i / 3]
        })))
    }
}
const WHITE: [f64; 3] = [0.3457 / 0.3585, 1., (1. - 0.3457 - 0.3585) / 0.3585];
const ROMM: Matrix = Matrix([
    0.7976749, 0.1351917, 0.0313534, 0.2880402, 0.7118741, 0.0000857, 0., 0., 0.82521,
]);
// More hue, saturation, and brightness samples reduce interpolation error.
// These grid sizes do not make the color correction stronger.
pub(crate) const DIMS: [u32; 3] = [72, 33, 65];
const INTERVALS: usize = 512;

pub(crate) struct Prepared {
    pub mapping: Matrix,
    pub neutral: [f64; 3],
    pub forward: Matrix,
    pub color: Matrix,
    camera_to_romm: Matrix,
    camera_to_native: Matrix,
    after_tone: Matrix,
    tone: Vec<f32>,
    pub calibration_exposure: f64,
    exposure: f64,
}
pub(crate) struct ProfileData {
    pub curve: Vec<f32>,
    pub look: Vec<f32>,
    pub baseline: f64,
}

impl Prepared {
    pub fn new(m: &Metadata, settings: &Settings) -> Result<Self> {
        let key = m
            .lists
            .get("WhiteBalanceGains")
            .and_then(|x| x.get(&settings.white_balance))
            .ok_or("missing DNG WB gains")?;
        let gains = m.fixed::<3>(key)?;
        if gains.iter().any(|v| !v.is_finite() || *v <= 0.) {
            return Err("invalid DNG WB gains".into());
        }
        let n = gains.map(|v| 1. / v);
        let maximum = n.into_iter().fold(0., f64::max);
        // Readers see rounded rationals, not our full-precision WB values.
        // Use those same values here so the fitted look matches their input.
        let neutral = n.map(|v| (v / maximum * 1_000_000.).round() / 1_000_000.);
        if neutral.iter().any(|v| *v <= 0.) {
            return Err("unrepresentable DNG neutral".into());
        }
        let unselected = color_mode::Settings {
            tone_index: 0,
            contrast: 0.,
            compensation: None,
        };
        let rows = calibration::color(m, &settings.white_balance, &unselected)?.matrix;
        let base = Matrix(std::array::from_fn(|i| f64::from(rows[i / 3][i % 3])));
        let mode = color_mode::prepare(m, &settings.color_mode)?;
        let selected = Matrix(mode.camera_matrix(base.0));
        let mapping = base
            .inverse()?
            .then(Matrix::diagonal(gains.map(|v| 1. / v / 4095.)));
        let space = ColorSpace::from_metadata(m)?;
        let pcs = match space {
            ColorSpace::Srgb => Matrix([
                0.4360747, 0.3850649, 0.1430804, 0.2225045, 0.7168786, 0.0606169, 0.0139322,
                0.0971045, 0.7141733,
            ]),
            ColorSpace::AdobeRgb => Matrix([
                0.6097559, 0.2052401, 0.1492240, 0.3111242, 0.6256560, 0.0632197, 0.0194811,
                0.0608902, 0.7448387,
            ]),
        }
        .white_normalized()?;
        let romm = ROMM.white_normalized()?;
        let camera_to_native = Matrix::diagonal(gains).then(selected);
        let forward = Matrix::diagonal(neutral)
            .then(camera_to_native)
            .then(pcs)
            .white_normalized()?;
        let forward = Matrix(forward.0.map(|v| (v * 1_000_000.).round() / 1_000_000.));
        // The SDK normalizes ForwardMatrix to its reference white again.
        // Fit against that normalized matrix, not just the values we publish.
        let reader_forward = forward.white_normalized()?;
        let color = reader_forward.inverse()?.then(Matrix::diagonal(neutral));
        let camera_to_romm = Matrix::diagonal(neutral.map(|v| 1. / v))
            .then(reader_forward)
            .then(romm.inverse()?);
        let sat = f64::from(settings.saturation);
        let saturation = Matrix(std::array::from_fn(|i| {
            [0.299, 0.587, 0.114][i % 3] * (1. - sat) + if i / 3 == i % 3 { sat } else { 0. }
        }));
        // Moving saturation before channel tone changes the look. Combine it
        // only with the color-space conversion that follows it.
        let after_tone = saturation.then(pcs).then(romm.inverse()?);
        let (_, iso) = crate::noise::processing_gain(m)?;
        Ok(Self {
            mapping,
            neutral,
            forward,
            color,
            camera_to_romm,
            camera_to_native,
            after_tone,
            tone: tone::prepare(m, &mode, settings.tone)?,
            calibration_exposure: (f64::from(iso) / maximum).log2(),
            exposure: f64::from(settings.exposure),
        })
    }
    pub fn finish(&self, scale: f32) -> Result<ProfileData> {
        // Fit using the exposure the reader will receive. Extra precision here
        // would fit a slightly different rendering than the exported metadata.
        let baseline = ((f64::from(scale).log2() + self.calibration_exposure) * 1_000_000.).round()
            / 1_000_000.;
        let ramp = baseline.max(0.).exp2();
        let before = self
            .camera_to_romm
            .inverse()?
            .then(self.camera_to_native)
            .gain(f64::from(scale) / ramp * self.exposure.exp2());
        let target = |rgb: [f64; 3]| {
            self.after_tone
                .apply(before.apply(rgb).map(|v| native_tone(&self.tone, v)))
                .map(|v| v.clamp(0., 1.))
        };
        let mut curve = Vec::with_capacity((INTERVALS + 1) * 2);
        for i in 0..=INTERVALS {
            let x = i as f64 / INTERVALS as f64;
            let y = if i == INTERVALS {
                1.
            } else {
                target([inverse_exposure(x, baseline); 3])
                    .into_iter()
                    .sum::<f64>()
                    / 3.
            };
            curve.extend([x as f32, y as f32]);
        }
        if curve.windows(4).step_by(2).any(|v| v[3] < v[1]) {
            return Err("non-monotonic DNG presentation curve".into());
        }
        let effective: Vec<f64> = (0..=4095)
            .map(|i| curve_at(&curve, exposure_tone(i as f64 / 4095., baseline)))
            .collect();
        // Leave gray brightness to the common curve. Adjusting it in the color
        // table as well would apply the tone change twice.
        let [nh, ns, nv] = DIMS;
        let mut look = Vec::with_capacity((nh * ns * nv * 3) as usize);
        for v in 0..nv {
            for h in 0..nh {
                for s in 0..ns {
                    if v == 0 || s == 0 {
                        look.extend([0., 1., 1.]);
                        continue;
                    }
                    let value = v as f64 / (nv - 1) as f64;
                    let hue = h as f64 / nh as f64;
                    let sat = s as f64 / (ns - 1) as f64;
                    let rgb = hsv_to_rgb(hue, sat, decode(value));
                    let pre = inverse_rgb_tone(target(rgb), &effective);
                    let [th, ts, tv] = rgb_to_hsv(pre);
                    let shift = if ts < 1e-7 {
                        0.
                    } else {
                        ((th - hue + 0.5).rem_euclid(1.) - 0.5) * 360.
                    };
                    look.extend([shift as f32, (ts / sat) as f32, (encode(tv) / value) as f32]);
                }
            }
        }
        if look.iter().any(|v| !v.is_finite()) {
            return Err("non-finite DNG look data".into());
        }
        Ok(ProfileData {
            curve,
            look,
            baseline,
        })
    }
}
fn native_tone(table: &[f32], value: f64) -> f64 {
    f64::from(table[(value * 4095. + 0.5).floor().clamp(0., 65535.) as usize])
}
fn curve_at(curve: &[f32], x: f64) -> f64 {
    let p = x.clamp(0., 1.) * INTERVALS as f64;
    let lo = (p.floor() as usize).min(INTERVALS - 1);
    f64::from(curve[lo * 2 + 1])
        + (f64::from(curve[lo * 2 + 3]) - f64::from(curve[lo * 2 + 1])) * (p - lo as f64)
}
// Adobe's negative exposure darkens shadows but keeps white fixed. Include
// that extra curve when fitting the profile so the reader does not change tone twice.
fn exposure_tone(x: f64, exposure: f64) -> f64 {
    if exposure >= 0. {
        return x;
    }
    let slope = exposure.exp2();
    if x <= 0.25 {
        return x * slope;
    }
    let a = 16. / 9. * (1. - slope);
    let b = slope - 0.5 * a;
    let c = 1. - a - b;
    (a * x + b) * x + c
}
fn inverse_exposure(y: f64, exposure: f64) -> f64 {
    if exposure >= 0. || y <= 0. || y >= 1. {
        return y;
    }
    let (mut a, mut b) = (0., 1.);
    for _ in 0..48 {
        let x = (a + b) * 0.5;
        if exposure_tone(x, exposure) < y {
            a = x;
        } else {
            b = x;
        }
    }
    (a + b) * 0.5
}
// Adobe's tone step keeps the middle channel between the lowest and highest.
// Undo that relationship too, rather than inverting each channel independently.
fn inverse_rgb_tone(rgb: [f64; 3], tone: &[f64]) -> [f64; 3] {
    let inverse = |y: f64| {
        if y <= 0. {
            return 0.;
        }
        if y >= tone[4095] {
            return 1.;
        }
        let r = tone.partition_point(|v| *v < y).clamp(1, 4095);
        (r as f64 - 1. + (y - tone[r - 1]) / (tone[r] - tone[r - 1])) / 4095.
    };
    let low = rgb.into_iter().fold(f64::INFINITY, f64::min);
    let high = rgb.into_iter().fold(0., f64::max);
    let a = inverse(low);
    let b = inverse(high);
    if high - low < 1e-12 {
        [a; 3]
    } else {
        rgb.map(|v| a + (b - a) * (v - low) / (high - low))
    }
}
fn decode(x: f64) -> f64 {
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}
fn encode(x: f64) -> f64 {
    if x <= 0.0031308 {
        x * 12.92
    } else {
        1.055 * x.powf(1. / 2.4) - 0.055
    }
}
fn hsv_to_rgb(h: f64, s: f64, v: f64) -> [f64; 3] {
    [5., 3., 1.].map(|o| {
        let k = (h * 6. + o) % 6.;
        v * (1. - s * k.min(4. - k).clamp(0., 1.))
    })
}
fn rgb_to_hsv(rgb: [f64; 3]) -> [f64; 3] {
    let hi = rgb.into_iter().fold(0., f64::max);
    let lo = rgb.into_iter().fold(f64::INFINITY, f64::min);
    let d = hi - lo;
    // Nearly gray values have no reliable hue. Dividing by their tiny channel
    // difference would turn rounding noise into a large color-table adjustment.
    if d < 1e-12 || hi <= 0. {
        return [0., 0., hi];
    }
    let h = if rgb[0] == hi {
        (rgb[1] - rgb[2]) / d
    } else if rgb[1] == hi {
        (rgb[2] - rgb[0]) / d + 2.
    } else {
        (rgb[0] - rgb[1]) / d + 4.
    };
    [(h / 6.).rem_euclid(1.), d / hi, hi]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adjacent_matrices_compose_in_order_without_crossing_tone() {
        let m = Matrix([1., 0.2, -0.1, 0., 1., 0.3, 0.1, 0., 1.]);
        let g = Matrix::diagonal([1.2, 0.8, 1.1]);
        let x = [0.1, 0.4, 0.6];
        for (a, b) in m.then(g).apply(x).into_iter().zip(g.apply(m.apply(x))) {
            assert!((a - b).abs() < 1e-12);
        }
        assert_ne!(m.then(g).apply(x), g.then(m).apply(x));
        let squared = m.apply(x).map(|v| v * v);
        assert_ne!(g.apply(squared), g.apply(m.apply(x)).map(|v| v * v));
        let restored = m.then(m.inverse().unwrap()).apply(x);
        for i in 0..3 {
            assert!((restored[i] - x[i]).abs() < 1e-12);
        }
        assert!(Matrix([0.; 9]).inverse().is_err());
    }
    #[test]
    fn negative_exposure_inverse_and_hsv_coordinates_round_trip() {
        for e in [-3., -0.5, 0., 1.] {
            for x in [0., 0.01, 0.18, 0.7, 1.] {
                assert!((inverse_exposure(exposure_tone(x, e), e) - x).abs() < 1e-12);
            }
        }
        for h in 0..72 {
            for s in [0.01, 0.5, 1.] {
                let hsv = [h as f64 / 72., s, 0.3];
                let r = rgb_to_hsv(hsv_to_rgb(hsv[0], hsv[1], hsv[2]));
                for i in 0..3 {
                    assert!((r[i] - hsv[i]).abs() < 1e-12);
                }
            }
        }
    }
}
