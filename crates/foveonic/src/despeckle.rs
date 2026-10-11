//! Prepares reference speckle thresholds and runs deterministic detection and ordered chroma-anchored correction on GPU.
//!
//! Tables and detector rules adapted from the owner's read-only Foveonic reference
//! `despeckle.rs` and `repair.rs`, audited at e47af35 (including permitted unstaged changes).

use crate::{
    gpu::{Gpu, Kernel},
    profile::Profile,
    source::{MatrixEncoding, Metadata, Result},
};

const BANKS: u32 = 8;
const BLOCK: u32 = 256;
const CAPACITY: u32 = 624_999;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Params {
    pub source: [u32; 4], // width, height, block count, records per bank
    pub area: [u32; 4],   // inclusive detector bounds in full-sensor coordinates
    pub gain: [f32; 4],   // global Sensor gain; detection uses its unscaled signal
}

pub(crate) struct Prepared {
    pub params: Params,
    pub tables: Vec<f32>,
}

fn parameters(width: u32, height: u32, active: [u32; 4], gain: f32) -> Result<Params> {
    let count = width
        .checked_mul(height)
        .ok_or("despeckle dimensions overflow")?;
    if width < 16
        || height < 16
        || !height.is_multiple_of(BANKS)
        || !(count / BANKS).is_multiple_of(BLOCK)
        || active[0] < 5
        || active[1] < 6
        || active[0] >= width / 2
        || active[1] >= height / 2
        || !gain.is_finite()
        || gain < 0.
    {
        return Err("unsupported despeckle geometry or Sensor gain".into());
    }
    Ok(Params {
        source: [width, height, count / BLOCK, CAPACITY.min(count / BANKS)],
        area: [
            active[0] - 4,
            active[1] - 4,
            width - active[0] + 2,
            height - (active[1] - 4),
        ],
        gain: [gain, 0., 0., 0.],
    })
}

fn tables(gains: [f64; 3], adjustment: [f64; 3]) -> Result<Vec<f32>> {
    if gains.iter().any(|v| !v.is_finite() || *v <= 0.)
        || adjustment.iter().any(|v| !v.is_finite() || *v < 0.)
    {
        return Err("invalid speckle table inputs".into());
    }
    let constants = [0x3d08c5117a7e165e, 0x3d00e374a4f8e0b4, 0x3cecd2b297d889bc];
    let floors = [0x3ea5798ee0000000, 0x3eab2dd8e0000000, 0x3e65798ee0000000];
    let mut result = Vec::with_capacity(3 * 4096);
    for c in 0..3 {
        let scale = (f64::from_bits(constants[c]) / gains[c]) as f32;
        for i in 0..4096 {
            let signal = ((i as f32 as f64 * f64::from_bits(0x3f36666666666666)) * scale as f64
                / f64::from_bits(0x3c048ad3caefe280)) as f32;
            let converted =
                (signal.sqrt() as f64 * f64::from_bits(0x3c079ca10c924223) / scale as f64) as f32;
            let product = converted as f64 * f64::from_bits(0x3fe83886594af4f1);
            let noise = (converted as f64)
                .mul_add(product, f64::from_bits(floors[c]))
                .sqrt() as f32;
            let scaled = (noise * adjustment[c] as f32) * 4096.;
            result.push((scaled as f64 / f64::from_bits(0x3ff6666666666666)) as f32);
        }
    }
    if result.iter().any(|v| !v.is_finite()) {
        return Err("nonfinite speckle threshold table".into());
    }
    Ok(result)
}

pub(crate) fn prepare(metadata: &Metadata, wb: &str, gain: f32) -> Result<Prepared> {
    let key = metadata
        .lists
        .get("WhiteBalanceGains")
        .and_then(|l| l.get(wb))
        .ok_or("missing despeckle white balance")?;
    let raw = metadata.fixed::<3>(key)?;
    let optional = |key| {
        if metadata.matrix(key).is_some() {
            metadata.fixed::<3>(key)
        } else {
            Ok([1.; 3])
        }
    };
    let aperture = optional("FnumberGainFact")?;
    let mut temperature = optional("TempGainFact")?;
    if temperature.iter().any(|v| !(0.5..=1.5).contains(v)) {
        temperature = [1.; 3];
    }
    let sensor = optional("SensorAdjustmentGainFact")?;
    // Unscaled WB inputs, as in the native diagnostic preparation. Calibration
    // applies its separate factor of two. Threshold scale is the reference 1x;
    // this independent correction is not driven by reconstruction presets.
    let gains = std::array::from_fn(|c| ((raw[c] * aperture[c]) * temperature[c]) * sensor[c]);
    let mut adjustment = [15., 15., 5.];
    if let Some(m) = metadata
        .matrix("DespAdjust")
        .filter(|m| m.encoding == MatrixEncoding::Float32 && m.dimensions == [3])
    {
        if m.values.len() != 3 {
            return Err("invalid float despeckle adjustment".into());
        }
        adjustment = std::array::from_fn(|c| m.values[c] as f32 as f64);
    }
    let active = crate::calibration::rectangle(metadata, "ActiveImageArea")?;
    let keep = crate::calibration::rectangle(metadata, "KeepImageArea")?;
    if keep != [0, 0, metadata.width as u32, metadata.height as u32] {
        return Err("despeckle requires full-sensor keep geometry".into());
    }
    Ok(Prepared {
        params: parameters(metadata.width as u32, metadata.height as u32, active, gain)?,
        tables: tables(gains, adjustment)?,
    })
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &wgpu::Buffer,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<wgpu::Buffer> {
    let p = prepared.params;
    let count = u64::from(p.source[0]) * u64::from(p.source[1]);
    let output = gpu.storage("Despeckled sensor layers", count * 12)?;
    let marks = gpu.storage("Speckle block ranks", count * 4)?;
    let counts = gpu.storage(
        "Speckle block prefixes and bank counts",
        u64::from(p.source[2] + BANKS) * 4,
    )?;
    let records = gpu.storage(
        "Ordered speckle records",
        u64::from(p.source[3]) * u64::from(BANKS) * 8,
    )?;
    let uniform = gpu.uniform(&p);
    let lookup = gpu.upload("Speckle threshold tables", &prepared.tables)?;
    let mut trace = gpu.trace(profile.enabled, 4);
    gpu.dispatch(
        Kernel::SpeckleDetect,
        &[
            (0, input),
            (1, &lookup),
            (2, &uniform),
            (3, &output),
            (4, &marks),
            (5, &counts),
        ],
        [p.source[2], 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::SpecklePrefix,
        &[(2, &uniform), (5, &counts)],
        [1, 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::SpeckleScatter,
        &[(2, &uniform), (4, &marks), (5, &counts), (6, &records)],
        [p.source[2], 1],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::SpeckleRepair,
        &[(2, &uniform), (3, &output), (5, &counts), (6, &records)],
        [1, 1],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "despeckle_test.rs"]
mod tests;
