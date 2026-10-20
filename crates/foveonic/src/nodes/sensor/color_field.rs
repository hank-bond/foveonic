//! Owns original evidence, GPU brightness/color solves, and native highlight application.
//!
//! N2 equations are adapted from the reference implementation at 2b0dd443.
//! Copyright 2026 Sang Lee and contributors. Apache-2.0. See color_field/NOTICE.

use super::calibration::CalibrationParams;
use crate::{
    gpu::{Gpu, GpuTrace, Kernel, ReadGuard},
    interactive::Execution,
    profile::Profile,
    source::{Metadata, Result},
};

const SOLVER_GROUP_SIZE: u32 = 256;
const STRICT_COLOR_CUTOFF: f32 = 0.005;

#[cfg(test)]
#[path = "color_field_integration_test.rs"]
mod integration_tests;
#[cfg(test)]
#[path = "color_field_reliability_test.rs"]
mod reliability_tests;
#[cfg(test)]
#[path = "color_field_test.rs"]
mod tests;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
/// Recovery family. Color-field choices share this operation and select a brightness solver.
pub enum HighlightMode {
    #[default]
    Classic,
    ColorField,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
/// Brightness solving only. Both choices share the same color reconstruction.
pub enum Approach {
    #[default]
    #[serde(
        alias = "anchors",
        alias = "uniform_support",
        alias = "coarse",
        alias = "fast_solve",
        alias = "fine"
    )]
    Faithful,
    LocalGradient,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "SavedControls")]
pub struct Controls {
    /// Selection strength, 0..1. Zero keeps the original behavior.
    pub selectivity: f32,
    pub approach: Approach,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            selectivity: 0.,
            approach: Approach::Faithful,
        }
    }
}
// Input-only migration. Retired automatic settings become zero, saved raw cutoffs
// become equivalent strengths, and neutral-restraint amounts are discarded.
#[derive(serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
struct SavedControls {
    #[serde(rename = "donation")]
    _donation: serde::de::IgnoredAny,
    #[serde(rename = "agreement")]
    _agreement: serde::de::IgnoredAny,
    #[serde(rename = "boundary_ev")]
    _boundary_ev: serde::de::IgnoredAny,
    #[serde(rename = "area")]
    _area: serde::de::IgnoredAny,
    // Distinguish an omitted setting from retired explicit null, so current settings win.
    #[serde(deserialize_with = "saved_selectivity")]
    selectivity: Option<Option<f32>>,
    cutoff: Option<f32>,
    approach: Approach,
    #[serde(rename = "restraint")]
    _retired_restraint: serde::de::IgnoredAny,
}
impl Default for SavedControls {
    fn default() -> Self {
        let controls = Controls::default();
        Self {
            _donation: serde::de::IgnoredAny,
            _agreement: serde::de::IgnoredAny,
            _boundary_ev: serde::de::IgnoredAny,
            _area: serde::de::IgnoredAny,
            selectivity: None,
            cutoff: None,
            approach: controls.approach,
            _retired_restraint: serde::de::IgnoredAny,
        }
    }
}
fn saved_selectivity<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Option<f32>>, D::Error> {
    <Option<f32> as serde::Deserialize>::deserialize(deserializer).map(Some)
}
impl TryFrom<SavedControls> for Controls {
    type Error = &'static str;
    fn try_from(saved: SavedControls) -> std::result::Result<Self, Self::Error> {
        let selectivity = match saved.selectivity {
            Some(value) => value.unwrap_or(0.),
            None => match saved.cutoff {
                None => 0.,
                Some(cutoff) if (0.005..=0.02).contains(&cutoff) => STRICT_COLOR_CUTOFF / cutoff,
                Some(_) => return Err("retired color-field cutoff must be 0.005..0.02"),
            },
        };
        Ok(Self {
            selectivity,
            approach: saved.approach,
        })
    }
}

impl Controls {
    pub fn validate(self) -> Result<()> {
        if !(0. ..=1.).contains(&self.selectivity) {
            return Err("color-field selectivity must be 0..1".into());
        }
        Ok(())
    }

    fn cutoff(self) -> f32 {
        if self.selectivity > 0. {
            // The reciprocal approaches an unfiltered solve smoothly at zero.
            // Keep twice the cutoff finite even for subnormal input amounts.
            (f64::from(STRICT_COLOR_CUTOFF) / f64::from(self.selectivity))
                .min(f64::from(f32::MAX) * 0.25) as f32
        } else {
            0.
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Params {
    pub size: [u32; 4],
    pub grid: [u32; 4],
    pub crop: [u32; 4],
    pub gain: [f32; 4],
    pub white: [f32; 4],
    pub neutral: [f32; 4],
    pub luminance: [f32; 4],
    pub metric: [f32; 4],
    pub tuning: [f32; 4],
    pub options: [u32; 4],
}

pub(crate) struct Prepared {
    pub params: Params,
    pub repairs: Vec<u32>,
}
impl Prepared {
    pub(crate) fn key(&self) -> crate::recipe::Key {
        crate::recipe::Key::pod(&self.params).slice(&self.repairs)
    }
}

fn multiply(a: [f64; 9], b: [f64; 9]) -> [f64; 9] {
    std::array::from_fn(|i| (0..3).map(|k| a[i / 3 * 3 + k] * b[k * 3 + i % 3]).sum())
}
fn neutral(m: [f64; 9]) -> Result<[f64; 3]> {
    let [a, b, c, d, e, f, g, h, i] = m;
    let inverse = [
        e * i - f * h,
        c * h - b * i,
        b * f - c * e,
        f * g - d * i,
        a * i - c * g,
        c * d - a * f,
        d * h - e * g,
        b * g - a * h,
        a * e - b * d,
    ];
    let determinant = a * inverse[0] + b * inverse[3] + c * inverse[6];
    let p: [f64; 3] =
        std::array::from_fn(|r| inverse[r * 3..r * 3 + 3].iter().sum::<f64>() / determinant);
    let maximum = p.into_iter().fold(0_f64, f64::max);
    if p.iter().any(|v| !v.is_finite() || *v <= 0.) {
        return Err("invalid color-field neutral calibration".into());
    }
    Ok(p.map(|v| v / maximum))
}
fn color_metric(p: [f64; 3], m: [f64; 9]) -> Result<[f32; 4]> {
    let rgb: [f64; 3] = std::array::from_fn(|r| (0..3).map(|c| m[3 * r + c] * p[c]).sum());
    if rgb.iter().any(|v| !v.is_finite() || *v <= 0.) {
        return Err("invalid color-field rendered neutral".into());
    }
    let j: [[f64; 2]; 2] =
        [0, 2].map(|r| std::array::from_fn(|c| p[c] * (m[3 * r + c] / rgb[r] - m[3 + c] / rgb[1])));
    let a = j[0][0] * j[0][0] + j[1][0] * j[1][0];
    let b = j[0][1] * j[0][1] + j[1][1] * j[1][1];
    let c = j[0][0] * j[0][1] + j[1][0] * j[1][1];
    let minimum = 0.5 * (a + b - ((a - b) * (a - b) + 4. * c * c).sqrt());
    if !minimum.is_finite() || minimum <= 1e-9 {
        return Err("singular color-field neutral metric".into());
    }
    Ok([
        (a / minimum) as f32,
        (b / minimum) as f32,
        (c / minimum) as f32,
        0.,
    ])
}

pub(crate) fn prepare(
    metadata: &Metadata,
    wb: &str,
    calibration: CalibrationParams,
    repair: bool,
    controls: Controls,
) -> Result<Prepared> {
    controls.validate()?;
    let matrix_key = metadata
        .lists
        .get("WhiteBalanceColorCorrections")
        .and_then(|v| v.get(wb))
        .ok_or("missing color-field camera matrix")?;
    let cc = metadata.fixed::<9>(matrix_key)?;
    let gain = calibration.gains.map(|v| f64::from(v) * 0.5);
    let matrix = std::array::from_fn(|i| cc[i] * gain[i % 3]);
    let p = neutral(matrix)?;
    // Match the reference's DNG luminance derivation, including its 1e-4 tag rounding.
    let xyz = multiply(
        [
            0.4124564, 0.3575761, 0.1804375, 0.2126729, 0.7151522, 0.0721750, 0.0193339, 0.1191920,
            0.9503041,
        ],
        cc,
    );
    let forward = multiply(
        [
            1.0478112, 0.0228866, -0.0501270, 0.0295424, 0.9904844, -0.0170491, -0.0092345,
            0.0150436, 0.7521316,
        ],
        xyz,
    );
    let row: [f64; 3] =
        std::array::from_fn(|c| ((forward[3 + c] as f32 as f64) * 10000.).round() / 10000.);
    let tags: [f64; 3] = std::array::from_fn(|c| (10000. / gain[c]).round().max(1.) / 10000.);
    let maximum = tags.into_iter().fold(0_f64, f64::max);
    let sum: f64 = row.iter().sum();
    let y: [f32; 4] = std::array::from_fn(|c| {
        if c < 3 {
            (row[c] / sum / (tags[c] / maximum)) as f32
        } else {
            0.
        }
    });
    if y.iter().any(|v| !v.is_finite()) {
        return Err("invalid color-field luminance".into());
    }
    // Recover before the active-area crop, preserving the sensor grid and neighbors.
    let crop = [0, 0, metadata.width as u32, metadata.height as u32];
    let white = if metadata.matrix("ImageDepth").is_some() {
        let depth = metadata.fixed::<1>("ImageDepth")?[0];
        if depth.fract() != 0. || !(1. ..=16.).contains(&depth) {
            return Err("invalid color-field image depth".into());
        }
        [((1_u32 << depth as u32) - 1) as f32; 3]
    } else {
        metadata.fixed::<3>("RawSaturationLevel")?.map(|v| v as f32)
    };
    if white.iter().any(|v| !v.is_finite() || *v <= 0.) {
        return Err("invalid color-field raw white levels".into());
    }
    let mut repairs = Vec::new();
    if repair {
        for key in ["BadPixelsF20", "Jpeg_BadClusters"] {
            if let Some(m) = metadata.matrix(key) {
                if m.values.len() % 3 != 0 {
                    return Err("invalid color-field repair list".into());
                }
                for record in m.values.as_chunks::<3>().0 {
                    let yy = record[0] as u32;
                    let xx = record[1] as u32;
                    if xx < metadata.width as u32 && yy < metadata.height as u32 {
                        repairs.push(yy * metadata.width as u32 + xx);
                    }
                }
            }
        }
    }
    repairs.sort_unstable();
    repairs.dedup();
    let step = 8;
    let cols = (crop[2] - crop[0]).div_ceil(step);
    let rows = (crop[3] - crop[1]).div_ceil(step);
    Ok(Prepared {
        params: Params {
            size: [
                metadata.width as u32,
                metadata.height as u32,
                (metadata.width * metadata.height) as u32,
                repairs.len() as u32,
            ],
            grid: [cols, rows, cols * rows, step],
            crop,
            gain: calibration.gains,
            white: [white[0], white[1], white[2], 0.],
            neutral: [p[0] as f32, p[1] as f32, p[2] as f32, controls.cutoff()],
            luminance: y,
            metric: color_metric(p, matrix)?,
            tuning: [0.035, 0.75, 1., 1.],
            options: [
                match controls.approach {
                    Approach::Faithful => 1,
                    Approach::LocalGradient => 2,
                },
                1,
                u32::from(repair),
                0,
            ],
        },
        repairs,
    })
}

fn dispatch(
    gpu: &Gpu,
    k: Kernel,
    buffers: &mut [(u32, ReadGuard)],
    count: u32,
    trace: &mut GpuTrace,
) {
    let group = if matches!(k, Kernel::FieldGrid) {
        64
    } else if k as usize >= Kernel::FieldSolveInit as usize {
        SOLVER_GROUP_SIZE
    } else {
        256
    };
    gpu.field_dispatch(k, buffers, count.div_ceil(group), trace);
}
fn buffer(buffers: &[(u32, ReadGuard)], slot: u32) -> &ReadGuard {
    &buffers
        .iter()
        .find(|(s, _)| *s == slot)
        .expect("color-field resource")
        .1
}
fn replace(buffers: &mut [(u32, ReadGuard)], slot: u32, value: ReadGuard) {
    buffers
        .iter_mut()
        .find(|(s, _)| *s == slot)
        .expect("color-field resource")
        .1 = value;
}
fn words(gpu: &Gpu, b: &ReadGuard) -> Result<Vec<u32>> {
    let v = gpu.readback(b)?;
    Ok(bytemuck::cast_slice::<u8, u32>(&v).to_vec())
}

pub(crate) struct Inputs<'a> {
    pub input: &'a ReadGuard,
    pub original: &'a ReadGuard,
    pub black: &'a ReadGuard,
    pub unrepaired: &'a ReadGuard,
}

pub(crate) fn apply(
    gpu: &Gpu,
    inputs: Inputs<'_>,
    p: &Prepared,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<ReadGuard> {
    execution.check()?;
    let Inputs {
        input,
        original,
        black,
        unrepaired,
    } = inputs;
    profile.measure("prepare optional color-field kernels", || {
        gpu.prepare_color_field()
    })?;
    let n = p.params.size[2];
    let g = p.params.grid[2];
    let mut trace = gpu.trace(profile.mode.gpu(), 32);
    let mut resources = vec![
        (0, gpu.uniform(&p.params)),
        (1, input.clone()),
        (2, original.clone()),
        (3, black.clone()),
        (
            4,
            gpu.storage("Color-field original reliability", u64::from(n) * 4)?,
        ),
        (
            5,
            gpu.storage("Color-field active positions", u64::from(n) * 4)?,
        ),
        (
            6,
            gpu.storage("Color-field active indices", u64::from(n) * 4)?,
        ),
        (
            7,
            gpu.storage("Color-field log brightness", u64::from(n) * 4)?,
        ),
        (8, gpu.storage("Color-field counts", 16)?),
        (
            9,
            gpu.upload(
                "Color-field marked repair sites",
                if p.repairs.is_empty() {
                    &[0]
                } else {
                    &p.repairs
                },
            )?,
        ),
        (18, unrepaired.clone()),
        (
            19,
            gpu.storage(
                "Color-field row block totals",
                u64::from(n.max(g).div_ceil(256)) * 4,
            )?,
        ),
        (20, gpu.uniform(&[n, 0_u32, 0, 0])),
    ];
    gpu.field_batch(
        &[
            (Kernel::FieldPrepare, n.div_ceil(256)),
            (Kernel::FieldRank, n.div_ceil(256)),
            (Kernel::FieldPrefix, 1),
            (Kernel::FieldScatter, n.div_ceil(256)),
        ],
        &mut resources,
        &mut trace,
    );
    let count = words(gpu, buffer(&resources, 8))?[0];
    if count == 0 {
        gpu.finish_trace(&trace, profile)?;
        return Ok(input.clone());
    }
    if p.params.size[3] > 0 {
        dispatch(
            gpu,
            Kernel::FieldRepairs,
            &mut resources,
            p.params.size[3],
            &mut trace,
        );
    }
    resources.push((
        10,
        gpu.storage("Color-field native equations", u64::from(count) * 48)?,
    ));
    resources.push((
        11,
        gpu.storage("Color-field native solution", u64::from(count) * 4)?,
    ));
    dispatch(gpu, Kernel::FieldNative, &mut resources, count, &mut trace);
    gpu.finish_trace(&trace, profile)?;
    let equations = buffer(&resources, 10).clone();
    let solution = resources
        .iter_mut()
        .find(|(s, _)| *s == 11)
        .map(|(_, b)| b)
        .unwrap();
    if p.params.options[0] == 2 {
        relax(gpu, &equations, solution, count, profile, execution)?;
    } else {
        solve(
            gpu,
            &equations,
            solution,
            Solve {
                count,
                metric: [1., 0., 0., 0.],
                domain: Domain::Brightness,
                fixed_rhs_max: 0.,
            },
            profile,
            execution,
        )?;
    }
    trace = gpu.trace(profile.mode.gpu(), 32);
    dispatch(gpu, Kernel::FieldTone, &mut resources, count, &mut trace);
    resources.retain(|(s, _)| *s != 10 && *s != 11);
    resources.extend([
        (
            12,
            gpu.storage("Color-field grid evidence", u64::from(g) * 64)?,
        ),
        (
            13,
            gpu.storage("Color-field graph edges", u64::from(g) * 8)?,
        ),
        (
            14,
            gpu.storage("Color-field region roots", u64::from(g) * 4)?,
        ),
        (
            15,
            gpu.storage("Color-field region peaks", u64::from(g) * 8)?,
        ),
        (
            16,
            gpu.storage("Color-field donor distances", u64::from(g) * 4)?,
        ),
    ]);
    replace(&mut resources, 20, gpu.uniform(&[g, 2_u32, 0, 0]));
    gpu.field_batch(
        &[
            (Kernel::FieldGrid, g.div_ceil(64)),
            (Kernel::FieldRank, g.div_ceil(256)),
            (Kernel::FieldPrefix, 1),
            (Kernel::FieldScatter, g.div_ceil(256)),
            (Kernel::FieldConnect, g.div_ceil(256)),
        ],
        &mut resources,
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    let color_counts = words(gpu, buffer(&resources, 8))?;
    let color_count = color_counts[2];
    // Fixed rows no longer occupy solver storage, but still set the original
    // RHS-relative stopping bound. Preserve that norm through graph checks.
    let graph_counts = [count, 0, color_count, color_counts[3]];
    let start = std::time::Instant::now();
    for batch in 0..128 {
        execution.check()?;
        replace(
            &mut resources,
            8,
            gpu.upload("Color-field graph convergence", &graph_counts)?,
        );
        let mut t = gpu.trace(profile.mode.gpu(), 8);
        gpu.field_batch(
            &[(Kernel::FieldUnion, g.div_ceil(256)); 8],
            &mut resources,
            &mut t,
        );
        gpu.finish_trace(&t, profile)?;
        if words(gpu, buffer(&resources, 8))?[1] == 0 {
            break;
        }
        if batch == 127 {
            return Err("color-field region union did not converge".into());
        }
    }
    profile.record("color-field region construction", start);
    trace = gpu.trace(profile.mode.gpu(), 8);
    gpu.field_batch(
        &[
            Kernel::FieldPeaks,
            Kernel::FieldPeakIndices,
            Kernel::FieldCondition,
        ]
        .map(|k| (k, g.div_ceil(256))),
        &mut resources,
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    let start = std::time::Instant::now();
    for batch in 0..128 {
        execution.check()?;
        replace(
            &mut resources,
            8,
            gpu.upload("Color-field distance convergence", &graph_counts)?,
        );
        let mut t = gpu.trace(profile.mode.gpu(), 32);
        gpu.field_batch(
            &[(Kernel::FieldDistance, g.div_ceil(256)); 32],
            &mut resources,
            &mut t,
        );
        gpu.finish_trace(&t, profile)?;
        if words(gpu, buffer(&resources, 8))?[1] == 0 {
            break;
        }
        if batch == 127 {
            return Err("color-field donor distance did not converge".into());
        }
    }
    profile.record("color-field donor distance solve", start);
    resources.push((
        10,
        gpu.storage(
            "Color-field color equations",
            u64::from(color_count.max(1)) * 80,
        )?,
    ));
    resources.push((
        11,
        gpu.storage(
            "Color-field color solution",
            u64::from(color_count.max(1)) * 16,
        )?,
    ));
    if color_count > 0 {
        trace = gpu.trace(profile.mode.gpu(), 4);
        dispatch(
            gpu,
            Kernel::FieldColor,
            &mut resources,
            color_count,
            &mut trace,
        );
        gpu.finish_trace(&trace, profile)?;
        let equations = buffer(&resources, 10).clone();
        solve(
            gpu,
            &equations,
            resources
                .iter_mut()
                .find(|(s, _)| *s == 11)
                .map(|(_, b)| b)
                .unwrap(),
            Solve {
                count: color_count,
                metric: [
                    p.params.metric[0],
                    p.params.metric[1],
                    p.params.metric[2],
                    p.params.tuning[3],
                ],
                domain: Domain::Color,
                fixed_rhs_max: f32::from_bits(color_counts[3]),
            },
            profile,
            execution,
        )?;
    }
    trace = gpu.trace(profile.mode.gpu(), 4);
    dispatch(gpu, Kernel::FieldCommit, &mut resources, g, &mut trace);
    resources.push((
        17,
        gpu.storage("Color-field restored layers", u64::from(n) * 12)?,
    ));
    dispatch(gpu, Kernel::FieldApply, &mut resources, n, &mut trace);
    gpu.finish_trace(&trace, profile)?;
    Ok(resources
        .swap_remove(resources.iter().position(|(s, _)| *s == 17).unwrap())
        .1)
}

fn relax(
    gpu: &Gpu,
    equations: &ReadGuard,
    x: &mut ReadGuard,
    count: u32,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<()> {
    execution.check()?;
    let start = std::time::Instant::now();
    let owned = std::mem::replace(x, gpu.storage("Local gradient placeholder", 16)?);
    let mut resources = vec![
        (
            0,
            gpu.uniform(&SolverParams {
                size: [count, count.div_ceil(SOLVER_GROUP_SIZE), 0, 0],
                metric: [1., 0., 0., 0.],
                tolerance: [0.; 4],
            }),
        ),
        (1, equations.clone()),
        (2, owned),
        (
            5,
            gpu.storage("Local gradient ping-pong", u64::from(count) * 4)?,
        ),
    ];
    let tasks: Vec<_> = (0..32)
        .flat_map(|_| {
            [
                (Kernel::FieldRelaxForward, count.div_ceil(SOLVER_GROUP_SIZE)),
                (
                    Kernel::FieldRelaxBackward,
                    count.div_ceil(SOLVER_GROUP_SIZE),
                ),
            ]
        })
        .collect();
    let mut trace = gpu.trace(profile.mode.gpu(), 64);
    gpu.field_batch(&tasks, &mut resources, &mut trace);
    gpu.finish_trace(&trace, profile)?;
    *x = resources
        .swap_remove(resources.iter().position(|(s, _)| *s == 2).unwrap())
        .1;
    profile.record("color-field local gradient solve", start);
    Ok(())
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SolverParams {
    size: [u32; 4],
    metric: [f32; 4],
    tolerance: [f32; 4],
}
#[derive(Clone, Copy)]
enum Domain {
    Brightness,
    Color,
}
struct Solve {
    count: u32,
    metric: [f32; 4],
    domain: Domain,
    fixed_rhs_max: f32,
}
fn finish_solver_trace(
    gpu: &Gpu,
    trace: &GpuTrace,
    profile: &mut Profile,
    domain: Domain,
) -> Result<()> {
    let first = profile.gpu.len();
    gpu.finish_trace(trace, profile)?;
    for t in &mut profile.gpu[first..] {
        t.stage = match (domain, t.stage) {
            (Domain::Brightness, "color-field true residual check") => "gradient CG true residual",
            (Domain::Brightness, "color-field initial reduction") => {
                "gradient CG initial reduction"
            }
            (Domain::Brightness, "color-field solver matrix product") => {
                "gradient CG matrix product"
            }
            (Domain::Brightness, "color-field solver alpha reduction") => {
                "gradient CG alpha reduction"
            }
            (Domain::Brightness, "color-field solver update") => "gradient CG update",
            (Domain::Brightness, "color-field solver beta reduction") => {
                "gradient CG beta reduction"
            }
            (Domain::Brightness, "color-field solver direction") => "gradient CG direction",
            (Domain::Color, "color-field true residual check") => "color CG true residual",
            (Domain::Color, "color-field initial reduction") => "color CG initial reduction",
            (Domain::Color, "color-field solver matrix product") => "color CG matrix product",
            (Domain::Color, "color-field solver alpha reduction") => "color CG alpha reduction",
            (Domain::Color, "color-field solver update") => "color CG update",
            (Domain::Color, "color-field solver beta reduction") => "color CG beta reduction",
            (Domain::Color, "color-field solver direction") => "color CG direction",
            (_, name) => name,
        };
    }
    Ok(())
}
fn solve(
    gpu: &Gpu,
    equations: &ReadGuard,
    x: &mut ReadGuard,
    spec: Solve,
    profile: &mut Profile,
    execution: &mut Execution<'_>,
) -> Result<()> {
    let Solve {
        count,
        metric,
        domain,
        fixed_rhs_max,
    } = spec;
    let (maximum, label, tolerance) = match domain {
        Domain::Brightness => (1000, "color-field gradient solve", 5e-7),
        // Tighter color residuals hit float32 matrix-product precision on
        // high-chroma scenes. Brightness needs the tighter bound; full-raster
        // reference tests independently check the combined output error.
        Domain::Color => (4000, "color-field color solve", 2e-6),
    };
    let start = std::time::Instant::now();
    let vector_bytes = if matches!(domain, Domain::Color) {
        8
    } else {
        4
    };
    let owned = std::mem::replace(x, gpu.storage("Color-field solution placeholder", 16)?);
    let mut b = vec![
        (
            0,
            gpu.uniform(&SolverParams {
                size: [
                    count,
                    count.div_ceil(SOLVER_GROUP_SIZE),
                    u32::from(matches!(domain, Domain::Color)),
                    0,
                ],
                metric,
                tolerance: [tolerance, fixed_rhs_max, 0., 0.],
            }),
        ),
        (1, equations.clone()),
        (2, owned),
        (
            3,
            gpu.storage(
                "Color-field solver residual",
                u64::from(count) * vector_bytes * 2,
            )?,
        ),
        (
            4,
            gpu.storage(
                "Color-field solver direction",
                u64::from(count) * vector_bytes,
            )?,
        ),
        (
            5,
            gpu.storage(
                "Color-field solver product",
                u64::from(count) * vector_bytes,
            )?,
        ),
        (
            6,
            gpu.storage(
                "Color-field solver reductions",
                u64::from(count.div_ceil(SOLVER_GROUP_SIZE)) * 16,
            )?,
        ),
        (7, gpu.storage("Color-field solver state", 32)?),
    ];
    let checkpoint = [
        (Kernel::FieldSolveInit, count.div_ceil(SOLVER_GROUP_SIZE)),
        (Kernel::FieldSolveState, 1),
    ];
    execution.check()?;
    let mut trace = gpu.trace(profile.mode.gpu(), 2);
    gpu.field_batch(&checkpoint, &mut b, &mut trace);
    finish_solver_trace(gpu, &trace, profile, domain)?;
    let mut iterations = 0;
    loop {
        execution.check()?;
        let state = gpu.readback(buffer(&b, 7))?;
        let state = bytemuck::cast_slice::<u8, f32>(&state);
        if state[6] != 0. || !state[4].is_finite() {
            return Err(format!("{label}: invalid preconditioner or nonfinite residual").into());
        }
        if state[3] != 0. {
            break;
        }
        if iterations >= maximum {
            return Err(format!("{label}: failed to converge after {iterations} iterations, residual {} tolerance {}",state[4],state[5]).into());
        }
        let steps = 16.min(maximum - iterations);
        let mut trace = gpu.trace(profile.mode.gpu(), steps * 5 + 2);
        let tasks: Vec<_> = (0..steps)
            .flat_map(|_| {
                [
                    (Kernel::FieldSolveMap, count.div_ceil(SOLVER_GROUP_SIZE)),
                    (Kernel::FieldSolveAlpha, 1),
                    (Kernel::FieldSolveStep, count.div_ceil(SOLVER_GROUP_SIZE)),
                    (Kernel::FieldSolveBeta, 1),
                    (
                        Kernel::FieldSolveDirection,
                        count.div_ceil(SOLVER_GROUP_SIZE),
                    ),
                ]
            })
            .chain(checkpoint)
            .collect();
        gpu.field_batch(&tasks, &mut b, &mut trace);
        finish_solver_trace(gpu, &trace, profile, domain)?;
        iterations += steps;
    }
    *x = b
        .swap_remove(b.iter().position(|(s, _)| *s == 2).unwrap())
        .1;
    profile.record(label, start);
    Ok(())
}
