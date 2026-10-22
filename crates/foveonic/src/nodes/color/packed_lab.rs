//! Converts unsigned RGB code triples to packed Lab codes and back with supplied matrices.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    Forward,
    Inverse,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Prepared {
    shape: [u32; 4],
    matrix: [[f32; 4]; 6],
}

impl Prepared {
    pub(crate) fn new(extent: [u32; 2], direction: Direction, matrix: [f64; 9]) -> Result<Self> {
        if extent.contains(&0) || matrix.iter().any(|v| !v.is_finite()) {
            return Err("invalid packed Lab conversion controls".into());
        }
        let mut split = [[0.; 4]; 6];
        for row in 0..3 {
            for column in 0..3 {
                let value = matrix[row * 3 + column];
                split[row * 2][column] = value as f32;
                split[row * 2 + 1][column] = (value - f64::from(value as f32)) as f32;
            }
        }
        Ok(Self {
            shape: [
                extent[0],
                extent[1],
                u32::from(matches!(direction, Direction::Inverse)),
                0,
            ],
            matrix: split,
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.shape[0], self.shape[1]]
    }
    pub(crate) fn direction(&self) -> Direction {
        if self.shape[2] == 0 {
            Direction::Forward
        } else {
            Direction::Inverse
        }
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(self)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    source: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height, _, _] = prepared.shape;
    if source.size() != u64::from(width) * u64::from(height) * 12 {
        return Err("packed Lab conversion extent mismatch".into());
    }
    let uniform = gpu.uniform(prepared);
    let mut output = gpu.storage("packed Lab conversion", source.size())?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::PackedLab,
        &mut [
            (0, Read(source)),
            (1, Read(&uniform)),
            (2, Write(output.write())),
        ],
        [width.div_ceil(8), height.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "packed_lab_test.rs"]
mod tests;
