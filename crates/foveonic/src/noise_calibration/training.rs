//! Measures one cap group at a time on the GPU before fitting shared noise and serial-specific defects.

use super::{
    fit::{Fitter, FrameProfiles},
    groups, Calibration, Capture, Condition, Defect, SensorDefects, TrainingProgress,
    FORMAT_VERSION, MINIMUM_CAPTURES,
};
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    nodes::sensor::{calibration, noise_model, shield_noise},
    source::{Result, Source},
};
use std::path::PathBuf;
const MAXIMUM_GROUP_DEFECTS: u32 = 65536;
const MAXIMUM_CAP_MEAN: f64 = 64.;
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    source: [u32; 4],
    active: [u32; 4],
}

fn floats(bytes: Vec<u8>) -> Vec<f32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| f32::from_le_bytes(*v))
        .collect()
}
fn black_levels(gpu: &Gpu, original: &ReadGuard, source: &Source) -> Result<ReadGuard> {
    let params = calibration::black(&source.metadata)?;
    let uniform = gpu.uniform(&params);
    let mut histogram = gpu.storage("Cap black histogram", 3 * 4096 * 4)?;
    let mut black = gpu.storage("Cap black levels", 12)?;
    let mut trace = gpu.trace(false, 2);
    gpu.dispatch(
        Kernel::Histogram,
        &mut [
            (0, Read(original)),
            (1, Write(histogram.write())),
            (2, Read(&uniform)),
        ],
        [params.source[2].div_ceil(64), 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::BlackMean,
        &mut [
            (1, Read(&histogram)),
            (2, Read(&uniform)),
            (3, Write(black.write())),
        ],
        [3, 1],
        &mut trace,
    );
    Ok(black)
}
fn measure_profiles(
    gpu: &Gpu,
    original: &ReadGuard,
    black: &ReadGuard,
    params: Params,
    regions: shield_noise::Regions,
) -> Result<FrameProfiles> {
    let [right, bottom] = shield_noise::training_profiles(
        gpu,
        original,
        params.source[..2].try_into().unwrap(),
        regions,
    )?;
    let uniform = gpu.uniform(&params);
    let mut profiles = gpu.storage(
        "Active cap profiles",
        2 * u64::from(params.source[3]) * 3 * 4,
    )?;
    gpu.dispatch(
        Kernel::CapProfiles,
        &mut [
            (0, Read(&uniform)),
            (1, Read(original)),
            (4, Read(black)),
            (5, Write(profiles.write())),
        ],
        [params.source[3], 2],
        &mut gpu.trace(false, 1),
    );
    let values = floats(gpu.readback(&profiles)?);
    let profiles: [Vec<[f64; 3]>; 2] = std::array::from_fn(|axis| {
        let n = if axis == 0 {
            params.active[3] - params.active[1]
        } else {
            params.active[2] - params.active[0]
        };
        (0..n as usize)
            .map(|i| {
                std::array::from_fn(|c| {
                    f64::from(values[(axis * params.source[3] as usize + i) * 3 + c])
                })
            })
            .collect()
    });
    let [active_rows, active_columns] = profiles;
    let means: [f64; 3] = std::array::from_fn(|c| {
        active_rows.iter().map(|v| v[c]).sum::<f64>() / active_rows.len() as f64
    });
    if means.iter().any(|v| v.abs() > MAXIMUM_CAP_MEAN) {
        return Err(format!("Capture appears illuminated (mean native signal {means:.2?}). Select lens-cap captures only.").into());
    }
    let black = floats(gpu.readback(black)?);
    Ok(FrameProfiles {
        active_rows,
        active_columns,
        right,
        bottom,
        black: std::array::from_fn(|c| f64::from(black[c])),
    })
}

struct GroupMeasurements {
    mean: ReadGuard,
    m2: ReadGuard,
    params: Params,
    frames: Vec<FrameProfiles>,
}
fn measure_group(
    gpu: &Gpu,
    condition: &Condition,
    paths: &[PathBuf],
    progress: &mut impl FnMut(TrainingProgress),
    cancelled: &impl Fn() -> bool,
) -> Result<GroupMeasurements> {
    let mut frames = Vec::new();
    let mut running: Option<(ReadGuard, ReadGuard, Params)> = None;
    for (index, path) in paths.iter().enumerate() {
        if cancelled() {
            return Err("Calibration training cancelled".into());
        }
        progress(TrainingProgress {
            condition: condition.clone(),
            completed: index,
            total: paths.len(),
        });
        let source = Source::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if Condition::from_properties(&source.metadata.properties)? != *condition {
            return Err(format!("Capture identity or exposure changed: {}", path.display()).into());
        }
        let extent = [source.metadata.width as u32, source.metadata.height as u32];
        let regions = shield_noise::Regions::from_metadata(&source.metadata)?;
        let active = regions.active;
        let bytes = u64::from(extent[0]) * u64::from(extent[1]) * 12;
        let (mean, m2, params) = if let Some(value) = &mut running {
            value
        } else {
            running = Some((
                gpu.storage("Cap running mean", bytes)?,
                gpu.storage("Cap running variance", bytes)?,
                Params {
                    source: [
                        extent[0],
                        extent[1],
                        0,
                        (active[2] - active[0]).max(active[3] - active[1]),
                    ],
                    active,
                },
            ));
            running.as_mut().unwrap()
        };
        if params.source[..2] != extent || params.active != active {
            return Err("Cap group has inconsistent sensor geometry".into());
        }
        params.source[2] = index as u32 + 1;
        let original = gpu.upload("Original cap layers", source.samples())?;
        let black = black_levels(gpu, &original, &source)?;
        frames.push(
            measure_profiles(gpu, &original, &black, *params, regions)
                .map_err(|e| format!("{}: {e}", path.display()))?,
        );
        let uniform = gpu.uniform(params);
        gpu.dispatch(
            Kernel::CapAccumulate,
            &mut [
                (0, Read(&uniform)),
                (1, Read(&original)),
                (2, Write(mean.write())),
                (3, Write(m2.write())),
                (4, Read(&black)),
            ],
            [extent[0].div_ceil(8), extent[1].div_ceil(8)],
            &mut gpu.trace(false, 1),
        );
        gpu.device.poll(wgpu::Maintain::Wait);
    }
    if cancelled() {
        return Err("Calibration training cancelled".into());
    }
    let (mean, m2, params) = running.expect("nonempty validated cap group");
    Ok(GroupMeasurements {
        mean,
        m2,
        params,
        frames,
    })
}
fn detect_defects(
    gpu: &Gpu,
    condition: &Condition,
    measurements: &GroupMeasurements,
) -> Result<Option<SensorDefects>> {
    let Some(serial) = &condition.serial else {
        return Ok(None);
    };
    let params = &measurements.params;
    let uniform = gpu.uniform(params);
    let mut count = gpu.storage("Persistent cap defect count", 4)?;
    let mut records = gpu.storage(
        "Persistent cap defects",
        u64::from(MAXIMUM_GROUP_DEFECTS) * 16,
    )?;
    gpu.dispatch(
        Kernel::CapDefects,
        &mut [
            (0, Read(&uniform)),
            (2, Read(&measurements.mean)),
            (3, Read(&measurements.m2)),
            (6, Write(count.write())),
            (7, Write(records.write())),
        ],
        [params.source[0].div_ceil(8), params.source[1].div_ceil(8)],
        &mut gpu.trace(false, 1),
    );
    let n = u32::from_le_bytes(gpu.readback(&count)?[..4].try_into().unwrap());
    if n > MAXIMUM_GROUP_DEFECTS {
        return Err(format!("Too many persistent defect candidates ({n}) for {} ISO {}. Check that all inputs are lens-cap captures.",condition.camera_model,condition.iso).into());
    }
    let bytes = gpu.readback(&records)?;
    let mut points: Vec<_> = bytes.as_chunks::<16>().0[..n as usize]
        .iter()
        .map(|r| Defect {
            pixel: u32::from_le_bytes(r[..4].try_into().unwrap()),
            channels: u32::from_le_bytes(r[4..8].try_into().unwrap()),
        })
        .collect();
    points.sort_by_key(|p| p.pixel);
    Ok(Some(SensorDefects {
        camera_model: condition.camera_model.clone(),
        serial: serial.clone(),
        extent: params.source[..2].try_into().unwrap(),
        points,
    }))
}

pub(crate) fn train(
    gpu: &Gpu,
    captures: &[Capture],
    mut progress: impl FnMut(TrainingProgress),
    cancelled: impl Fn() -> bool,
) -> Result<Calibration> {
    let groups = groups(captures);
    if groups.is_empty() || groups.values().any(|v| v.len() < MINIMUM_CAPTURES) {
        return Err("Add at least seven caps for every camera/ISO/shutter combination".into());
    }
    let model = noise_model::embedded()?.clone();
    let mut fitter = Fitter::new(&model);
    let mut defects = Vec::new();
    for (condition, paths) in groups {
        let measurements = measure_group(gpu, &condition, &paths, &mut progress, &cancelled)?;
        if let Some(map) = detect_defects(gpu, &condition, &measurements)? {
            defects.push(map);
        }
        fitter.observe(&condition, &measurements.frames)?;
        progress(TrainingProgress {
            condition,
            completed: paths.len(),
            total: paths.len(),
        });
    }
    let result = Calibration {
        version: FORMAT_VERSION,
        signature: Default::default(),
        captures: captures.to_vec(),
        defects: super::merge_defects(defects),
        model: fitter.finish(model)?,
    };
    result.validate()?;
    result.signature();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{nodes::sensor::calibrated_defects, noise_calibration::SensorDefects};

    #[test]
    #[ignore = "requires a GPU"]
    fn cap_moments_distinguish_persistent_defects_from_one_frame_outliers() {
        let gpu = Gpu::new().unwrap();
        let extent = [40u32, 32u32];
        let length = (extent[0] * extent[1]) as usize;
        let bytes = length as u64 * 12;
        let black = gpu.upload("Known cap black", &[10f32, 20., 30.]).unwrap();
        let mut mean = gpu.storage("Test mean", bytes).unwrap();
        let mut m2 = gpu.storage("Test M2", bytes).unwrap();
        let persistent = 13 * extent[0] + 12;
        let neighbor = persistent + 1;
        let transient = 20 * extent[0] + 25;
        let clipped = 8 * extent[0] + 6;
        let mut params = Params {
            source: [extent[0], extent[1], 0, extent[0]],
            active: [0, 0, extent[0], extent[1]],
        };
        let mut last = Vec::new();
        for i in 0..7 {
            let mut samples = vec![[10f32, 20., 30.]; length];
            // A known zero-mean temporal sequence gives M2=28 in each layer.
            for value in &mut samples {
                for v in value {
                    *v += i as f32 - 3.;
                }
            }
            samples[persistent as usize][1] += 40.;
            samples[neighbor as usize][1] += 60.;
            samples[clipped as usize][0] = 4095.;
            if i == 0 {
                samples[transient as usize][2] += 350.;
            }
            params.source[2] = i + 1;
            let original = gpu.upload("Test cap", &samples).unwrap();
            let uniform = gpu.uniform(&params);
            gpu.dispatch(
                Kernel::CapAccumulate,
                &mut [
                    (0, Read(&uniform)),
                    (1, Read(&original)),
                    (2, Write(mean.write())),
                    (3, Write(m2.write())),
                    (4, Read(&black)),
                ],
                [extent[0].div_ceil(8), extent[1].div_ceil(8)],
                &mut gpu.trace(false, 1),
            );
            last = samples;
        }
        let average = floats(gpu.readback(&mean).unwrap());
        let variance = floats(gpu.readback(&m2).unwrap());
        assert!(average[0..3].iter().all(|v| v.abs() < 1e-5));
        assert!(variance[0..3].iter().all(|v| (*v - 28.).abs() < 1e-4));
        assert_eq!(average[persistent as usize * 3 + 1], 40.);
        let uniform = gpu.uniform(&params);
        let original = gpu.upload("Last cap", &last).unwrap();
        let mut profiles = gpu
            .storage("Test line profiles", u64::from(extent[0]) * 2 * 12)
            .unwrap();
        gpu.dispatch(
            Kernel::CapProfiles,
            &mut [
                (0, Read(&uniform)),
                (1, Read(&original)),
                (4, Read(&black)),
                (5, Write(profiles.write())),
            ],
            [extent[0], 2],
            &mut gpu.trace(false, 1),
        );
        let lines = floats(gpu.readback(&profiles).unwrap());
        assert!((lines[13 * 3 + 1] - (3. + 100. / 40.)).abs() < 1e-5);
        assert!((lines[(40 + 12) * 3 + 1] - (3. + 40. / 32.)).abs() < 1e-5);
        let mut count = gpu.storage("Test defect count", 4).unwrap();
        let mut records = gpu
            .storage("Test records", u64::from(MAXIMUM_GROUP_DEFECTS) * 16)
            .unwrap();
        gpu.dispatch(
            Kernel::CapDefects,
            &mut [
                (0, Read(&uniform)),
                (2, Read(&mean)),
                (3, Read(&m2)),
                (6, Write(count.write())),
                (7, Write(records.write())),
            ],
            [extent[0].div_ceil(8), extent[1].div_ceil(8)],
            &mut gpu.trace(false, 1),
        );
        let n = u32::from_le_bytes(gpu.readback(&count).unwrap()[..4].try_into().unwrap());
        assert_eq!(n, 3);
        let raw = gpu.readback(&records).unwrap();
        let mut points: Vec<_> = raw.as_chunks::<16>().0[..n as usize]
            .iter()
            .map(|v| Defect {
                pixel: u32::from_le_bytes(v[..4].try_into().unwrap()),
                channels: u32::from_le_bytes(v[4..8].try_into().unwrap()),
            })
            .collect();
        points.sort_by_key(|p| p.pixel);
        assert_eq!(
            points
                .iter()
                .map(|p| (p.pixel, p.channels))
                .collect::<Vec<_>>(),
            vec![(clipped, 1), (persistent, 2), (neighbor, 2)]
        );
        let map = SensorDefects {
            camera_model: "SIGMA DP3 Merrill".into(),
            serial: "camera-one".into(),
            extent,
            points,
        };
        let fixed = calibrated_defects::apply(
            &gpu,
            &original,
            &calibrated_defects::Prepared::new(&map, false),
        )
        .unwrap();
        let result = floats(gpu.readback(&fixed).unwrap());
        for (i, value) in result.iter().enumerate() {
            let pixel = (i / 3) as u32;
            let c = i % 3;
            if ((pixel == persistent || pixel == neighbor) && c == 1)
                || (pixel == clipped && c == 0)
            {
                assert_eq!(*value, [13., 23., 33.][c]);
            } else {
                assert_eq!(*value, last[i / 3][c]);
            }
        }
        // Locations that are quiet in this exposure must not be interpolated.
        let quiet: Vec<[f32; 3]> = (0..length)
            .map(|i| {
                let ramp = (i % extent[0] as usize) as f32;
                [ramp, ramp * 2., ramp * 3.]
            })
            .collect();
        let quiet_input = gpu.upload("Quiet learned locations", &quiet).unwrap();
        let quiet_fixed = calibrated_defects::apply(
            &gpu,
            &quiet_input,
            &calibrated_defects::Prepared::new(&map, false),
        )
        .unwrap();
        assert_eq!(
            gpu.readback(&quiet_fixed).unwrap(),
            bytemuck::cast_slice::<_, u8>(&quiet)
        );
        // The immutable input is retained, and the final clipping pass touches only clipped layers.
        assert_eq!(
            floats(gpu.readback(&original).unwrap())[persistent as usize * 3 + 1],
            63.
        );
        let clipped_fixed = calibrated_defects::apply(
            &gpu,
            &original,
            &calibrated_defects::Prepared::new(&map, true),
        )
        .unwrap();
        let result = floats(gpu.readback(&clipped_fixed).unwrap());
        assert_eq!(result[clipped as usize * 3], 13.);
        assert_eq!(result[persistent as usize * 3 + 1], 63.);
    }
}
