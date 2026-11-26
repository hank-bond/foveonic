//! Estimates dark-channel haze and applies bounded correction in recipe-supplied camera RGB.

use crate::{
    color_space::{ColorSpace, TiffColorSpace},
    gpu::{
        Binding::{Read, Write},
        Gpu, GpuTrace, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    render::ProcessingQuality,
    source::Result,
};
use serde::{Deserialize, Serialize};

const ESTIMATE_PARTITIONS: u32 = 16;
const HISTOGRAM_BINS: u32 = 256;
// Each histogram workgroup lane clears and flushes one bin and reads one column,
// so a workgroup covers HISTOGRAM_BINS columns and HISTOGRAM_ROWS rows.
const HISTOGRAM_ROWS: u32 = 16;
const EXPANDED_MINIMUM_RADIUS: u32 = 16;

fn tail_partitions(shape: [u32; 2]) -> u32 {
    (u64::from(shape[0]) * u64::from(shape[1]))
        .div_ceil(65536)
        .next_power_of_two()
        .clamp(u64::from(ESTIMATE_PARTITIONS), 256) as u32
}

/// Manual shadow colors use normalized linear sRGB, independently of display encoding.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Controls {
    pub amount: f32,
    pub shadow: Option<[f32; 3]>,
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            amount: 0.,
            shadow: Some([0.5; 3]),
        }
    }
}
impl Controls {
    pub fn validate(self) -> Result<()> {
        if !self.amount.is_finite() || !(-100. ..=100.).contains(&self.amount) {
            return Err("Dehaze amount must be between -100 and 100".into());
        }
        if self.shadow.is_some_and(|rgb| {
            rgb.iter()
                .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
                || rgb.iter().all(|v| *v == 0.)
        }) {
            return Err("Dehaze shadow tone must be a nonzero normalized linear RGB color".into());
        }
        Ok(())
    }
}

/// White-normalized camera coordinates supplied by the photographic recipe.
/// Storage colors remain linear sRGB. Pixel processing uses the camera basis.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Basis {
    to_camera: [[f32; 4]; 3],
    from_camera: [[f32; 4]; 3],
    to_srgb: [[f32; 4]; 3],
    from_srgb: [[f32; 4]; 3],
    luma: [f32; 4],
}
impl Basis {
    pub fn new(space: ColorSpace, camera: [f64; 9], luma: [f32; 3]) -> Result<Self> {
        let inverse = crate::color_space::inverse(camera);
        let white: [f64; 3] = std::array::from_fn(|r| inverse[r * 3..r * 3 + 3].iter().sum());
        if camera.iter().chain(inverse.iter()).any(|v| !v.is_finite())
            || white.iter().any(|v| !v.is_finite() || *v <= 0.)
            || luma.iter().any(|v| !v.is_finite() || *v < 0.)
            || (luma.iter().sum::<f32>() - 1.).abs() > 1e-5
        {
            return Err("invalid Dehaze camera basis".into());
        }
        // Camera calibration can include ISO gain. Normalize its neutral white
        // so histogram thresholds and manual colors share the same signal scale.
        let camera = std::array::from_fn(|i| camera[i] * white[i % 3]);
        let inverse = crate::color_space::inverse(camera);
        let padded = |m: [f64; 9]| {
            std::array::from_fn(|r| {
                [
                    m[r * 3] as f32,
                    m[r * 3 + 1] as f32,
                    m[r * 3 + 2] as f32,
                    0.,
                ]
            })
        };
        let product = |a: [[f32; 4]; 3], b: [[f32; 4]; 3]| {
            std::array::from_fn(|r| {
                std::array::from_fn(|c| {
                    if c == 3 {
                        0.
                    } else {
                        (0..3).map(|k| a[r][k] * b[k][c]).sum()
                    }
                })
            })
        };
        let from_camera = padded(camera);
        let to_camera = padded(inverse);
        let to_srgb = product(TiffColorSpace::Srgb.transform(space), from_camera);
        let from_srgb = product(
            to_camera,
            TiffColorSpace::from(space).transform(ColorSpace::Srgb),
        );
        let basis = Self {
            to_camera,
            from_camera,
            to_srgb,
            from_srgb,
            luma: [luma[0], luma[1], luma[2], 0.],
        };
        if bytemuck::cast_slice::<Self, f32>(&[basis])
            .iter()
            .any(|v| !v.is_finite())
        {
            return Err("unrepresentable Dehaze camera basis".into());
        }
        Ok(basis)
    }
    pub fn monochrome() -> Self {
        Self::new(
            ColorSpace::Srgb,
            [1., 0., 0., 0., 1., 0., 0., 0., 1.],
            [0.2126, 0.7152, 0.0722],
        )
        .unwrap()
    }
    pub fn key(self) -> Key {
        Key::pod(&self)
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Estimate {
    shape: [u32; 4],
    region: [u32; 4],
    selection: [u32; 4],
    // Partition count, Quality tail averaging, reserved.
    partitions: [u32; 4],
    normalization: [f32; 4],
    basis: Basis,
}
impl Estimate {
    pub fn new(
        extent: [u32; 2],
        basis: Basis,
        white: f32,
        mono: bool,
        crop: [u32; 4],
        quality: ProcessingQuality,
    ) -> Self {
        let width = (crop[2] - crop[0]).max(1);
        let height = (crop[3] - crop[1]).max(1);
        let step = width.max(height).div_ceil(512).max(1);
        let virtual_shape = [width.div_ceil(step), height.div_ceil(step)];
        let virtual_radius = virtual_shape[0].min(virtual_shape[1]).min(64) / 4;
        let virtual_count = u64::from(virtual_shape[0]) * u64::from(virtual_shape[1]);
        let virtual_candidates = (virtual_count / 1000).clamp(100, 1000);
        let ([w, h], radius, candidates) = match quality {
            ProcessingQuality::Speed => (virtual_shape, virtual_radius, virtual_candidates as u32),
            ProcessingQuality::Quality => {
                let radius = virtual_radius * step;
                let count = u64::from(width) * u64::from(height);
                let eligible = u64::from(width.saturating_sub(2 * radius))
                    * u64::from(height.saturating_sub(2 * radius));
                // Preserve the overview's selected fraction when every working sample participates.
                let candidates = (virtual_candidates * count)
                    .div_ceil(virtual_count)
                    .min(eligible);
                ([width, height], radius, candidates as u32)
            }
        };
        Self {
            shape: [extent[0], extent[1], w, h],
            region: [crop[0], crop[1], width, height],
            selection: [radius, 16384, candidates, u32::from(mono)],
            partitions: [
                if quality == ProcessingQuality::Quality {
                    tail_partitions([w, h])
                } else {
                    ESTIMATE_PARTITIONS
                },
                u32::from(quality == ProcessingQuality::Quality),
                0,
                0,
            ],
            normalization: [1. / white, 0., 0., 0.],
            basis,
        }
    }
    pub fn patch(
        extent: [u32; 2],
        basis: Basis,
        white: f32,
        mono: bool,
        pixel: [u32; 2],
        crop: [u32; 4],
    ) -> Self {
        let mut region = [0; 4];
        for axis in 0..2 {
            let size = (crop[axis + 2] - crop[axis]).min(133);
            region[axis] = pixel[axis]
                .saturating_sub(size / 2)
                .clamp(crop[axis], crop[axis + 2] - size);
            region[axis + 2] = region[axis] + size;
        }
        let mut params = Self::new(
            extent,
            basis,
            white,
            mono,
            region,
            ProcessingQuality::Quality,
        );
        params.selection[1] = 65535;
        params.partitions[0] = ESTIMATE_PARTITIONS;
        params.partitions[1] = 0;
        params
    }
    pub fn estimates_color(self) -> bool {
        self.normalization[1] == 0.
    }
    pub fn with_color_estimation(mut self, enabled: bool) -> Self {
        self.normalization[1] = f32::from(!enabled);
        self
    }
    pub fn key(self) -> Key {
        Key::pod(&self)
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Params {
    shape: [u32; 4],
    /// Signed amount, code white, reserved.
    values: [f32; 4],
    manual: [f32; 4],
    basis: Basis,
}
impl Params {
    pub fn new(extent: [u32; 2], basis: Basis, white: f32, mono: bool, controls: Controls) -> Self {
        let mut manual = [0.; 4];
        if let Some(rgb) = controls.shadow {
            manual[..3].copy_from_slice(&rgb);
            manual[3] = 1.;
        }
        Self {
            shape: [extent[0], extent[1], u32::from(mono), 0],
            values: [controls.amount / 100., white, 0., 0.],
            manual,
            basis,
        }
    }
    pub fn key(self) -> Key {
        Key::pod(&self)
    }
}

pub(crate) fn estimate(
    gpu: &Gpu,
    input: &ReadGuard,
    p: Estimate,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    if input.size() != u64::from(p.shape[0]) * u64::from(p.shape[1]) * 12 {
        return Err("Dehaze analysis geometry mismatch".into());
    }
    if p.estimates_color() {
        let radius = p.selection[0];
        let offsets = (radius > EXPANDED_MINIMUM_RADIUS).then(|| minimum_offsets(radius));
        estimate_color(gpu, input, p, offsets.as_deref(), profile)
    } else {
        estimate_brightness(gpu, input, p, profile)
    }
}

fn estimate_brightness(
    gpu: &Gpu,
    input: &ReadGuard,
    p: Estimate,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let uniform = gpu.uniform(&p);
    let mut output = gpu.storage("Dehaze estimate", 32)?;
    let mut histogram = gpu.storage(
        "Dehaze brightness histogram",
        u64::from(HISTOGRAM_BINS) * 3 * 4,
    )?;
    let mut trace = gpu.trace(profile.mode.gpu(), 2);
    gpu.dispatch(
        Kernel::DehazeBrightnessHistogram,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (9, Write(histogram.write())),
        ],
        [
            p.shape[2].div_ceil(HISTOGRAM_BINS),
            p.shape[3].div_ceil(HISTOGRAM_ROWS),
        ],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::DehazeBrightnessEstimate,
        &mut [
            (1, Read(&uniform)),
            (5, Write(output.write())),
            (9, Read(&histogram)),
        ],
        [1, 1],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

fn prepare_analysis_pixels(
    gpu: &Gpu,
    input: &ReadGuard,
    p: Estimate,
    uniform: &ReadGuard,
    trace: &mut GpuTrace,
) -> Result<ReadGuard> {
    let count = u64::from(p.shape[2]) * u64::from(p.shape[3]);
    let mut pixels = gpu.storage("Dehaze analysis pixels", count * 16)?;
    gpu.dispatch(
        Kernel::DehazePrepare,
        &mut [
            (0, Read(input)),
            (1, Read(uniform)),
            (2, Write(pixels.write())),
        ],
        [p.shape[2].div_ceil(8), p.shape[3].div_ceil(8)],
        trace,
    );
    Ok(pixels)
}

fn estimate_color(
    gpu: &Gpu,
    input: &ReadGuard,
    p: Estimate,
    offsets: Option<&[u32]>,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let uniform = gpu.uniform(&p);
    let mut output = gpu.storage("Dehaze estimate", 32)?;
    let minimum_passes = offsets.map_or(2, |steps| steps.len() as u32);
    let mut trace = gpu.trace(profile.mode.gpu(), 3 + minimum_passes);
    let pixels = prepare_analysis_pixels(gpu, input, p, &uniform, &mut trace)?;
    let dark = neighborhood_minimum(gpu, &pixels, p, &uniform, offsets, &mut trace)?;
    {
        let partitions = p.partitions[0];
        let tail_average = p.partitions[1] != 0;
        // Tail bins store a count and a two-word sum for each RGB channel.
        let mut heaps = gpu.storage(
            "haze candidate selection",
            if tail_average {
                u64::from(partitions) * 256 * 32
            } else {
                (u64::from(partitions) + 1) * u64::from(p.selection[2]).max(1) * 16
            },
        )?;
        let mut histograms =
            gpu.storage("haze partial histograms", u64::from(partitions) * 256 * 16)?;
        let mut counts = gpu.storage("haze candidate counts", u64::from(partitions) * 4)?;
        let mut partition_bindings = [
            (1, Read(&uniform)),
            (2, Read(&pixels)),
            (4, Read(&dark)),
            (6, Write(heaps.write())),
            (7, Write(histograms.write())),
            (8, Write(counts.write())),
        ];
        gpu.dispatch(
            if tail_average {
                Kernel::DehazeTailPartition
            } else {
                Kernel::DehazePartition
            },
            &mut partition_bindings[..if tail_average { 5 } else { 6 }],
            [partitions, 1],
            &mut trace,
        );
        let mut merge_bindings = [
            (1, Read(&uniform)),
            (5, Write(output.write())),
            (
                6,
                if tail_average {
                    Read(&heaps)
                } else {
                    Write(heaps.write())
                },
            ),
            (7, Read(&histograms)),
            (2, Read(&pixels)),
            (8, Read(&counts)),
        ];
        gpu.dispatch(
            if tail_average {
                Kernel::DehazeTailMerge
            } else {
                Kernel::DehazeMerge
            },
            &mut merge_bindings[..if tail_average { 4 } else { 6 }],
            [1, 1],
            &mut trace,
        );
    }
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

// A radius-r minimum expanded at offsets {-d, 0, d} covers every sample
// through radius r+d when d <= 2*r+1. The final step overlaps as needed.
fn minimum_offsets(radius: u32) -> Vec<u32> {
    let mut covered = radius.min(1);
    let mut offsets = vec![covered];
    while covered < radius {
        let offset = covered
            .saturating_mul(2)
            .saturating_add(1)
            .min(radius - covered);
        offsets.push(offset);
        covered += offset;
    }
    offsets
}

fn neighborhood_minimum(
    gpu: &Gpu,
    pixels: &ReadGuard,
    p: Estimate,
    uniform: &ReadGuard,
    offsets: Option<&[u32]>,
    trace: &mut GpuTrace,
) -> Result<ReadGuard> {
    let count = u64::from(p.shape[2]) * u64::from(p.shape[3]);
    let groups = [p.shape[2].div_ceil(8), p.shape[3].div_ceil(8)];
    let mut dark = gpu.storage("Dehaze neighborhood minimum", count * 4)?;
    let mut horizontal = gpu.storage("Dehaze horizontal minimum", count * 4)?;
    if let Some(offsets) = offsets {
        let first = gpu.uniform(&[p.shape[2], p.shape[3], offsets[0], 0]);
        gpu.dispatch(
            Kernel::DehazeMinimumPrepare,
            &mut [
                (0, Read(&first)),
                (1, Read(pixels)),
                (3, Write(dark.write())),
            ],
            groups,
            trace,
        );
        for &offset in &offsets[1..] {
            let step = gpu.uniform(&[p.shape[2], p.shape[3], offset, 0]);
            gpu.dispatch(
                Kernel::DehazeMinimumExpand,
                &mut [
                    (0, Read(&step)),
                    (2, Read(&dark)),
                    (3, Write(horizontal.write())),
                ],
                groups,
                trace,
            );
            std::mem::swap(&mut dark, &mut horizontal);
        }
    } else {
        gpu.dispatch(
            Kernel::DehazeHorizontal,
            &mut [
                (1, Read(uniform)),
                (2, Read(pixels)),
                (3, Write(horizontal.write())),
            ],
            groups,
            trace,
        );
        gpu.dispatch(
            Kernel::DehazeVertical,
            &mut [
                (1, Read(uniform)),
                (3, Read(&horizontal)),
                (4, Write(dark.write())),
            ],
            groups,
            trace,
        );
    }
    Ok(dark)
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    analysis: &ReadGuard,
    p: Params,
    extent: [u32; 2],
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let bytes = u64::from(extent[0]) * u64::from(extent[1]) * 12;
    if p.shape[..2] != extent || input.size() != bytes || analysis.size() != 32 {
        return Err("Dehaze correction geometry mismatch".into());
    }
    let uniform = gpu.uniform(&p);
    let mut output = gpu.storage("Dehaze corrected RGB", bytes)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::DehazeApply,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(analysis)),
            (3, Write(output.write())),
        ],
        [extent[0].div_ceil(8), extent[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "dehaze_test.rs"]
mod tests;
