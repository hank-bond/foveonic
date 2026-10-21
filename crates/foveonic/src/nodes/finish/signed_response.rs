//! Restores linear detail or applies photographic response to signed root-domain samples.
//!
//! Equations are independently recovered in the reference implementation research.
//! The operation contract uses signed 16-bit code values, not normalized linear RGB.

// Signed codes address tables through their low 16 bits, including negatives.
const LUT_LEN: usize = 65536;
const LUT_MASK: i32 = 65535;
// Root, linear and tone code domains use 4096 codes per unit of signal (Q12).
// This is distinct from the final encoded 16-bit range, whose white is 65535.
const SIGNAL_CODE_UNIT: f64 = 4096.;
const ENCODED_WHITE: f64 = 65535.;
const SATURATION_Q12_UNIT: f64 = 4096.;
const DETAIL_Q14_UNIT: f64 = 16384.;
// Keep the recovered literal, rather than rounding the cubic/root offset.
const ROOT_OFFSET: f64 = 0.008000000000000002;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Controls {
    pub exposure: f64,
    pub shadows: f64,
    pub contrast: f64,
    pub highlights: f64,
    pub saturation: f64,
    pub detail: f64,
}

/// Functional tone adjustments selected by the recipe, independent of camera identity.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ToneAdjustments {
    pub exposure: f64,
    pub shadows: f64,
    pub contrast: f64,
    pub highlights: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Prepared {
    /// Exponential, root, three tone channels, linear response tables.
    /// The response stores f32 bits. Other tables contain integer code values.
    tables: Vec<i32>,
    saturation: [i32; 9],
    detail_q14: i32,
}

impl Prepared {
    pub(crate) fn key(&self) -> crate::recipe::Key {
        crate::recipe::Key::default()
            .slice(&self.tables)
            .slice(&self.saturation)
            .with(&self.detail_q14)
    }
}

/// Root-domain detail strength without photographic tone or encoding.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LinearDetail {
    detail_q14: i32,
}

impl LinearDetail {
    pub(crate) fn key(&self) -> crate::recipe::Key {
        crate::recipe::Key::default().with(&self.detail_q14)
    }
}

pub(crate) fn prepare_linear_detail(detail: f64) -> Result<LinearDetail, String> {
    let strength = (detail + 1.) * DETAIL_Q14_UNIT;
    if !strength.is_finite() || strength < i32::MIN as f64 || strength > i32::MAX as f64 {
        return Err("invalid signed linear detail strength".into());
    }
    Ok(LinearDetail {
        detail_q14: strength as i32,
    })
}

impl Prepared {
    /// Samples the pointwise photographic response for profile fitting.
    /// Input is normalized linear signal after spatial detail. Root conversion
    /// precedes tone here, while detail is already baked into the DNG samples.
    pub(crate) fn evaluate_linear(&self, linear: [f64; 3]) -> [f64; 3] {
        let index = |value: i32| (value & LUT_MASK) as usize;
        let rgb: [i32; 3] = std::array::from_fn(|c| {
            let code = (linear[c] * SIGNAL_CODE_UNIT).round().clamp(-4096., 32767.) as i32;
            let root = self.tables[LUT_LEN + index(code)];
            self.tables[(2 + c) * LUT_LEN + index(root)]
        });
        std::array::from_fn(|c| {
            let sum = self.saturation[c * 3..c * 3 + 3]
                .iter()
                .zip(rgb)
                .fold(2048_i32, |sum, (m, v)| sum.wrapping_add(m.wrapping_mul(v)));
            f64::from(f32::from_bits(
                self.tables[5 * LUT_LEN + index(sum >> 12)] as u32,
            ))
        })
    }
}

pub(crate) fn prepare(
    c: Controls,
    gains: [f64; 3],
    a: ToneAdjustments,
    response_transfer: crate::color_space::TransferFunction,
) -> Result<Prepared, String> {
    if [
        c.exposure,
        c.shadows,
        c.contrast,
        c.highlights,
        c.saturation,
        c.detail,
        a.exposure,
        a.shadows,
        a.contrast,
        a.highlights,
    ]
    .iter()
    .any(|x| !x.is_finite())
        || gains.iter().any(|x| !x.is_finite() || *x <= 0.)
        || matches!(response_transfer, crate::color_space::TransferFunction::Power(gamma)
            if !gamma.is_finite() || gamma <= 0.)
    {
        return Err("invalid signed photographic response parameters".into());
    }
    let exp = exponential();
    let mut tables = exp.clone();
    tables.extend(root());
    tables.extend(tone(c, gains, a, &exp));
    // Preserve the photographic toe and shoulder in this response. Only undo
    // the selected profile encoding, leaving output conversion to the consumer.
    tables.extend(gamma().into_iter().map(|code| {
        (response_transfer.decode(f64::from(code) / ENCODED_WHITE) as f32).to_bits() as i32
    }));
    let off = -c.saturation / 6.;
    let saturation = std::array::from_fn(|i| {
        let value = if i / 3 == i % 3 { 1. - 2. * off } else { off };
        (value * SATURATION_Q12_UNIT + 0.5) as i32
    });
    Ok(Prepared {
        tables,
        saturation,
        detail_q14: ((c.detail + 1.) * DETAIL_Q14_UNIT) as i32,
    })
}

// Inverse root mapping: cubic above the join and the reference linear extension
// below it. The join codes and signed clamps are recovered rendering boundaries.
fn exponential() -> Vec<i32> {
    static TABLE: std::sync::OnceLock<Vec<i32>> = std::sync::OnceLock::new();
    TABLE.get_or_init(build_exponential).clone()
}

fn build_exponential() -> Vec<i32> {
    let mut out = vec![0; LUT_LEN];
    for (i, value) in out.iter_mut().enumerate().take(816) {
        *value = ((i as f64 / SIGNAL_CODE_UNIT - 0.2) * 0.12000000000000002 * SIGNAL_CODE_UNIT)
            .round() as i32;
    }
    for (i, value) in out.iter_mut().enumerate().take(65531).skip(819) {
        let x = i as f64 / SIGNAL_CODE_UNIT;
        *value = (x.mul_add(x * x, -ROOT_OFFSET) * SIGNAL_CODE_UNIT)
            .round()
            .min(32767.) as i32;
    }
    out[65531..].fill(32767);
    for i in -4096_i32..0 {
        out[(i & LUT_MASK) as usize] =
            ((f64::from(i) / SIGNAL_CODE_UNIT - 0.2) * 0.12000000000000002 * SIGNAL_CODE_UNIT)
                .round()
                .max(-288.) as i32;
    }
    out
}

// The negative-code extension is a recovered quadratic continuation. Its 819.2
// constants correspond to 0.2 in Q12. Preserve these literals and evaluation order.
pub(crate) fn root() -> Vec<i32> {
    static TABLE: std::sync::OnceLock<Vec<i32>> = std::sync::OnceLock::new();
    TABLE.get_or_init(build_root).clone()
}

fn build_root() -> Vec<i32> {
    let mut out: Vec<i32> = (0..LUT_LEN)
        .map(|i| {
            ((i as f64 / SIGNAL_CODE_UNIT + ROOT_OFFSET).powf(1. / 3.) * SIGNAL_CODE_UNIT + 0.5)
                as i32
        })
        .collect();
    for i in -4096_i32..0 {
        let x = ((f64::from(i) / SIGNAL_CODE_UNIT).mul_add(8.333333333333332, 0.2)
            * SIGNAL_CODE_UNIT
            + 819.2)
            .max(0.)
            * 0.5;
        out[(i & LUT_MASK) as usize] = (x * x / 819.2 + 0.5) as i32;
    }
    out
}

fn tone(c: Controls, gains: [f64; 3], a: ToneAdjustments, exp: &[i32]) -> Vec<i32> {
    let exposure = c.exposure + a.exposure;
    let shadows = c.shadows + a.shadows;
    // These contrast, shoulder and toe coefficients define the reference
    // photographic appearance. They are rendering constants, not RGB primaries
    // or the destination transfer function. Keep their recovered precision.
    let power = (c.contrast + a.contrast + 2.) * 0.25 * 1.29125 + 0.64375;
    let control = (c.highlights + a.highlights + 2.) * 0.25;
    // Normalize gains by the average of weighted arithmetic and harmonic means.
    // The reference uses green/red/blue weights of 0.5/0.3/0.2.
    let mean = (gains[1] * 0.5
        + gains[0] * 0.3
        + gains[2] * 0.2
        + 1. / (0.5 / gains[1] + 0.3 / gains[0] + 0.2 / gains[2]))
        * 0.5;
    let middle = 0.18_f64.powf(1. - power);
    let shoulder_power = 1.07_f64.powf((control * 1.5 + 1.).powi(4));
    let shoulder_scale = control * 0.44 + 0.66;
    let shoulder = |value: f64| {
        let p = middle * value.powf(power);
        let limit = shoulder_scale * p.powf(0.22);
        p * limit
            * (1. / (p.powf(shoulder_power) + limit.powf(shoulder_power))).powf(1. / shoulder_power)
    };
    let fraction = 1. - power / 2.4;
    let (mut offset, mut join, mut slope) = (0., 0., 0.);
    if fraction > 0. {
        offset = fraction * 0.06;
        join = 0.18_f64.powf((power - 1.) / 2.4);
        join = (((offset + 0.05213316845261357) * join) / (fraction * 1.0521331684526136))
            .powf(1. / (power / 2.4));
        let y = shoulder(join);
        let encoded = if y <= 0.00304 {
            y * 12.923054468333259
        } else {
            (y.powf(1. / 2.4) - 0.05213316845261357) / 0.9478668315473864
        };
        slope = (encoded - offset) / join;
    }
    let mut out = vec![0; 3 * LUT_LEN];
    for channel in 0..3 {
        let row = &mut out[channel * LUT_LEN..(channel + 1) * LUT_LEN];
        let (mut previous_knot, mut previous_value) = (0, 0.);
        for knot in (0..=8160).step_by(32) {
            let signal =
                f64::from(exp[knot]) / SIGNAL_CODE_UNIT - ((2. - shadows) * 0.25 * 2. - 1.) * 0.025;
            let value = signal * (gains[channel] / mean) * exposure.exp2() - 0.005;
            let y = if value >= join {
                shoulder(value)
            } else {
                let encoded = (offset + value * slope).max(0.);
                let linear = encoded / 12.923054468333259;
                if linear > 0.00304 {
                    (encoded * 0.9478668315473864 + 0.05213316845261357).powf(2.4)
                } else {
                    linear
                }
            };
            for (index, sample) in row
                .iter_mut()
                .enumerate()
                .take(knot)
                .skip(previous_knot + 1)
            {
                let weight = (index - previous_knot) as f64 / (knot - previous_knot) as f64;
                *sample = ((y * weight + previous_value * (1. - weight)) * SIGNAL_CODE_UNIT + 0.5)
                    .min(16384.) as i32;
            }
            row[knot] = (y * SIGNAL_CODE_UNIT + 0.5).min(16384.) as i32;
            previous_knot = knot;
            previous_value = y;
        }
        let tail = row[8160];
        row[8161..32768].fill(tail);
    }
    out
}

// Reference photographic response: lifted toe, power segment and highlight
// shoulder. This is more than transfer encoding. prepare() removes only the
// recipe-selected profile encoding after constructing these quantized codes.
fn gamma() -> Vec<i32> {
    static TABLE: std::sync::OnceLock<Vec<i32>> = std::sync::OnceLock::new();
    TABLE.get_or_init(build_gamma).clone()
}

fn build_gamma() -> Vec<i32> {
    let transfer = |x: f64| {
        let mut value = if x <= 0.0018992897185657936 {
            0.013674885973673709 + x * 9.
        } else {
            x.powf(0.5555555555555556)
        };
        if value > 0.4 {
            value *= 1.5 * (1. / (value.powi(4) + 5.0625)).powf(0.25);
        }
        value
    };
    let mut out = vec![0; LUT_LEN];
    let (mut index, mut knot, mut step, mut previous, mut last) = (0, 0, 8, 0., 0);
    while last < 65535 {
        let value = transfer(knot as f64 / SIGNAL_CODE_UNIT);
        let origin = index as i32 - 1;
        let interval_start = index;
        for (position, sample) in out
            .iter_mut()
            .enumerate()
            .take(knot + 1)
            .skip(interval_start)
        {
            let fraction = (position as i32 - origin) as f64 / (knot as i32 - origin) as f64;
            last = ((previous * (1. - fraction) + fraction * value) * ENCODED_WHITE).round() as i32;
            if last >= 65535 {
                break;
            }
            *sample = last & LUT_MASK;
            index = position + 1;
        }
        knot += step;
        step += 1;
        previous = value;
    }
    out[index..32768].fill(65535);
    for position in (32769..65536).rev() {
        let value = transfer((position as i32 - 65536) as f64 / SIGNAL_CODE_UNIT);
        if value <= 0. {
            break;
        }
        out[position] = (value * ENCODED_WHITE).round() as i32 & LUT_MASK;
    }
    out
}

/// Applies detail and photographic response to integral signed root-domain RGB.
/// Source has three packed components per pixel. Detail has one signed component.
/// LUT addressing intentionally wraps to 16 bits. Output is normalized linear ROMM,
/// with destination gamut clipping and encoding left to the output consumer.
pub(crate) fn apply(
    gpu: &crate::gpu::Gpu,
    source: &crate::gpu::ReadGuard,
    detail: &crate::gpu::ReadGuard,
    dimensions: [u32; 2],
    prepared: &Prepared,
    profile: &mut crate::profile::Profile,
) -> crate::source::Result<crate::gpu::ReadGuard> {
    dispatch_response(
        gpu,
        source,
        detail,
        dimensions,
        &prepared.tables,
        prepared.saturation,
        prepared.detail_q14,
        false,
        profile,
    )
}

/// Restores linear integral codes after applying root-domain spatial detail.
pub(crate) fn apply_linear_detail(
    gpu: &crate::gpu::Gpu,
    source: &crate::gpu::ReadGuard,
    detail: &crate::gpu::ReadGuard,
    dimensions: [u32; 2],
    prepared: &LinearDetail,
    profile: &mut crate::profile::Profile,
) -> crate::source::Result<crate::gpu::ReadGuard> {
    dispatch_response(
        gpu,
        source,
        detail,
        dimensions,
        &exponential(),
        [0; 9],
        prepared.detail_q14,
        true,
        profile,
    )
}

#[allow(clippy::too_many_arguments)]
fn dispatch_response(
    gpu: &crate::gpu::Gpu,
    source: &crate::gpu::ReadGuard,
    detail: &crate::gpu::ReadGuard,
    dimensions: [u32; 2],
    tables: &[i32],
    saturation: [i32; 9],
    detail_q14: i32,
    linear: bool,
    profile: &mut crate::profile::Profile,
) -> crate::source::Result<crate::gpu::ReadGuard> {
    use crate::gpu::{
        Binding::{Read, Write},
        Kernel,
    };
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Params {
        size: [u32; 4],
        matrix: [[i32; 4]; 3],
        detail: [i32; 4],
    }
    let [w, h] = dimensions;
    let pixels = u64::from(w) * u64::from(h);
    if w == 0 || h == 0 || tables.len() != if linear { LUT_LEN } else { 6 * LUT_LEN } {
        return Err("invalid signed response geometry or tables".into());
    }
    if [w.div_ceil(8), h.div_ceil(8)]
        .iter()
        .any(|&groups| groups > gpu.device.limits().max_compute_workgroups_per_dimension)
    {
        return Err("signed response exceeds GPU dispatch limits".into());
    }
    if source.size() != pixels * 12 || detail.size() != pixels * 4 {
        return Err("signed response requires matching packed RGB and mono planes".into());
    }
    let params = Params {
        size: [w, h, u32::from(linear), 0],
        matrix: std::array::from_fn(|r| {
            std::array::from_fn(|c| if c < 3 { saturation[r * 3 + c] } else { 0 })
        }),
        detail: [detail_q14, 0, 0, 0],
    };
    let mut output = gpu.storage("Signed response RGB", pixels * 12)?;
    let uniform = gpu.uniform(&params);
    let tables = gpu.upload("Signed response tables", tables)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::SignedPhotographic,
        &mut [
            (0, Read(&uniform)),
            (1, Read(source)),
            (2, Read(detail)),
            (4, Read(&tables)),
            (5, Write(output.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fingerprint(values: &[i32]) -> u64 {
        values
            .iter()
            .flat_map(|x| x.to_le_bytes())
            .fold(14695981039346656037, |h, b| {
                (h ^ u64::from(b)).wrapping_mul(1099511628211)
            })
    }
    #[test]
    fn full_tables_match_independent_recovered_equations() {
        // Complete-table fingerprints generated with the independent research
        // scalars, including signed extensions, tone joins and variable knots.
        let exp = exponential();
        assert_eq!(fingerprint(&exp), 0x1f07cd9cda2ac7e0);
        assert_eq!(fingerprint(&root()), 0x386f328e77bef66e);
        assert_eq!(fingerprint(&gamma()), 0x3da97a3391d5a76f);
        assert_eq!(
            fingerprint(&tone(
                Controls::default(),
                [1.; 3],
                ToneAdjustments::default(),
                &exp
            )),
            0xbbb4bd4357af67a0
        );
        assert_eq!(
            fingerprint(&tone(
                Controls::default(),
                [1.; 3],
                ToneAdjustments {
                    exposure: 0.6,
                    shadows: -0.2,
                    contrast: 0.4,
                    highlights: 0.2
                },
                &exp
            )),
            0xae8f7b1f1601677f
        );
        for (controls, gains, expected) in [
            ([0.7, -0.4, 0.3, -0.2], [0.9, 1.1, 0.8], 0x5a7b9c388b812522),
            ([-0.8, 0.6, -0.5, 0.4], [1.; 3], 0x8966547060b31cdc),
        ] {
            let c = Controls {
                exposure: controls[0],
                shadows: controls[1],
                contrast: controls[2],
                highlights: controls[3],
                ..Controls::default()
            };
            assert_eq!(
                fingerprint(&tone(c, gains, ToneAdjustments::default(), &exp)),
                expected
            );
        }
    }
}

#[cfg(test)]
#[test]
#[ignore = "requires a working GPU adapter"]
fn signed_response_gpu_generic_output_measures_captured_stage4_difference() {
    use crate::{gpu::Gpu, profile::Profile};
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/data/signed-lut-finish-reference.json"
    ))
    .unwrap();
    let values = |name: &str| -> Vec<f32> {
        fixture[name]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap() as f32)
            .collect()
    };
    let gpu = Gpu::new().unwrap();
    let source = gpu
        .upload("Captured signed Stage3 RGB", &values("source"))
        .unwrap();
    let detail = gpu
        .upload("Captured signed Stage3 detail", &values("detail"))
        .unwrap();
    let prepared = prepare(
        Controls::default(),
        [1.; 3],
        ToneAdjustments {
            exposure: 0.6,
            shadows: -0.2,
            contrast: 0.4,
            highlights: 0.2,
        },
        crate::color_space::TransferFunction::Power(461. / 256.),
    )
    .unwrap();
    let output = apply(
        &gpu,
        &source,
        &detail,
        [64, 8],
        &prepared,
        &mut Profile::default(),
    )
    .unwrap();
    let bytes = gpu.readback(&output).unwrap();
    let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
    let expected = values("expected");
    let transform =
        crate::color_space::TiffColorSpace::Srgb.transform(crate::image::ColorSpace::ProPhotoRgb);
    let mut maximum = 0_f32;
    let mut total = 0_f64;
    for (rgb, codes) in actual
        .as_chunks::<3>()
        .0
        .iter()
        .zip(expected.as_chunks::<3>().0)
    {
        for c in 0..3 {
            let linear = (0..3)
                .map(|j| transform[c][j] * rgb[j])
                .sum::<f32>()
                .clamp(0., 1.);
            let encoded = if linear <= 0.0031308 {
                linear * 12.92
            } else {
                1.055 * linear.powf(1. / 2.4) - 0.055
            };
            let error = ((encoded * 65535. + 0.5).floor() - codes[c]).abs();
            maximum = maximum.max(error);
            total += f64::from(error);
        }
    }
    let mean = total / actual.len() as f64;
    eprintln!("Generic output vs legacy profile RGB16: max={maximum}, mean={mean}");
    // This fixture measures replacing the legacy profile, rather than claiming
    // its fixed-point behavior is identical to the standard output transform.
    assert!(
        maximum <= 16. && mean <= 5.,
        "unexpected profile conversion difference"
    );
}

#[cfg(test)]
#[path = "signed_response_test.rs"]
mod response_tests;

#[cfg(test)]
#[test]
#[ignore = "requires a working GPU adapter"]
fn linear_detail_preserves_signed_codes_and_headroom_without_photographic_tone() {
    use crate::{gpu::Gpu, profile::Profile};
    let gpu = Gpu::new().unwrap();
    let source = gpu
        .upload("Root samples", &[4096_f32, 0., -4096., 8192., 4096., 0.])
        .unwrap();
    let detail = gpu.upload("Signed spatial detail", &[0_f32, 32.]).unwrap();
    let prepared = prepare_linear_detail(0.).unwrap();
    let output = apply_linear_detail(
        &gpu,
        &source,
        &detail,
        [2, 1],
        &prepared,
        &mut Profile::default(),
    )
    .unwrap();
    let bytes = gpu.readback(&output).unwrap();
    let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
    let expected: Vec<f32> = [4096_i32, 0, -4096, 8224, 4128, 32]
        .map(|root| {
            let x = f64::from(root) / 4096.;
            let code = if root < 819 {
                ((x - 0.2) * 0.12000000000000002 * 4096.).round().max(-288.)
            } else {
                (x.mul_add(x * x, -0.008000000000000002) * 4096.)
                    .round()
                    .min(32767.)
            };
            code as f32
        })
        .to_vec();
    assert_eq!(actual, expected);
    assert!(actual[0] > 4096. * 0.9);
    assert!(actual[2] < 0.);
    assert!(actual[3] > 7. * 4096.);
}

#[cfg(test)]
#[test]
#[ignore = "requires a working GPU adapter"]
fn pointwise_profile_response_matches_gpu_without_spatial_detail() {
    use crate::{gpu::Gpu, profile::Profile};
    let gpu = Gpu::new().unwrap();
    let prepared = prepare(
        Controls {
            exposure: 0.3,
            saturation: 0.7,
            ..Controls::default()
        },
        [1.; 3],
        ToneAdjustments::default(),
        crate::color_space::TransferFunction::Power(461. / 256.),
    )
    .unwrap();
    let inputs = [[-0.02, 0.01, 0.1], [0.12, 0.5, 1.3], [3., 0.8, 0.01]];
    let root = root();
    let source: Vec<f32> = inputs
        .iter()
        .flat_map(|rgb| {
            rgb.iter().map(|value| {
                let code = (value * 4096_f64).round() as i32;
                root[(code & LUT_MASK) as usize] as f32
            })
        })
        .collect();
    let source = gpu.upload("Profile root samples", &source).unwrap();
    let detail = gpu.upload("No spatial detail", &[0_f32; 3]).unwrap();
    let output = apply(
        &gpu,
        &source,
        &detail,
        [3, 1],
        &prepared,
        &mut Profile::default(),
    )
    .unwrap();
    let bytes = gpu.readback(&output).unwrap();
    let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
    let technical = apply_linear_detail(
        &gpu,
        &source,
        &detail,
        [3, 1],
        &prepare_linear_detail(0.).unwrap(),
        &mut Profile::default(),
    )
    .unwrap();
    let technical = gpu.readback(&technical).unwrap();
    // The profile consumes the actual technical GPU output. It must not repeat
    // the root expansion and its quantization before applying channel tone.
    let expected: Vec<f32> = bytemuck::cast_slice::<u8, f32>(&technical)
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|rgb| {
            prepared
                .evaluate_linear(rgb.map(|v| f64::from(v) / SIGNAL_CODE_UNIT))
                .map(|x| x as f32)
        })
        .collect();
    assert_eq!(actual, expected);
}
