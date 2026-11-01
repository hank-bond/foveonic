//! Repairs serial-matched persistent sensor locations before noise filtering and restores clipped defects afterward.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    noise_calibration::SensorDefects,
    recipe::Key,
    source::Result,
};
#[derive(Clone, Debug)]
pub struct Prepared {
    map: SensorDefects,
    clipped_only: bool,
}
impl Prepared {
    pub(crate) fn new(map: &SensorDefects, clipped_only: bool) -> Self {
        Self {
            map: map.clone(),
            clipped_only,
        }
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        self.map.extent
    }
    pub(crate) fn key(&self) -> Key {
        let mut key = Key::pod(&self.map.extent).with(&u32::from(self.clipped_only));
        for p in &self.map.points {
            key = key.with(&[p.pixel, p.channels]);
        }
        key
    }
    pub(crate) fn output(&self) -> &'static str {
        if self.clipped_only {
            "sensor.clipped-defects"
        } else {
            "sensor.calibrated-defects"
        }
    }
}
pub(crate) fn apply(gpu: &Gpu, input: &ReadGuard, prepared: &Prepared) -> Result<ReadGuard> {
    if input.size() != u64::from(prepared.map.extent[0]) * u64::from(prepared.map.extent[1]) * 12 {
        return Err("calibrated defect geometry mismatch".into());
    }
    repair(gpu, input, &prepared.map, prepared.clipped_only)
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    source: [u32; 4],
    area: [u32; 4],
}

fn repair(
    gpu: &Gpu,
    input: &ReadGuard,
    map: &SensorDefects,
    clipped_only: bool,
) -> Result<ReadGuard> {
    if map.points.is_empty() {
        return Ok(input.clone());
    }
    let records: Vec<[u32; 4]> = map
        .points
        .iter()
        .map(|p| [p.pixel, p.channels, 0, 0])
        .collect();
    let records = gpu.upload("Serial-matched sensor defects", &records)?;
    let params = Params {
        source: [
            map.extent[0],
            map.extent[1],
            map.points.len() as u32,
            u32::from(clipped_only),
        ],
        area: [0; 4],
    };
    let uniform = gpu.uniform(&params);
    let mut output = gpu.storage("Calibrated sensor defects", input.size())?;
    gpu.copy(input, output.write(), input.size());
    gpu.dispatch(
        Kernel::CalibratedDefectRepair,
        &mut [
            (0, Read(&uniform)),
            (1, Read(input)),
            (7, Read(&records)),
            (8, Write(output.write())),
        ],
        [(map.points.len() as u32).div_ceil(64), 1],
        &mut gpu.trace(false, 1),
    );
    Ok(output)
}
