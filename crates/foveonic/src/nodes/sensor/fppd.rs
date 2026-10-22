//! Corrects sparse RGB gains and paired mono defects with explicit numerical grids.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    // Physical width/height, paired-mono flag, reserved.
    shape: [u32; 4],
    // First corrected x/y, then spacing between corrected x/y sites.
    grid: [u32; 4],
    // Corrected columns/rows, Boolean defect positions, reserved.
    count: [u32; 4],
}

#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
    gains: Vec<f32>,
    defects: Vec<u32>,
}
impl Prepared {
    /// Grid contains origin x/y and spacing x/y. Gains are channel-major.
    /// Mono pairs need eleven readable pixels on each side and twelve below.
    /// Their minimum grid spacing keeps every search neighborhood independent.
    pub(crate) fn new(
        extent: [u32; 2],
        grid: [u32; 4],
        count: [u32; 2],
        gains: Vec<f32>,
        mono: bool,
    ) -> Result<Self> {
        let channels = if mono { 2 } else { 3 };
        let valid = extent.iter().all(|v| *v > 0)
            && count.iter().all(|v| *v > 0)
            && grid[2] >= if mono { 16 } else { 1 }
            && grid[3] >= if mono { 32 } else { 1 }
            && gains.len() == channels * count[0] as usize
            && gains.iter().all(|v| v.is_finite());
        let last = [
            u64::from(grid[0]) + u64::from(count[0].saturating_sub(1)) * u64::from(grid[2]),
            u64::from(grid[1]) + u64::from(count[1].saturating_sub(1)) * u64::from(grid[3]),
        ];
        let border = if mono { 11 } else { 0 };
        if !valid
            || grid[0] < border
            || grid[1] < border
            || last[0] + u64::from(border) >= u64::from(extent[0])
            || last[1] + u64::from(border) + u64::from(mono) >= u64::from(extent[1])
        {
            return Err("invalid sparse phase-correction geometry or gains".into());
        }
        Ok(Self {
            params: Params {
                shape: [extent[0], extent[1], u32::from(mono), 0],
                grid,
                count: [count[0], count[1], 0, 0],
            },
            gains,
            defects: Vec::new(),
        })
    }
    pub(crate) fn with_defects(mut self, positions: Vec<u32>) -> Result<Self> {
        let pixels = u64::from(self.params.shape[0]) * u64::from(self.params.shape[1]);
        if positions.iter().any(|&v| u64::from(v) >= pixels) {
            return Err("phase-correction mask position outside image".into());
        }
        self.params.count[2] = u32::try_from(positions.len())?;
        self.defects = positions;
        Ok(self)
    }
    #[cfg(test)]
    pub(crate) fn gains(&self) -> &[f32] {
        &self.gains
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn mono(&self) -> bool {
        self.params.shape[2] != 0
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params)
            .slice(&self.gains)
            .slice(&self.defects)
    }
}

/// Metadata defect positions initialize an independent Boolean mask on the GPU.
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    p: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let pixels = u64::from(p.params.shape[0]) * u64::from(p.params.shape[1]);
    let components = if p.mono() { 1 } else { 3 };
    if input.size() != pixels * components * 4 {
        return Err("phase-correction input or mask does not match its extent".into());
    }
    let mut output = gpu.storage("Sparse phase correction", input.size())?;
    gpu.copy(input, output.write(), input.size());
    let uniform = gpu.uniform(&p.params);
    let gains = gpu.upload("Sparse phase gains", &p.gains)?;
    let mut mask = gpu.storage(
        "Phase-correction Boolean defects",
        if p.mono() { pixels * 4 } else { 4 },
    )?;
    let mut trace = gpu.trace(profile.mode.gpu(), 2);
    if !p.defects.is_empty() {
        let defects = gpu.upload("Phase-correction defect positions", &p.defects)?;
        gpu.dispatch(
            Kernel::FppdMask,
            &mut [
                (0, Read(&uniform)),
                (1, Read(&defects)),
                (2, Write(mask.write())),
            ],
            [p.params.count[2].div_ceil(64), 1],
            &mut trace,
        );
    }
    gpu.dispatch(
        Kernel::Fppd,
        &mut [
            (0, Read(input)),
            (1, Read(&mask)),
            (2, Read(&uniform)),
            (3, Read(&gains)),
            (4, Write(output.write())),
        ],
        [p.params.count[0].div_ceil(8), p.params.count[1].div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "fppd_test.rs"]
mod tests;
