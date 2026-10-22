//! Keeps the selected DNG appearance separate from its editable camera samples.
use crate::source::Result;

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
    pub(crate) fn white_normalized(self) -> Result<Self> {
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

/// Choose where the reader restores the common linear storage scale.
/// Baseline exposure can exceed the reader's working-RGB range before its look.
/// Profile-curve compensation keeps that headroom available to the fitted tone.
#[derive(Clone, Copy)]
enum RangeCompensation {
    BaselineExposure,
    ProfileCurve,
}

/// An inverse is ambiguous where photographic tone reaches a white plateau.
/// Keeping its first crossing avoids a jump in the residual look at saturation.
#[derive(Clone, Copy)]
enum TonePlateau {
    WhiteEndpoint,
    FirstCrossing,
}

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
    /// Mode selection has already collapsed the sensor mixture in the samples.
    pub baked_color_mode: bool,
    pub sample_notes: Option<&'static str>,
    exposure: f64,
    response: Option<Response>,
    range_compensation: RangeCompensation,
}
/// Numerical linear coordinates and a channel tone curve for matrix-profile fitting.
pub(crate) struct MatrixAppearance {
    pub mapping: Matrix,
    pub neutral: [f64; 3],
    pub camera_to_linear: Matrix,
    pub linear_to_pcs: Matrix,
    pub after_response: Matrix,
    pub tone: Vec<f32>,
    pub calibration_exposure: f64,
    pub exposure: f64,
}
/// Calibrated sample coordinates and a signed-code photographic response.
/// The linear basis is a profile approximation, not a claim about sensor spectra.
pub(crate) struct SignedAppearance {
    pub mapping: [f64; 9],
    pub neutral: [f64; 3],
    pub camera_to_linear: [f64; 9],
    pub selected_from_base: [f64; 9],
    pub response: crate::nodes::finish::signed_response::Prepared,
}
/// Numerical sample coordinates and the selected linear mapping, independent of
/// camera family. Nonlinear content corrections can remain baked in the base.
pub(crate) struct ProfileBasis {
    pub mapping: [f64; 9],
    pub neutral: [f64; 3],
    pub camera_to_linear: [f64; 9],
    pub selected_from_base: [f64; 9],
    pub linear_to_pcs: [f64; 9],
}

/// An integral-code photographic response and the transform applied after it.
pub(crate) struct SampledAppearance {
    pub basis: ProfileBasis,
    pub response: crate::nodes::finish::code_response::Prepared,
    pub code_white: f64,
    pub after_response: [f64; 9],
    pub notes: Option<&'static str>,
}

enum Response {
    Signed(crate::nodes::finish::signed_response::Prepared),
    Unsigned {
        table: crate::nodes::finish::code_response::Prepared,
        code_white: f64,
    },
}

/// Linear sRGB primaries adapted to the D50 profile connection white.
const SRGB_D50: Matrix = Matrix([
    0.4360747, 0.3850649, 0.1430804, 0.2225045, 0.7168786, 0.0606169, 0.0139322, 0.0971045,
    0.7141733,
]);
pub(crate) fn srgb_to_pcs() -> Result<Matrix> {
    SRGB_D50.white_normalized()
}
pub(crate) fn srgb_to_romm() -> Result<Matrix> {
    Ok(SRGB_D50
        .white_normalized()?
        .then(ROMM.white_normalized()?.inverse()?))
}

pub(crate) struct ProfileData {
    pub curve: Vec<f32>,
    pub look: Vec<f32>,
    pub baseline: f64,
}

impl Prepared {
    #[cfg(test)]
    pub fn new(m: &crate::source::Metadata, settings: &crate::render::Settings) -> Result<Self> {
        crate::render::dng_appearance(m, settings, Default::default())
    }

    pub(crate) fn matrix(input: MatrixAppearance) -> Result<Self> {
        let romm = ROMM.white_normalized()?;
        let mapping = input.mapping;
        let neutral = input.neutral;
        let pcs = input.linear_to_pcs;
        let camera_to_native = input.camera_to_linear;
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
        // Saturation follows channel tone and precedes conversion to PCS.
        let after_tone = input.after_response.then(pcs).then(romm.inverse()?);
        Ok(Self {
            mapping,
            neutral,
            forward,
            color,
            camera_to_romm,
            camera_to_native,
            after_tone,
            tone: input.tone,
            baked_color_mode: false,
            sample_notes: None,
            calibration_exposure: input.calibration_exposure,
            exposure: input.exposure,
            response: None,
            range_compensation: RangeCompensation::BaselineExposure,
        })
    }
    pub(crate) fn signed(input: SignedAppearance) -> Result<Self> {
        Self::linear(
            ProfileBasis {
                mapping: input.mapping,
                neutral: input.neutral,
                camera_to_linear: input.camera_to_linear,
                selected_from_base: input.selected_from_base,
                linear_to_pcs: ROMM.white_normalized()?.0,
            },
            Response::Signed(input.response),
            Matrix::diagonal([1.; 3]),
        )
    }
    pub(crate) fn sampled(input: SampledAppearance) -> Result<Self> {
        if !input.code_white.is_finite()
            || input.code_white <= 0.
            || input.after_response.iter().any(|value| !value.is_finite())
        {
            return Err("invalid sampled DNG appearance".into());
        }
        let mut prepared = Self::linear(
            input.basis,
            Response::Unsigned {
                table: input.response,
                code_white: input.code_white,
            },
            Matrix(input.after_response),
        )?;
        prepared.sample_notes = input.notes;
        Ok(prepared)
    }
    fn linear(input: ProfileBasis, response: Response, after_response: Matrix) -> Result<Self> {
        let neutral = input.neutral.map(|v| (v * 1_000_000.).round() / 1_000_000.);
        if neutral.iter().any(|v| !v.is_finite() || *v <= 0.) {
            return Err("invalid DNG profile neutral".into());
        }
        let mapping = Matrix(input.mapping);
        let base = Matrix(input.camera_to_linear);
        let selected = Matrix(input.selected_from_base);
        // Publish the recipe's linear basis in the D50 connection space. The
        // selected nonlinear photographic response remains in the profile look.
        let romm = ROMM.white_normalized()?;
        let forward = Matrix::diagonal(neutral)
            .then(base)
            .then(Matrix(input.linear_to_pcs))
            .white_normalized()?;
        let forward = Matrix(forward.0.map(|v| (v * 1_000_000.).round() / 1_000_000.));
        let reader = forward.white_normalized()?;
        let color = reader.inverse()?.then(Matrix::diagonal(neutral));
        let camera_to_romm = Matrix::diagonal(neutral.map(|v| 1. / v))
            .then(reader)
            .then(romm.inverse()?);
        // Storage may collapse a monochrome signal to neutral RGB. Only the
        // reader's interpretation must remain invertible for profile fitting.
        if mapping.0.iter().any(|v| !v.is_finite())
            || mapping
                .0
                .as_chunks::<3>()
                .0
                .iter()
                .any(|row| row.iter().all(|v| *v == 0.))
        {
            return Err("invalid DNG storage transform".into());
        }
        selected.inverse()?;
        Ok(Self {
            mapping,
            neutral,
            forward,
            color,
            camera_to_romm,
            camera_to_native: base.then(selected),
            after_tone: after_response,
            tone: Vec::new(),
            baked_color_mode: false,
            sample_notes: None,
            calibration_exposure: 0.,
            exposure: 0.,
            response: Some(response),
            // Restore the storage range inside photographic tone, rather than
            // clipping bright linear colors before the reader applies its look.
            range_compensation: RangeCompensation::ProfileCurve,
        })
    }
    pub(crate) fn monochrome(tone: Vec<f32>, exposure: f64, code_white: f64) -> Result<Self> {
        let identity = Matrix::diagonal([1.; 3]);
        // Equal linear samples are already WB-normalized and include technical ISO gain.
        // Define neutral RGB in a standard D50 basis without inverting the source color matrix.
        let romm = ROMM.white_normalized()?;
        let forward = Matrix(romm.0.map(|v| (v * 1_000_000.).round() / 1_000_000.));
        let reader = forward.white_normalized()?;
        Ok(Self {
            mapping: identity.gain(1. / code_white),
            neutral: [1.; 3],
            forward,
            color: reader.inverse()?,
            camera_to_romm: reader.then(romm.inverse()?),
            camera_to_native: identity,
            after_tone: identity,
            tone,
            baked_color_mode: false,
            sample_notes: None,
            calibration_exposure: 0.,
            exposure,
            response: None,
            range_compensation: RangeCompensation::BaselineExposure,
        })
    }
    pub fn finish(&self, scale: f32) -> Result<ProfileData> {
        // Fit using the exposure the reader will receive. Extra precision here
        // would fit a slightly different rendering than the exported metadata.
        let baseline = match self.range_compensation {
            RangeCompensation::BaselineExposure => {
                ((f64::from(scale).log2() + self.calibration_exposure) * 1_000_000.).round()
                    / 1_000_000.
            }
            RangeCompensation::ProfileCurve => 0.,
        };
        let ramp = baseline.max(0.).exp2();
        let before = self
            .camera_to_romm
            .inverse()?
            .then(self.camera_to_native)
            .gain(f64::from(scale) / ramp * self.exposure.exp2());
        let target = |rgb: [f64; 3]| {
            let native = before.apply(rgb);
            let result = if let Some(response) = &self.response {
                match response {
                    Response::Signed(response) => response.evaluate_linear(native),
                    Response::Unsigned { table, code_white } => self
                        .after_tone
                        .apply(table.evaluate_codes(native.map(|value| value * code_white))),
                }
            } else {
                self.after_tone
                    .apply(native.map(|v| native_tone(&self.tone, v)))
            };
            result.map(|v| v.clamp(0., 1.))
        };
        let plateau = match self.range_compensation {
            RangeCompensation::BaselineExposure => TonePlateau::WhiteEndpoint,
            RangeCompensation::ProfileCurve => TonePlateau::FirstCrossing,
        };
        fit_profile(baseline, plateau, target)
    }
}

// Both pipelines describe their desired ROMM appearance at this boundary.
// The common fitter packages it as the reader's curve and residual color look.
fn fit_profile(
    baseline: f64,
    plateau: TonePlateau,
    target: impl Fn([f64; 3]) -> [f64; 3],
) -> Result<ProfileData> {
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
                let pre = inverse_rgb_tone(target(rgb), &effective, plateau);
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
fn inverse_rgb_tone(rgb: [f64; 3], tone: &[f64], plateau: TonePlateau) -> [f64; 3] {
    let inverse = |y: f64| {
        if y <= 0. {
            return 0.;
        }
        if y > tone[4095] || (y == tone[4095] && matches!(plateau, TonePlateau::WhiteEndpoint)) {
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
#[path = "appearance_test.rs"]
mod tests;
