//! Mixes integral RGB codes with normalized channel weights or copies selected channels.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

#[derive(Clone, Debug)]
pub(crate) struct Prepared {
    shape: [u32; 4],
    coefficients: [[f32; 2]; 9],
}
impl Prepared {
    pub(crate) fn mix(extent: [u32; 2], mut weights: [f64; 3]) -> Result<Self> {
        if weights.iter().any(|v| !v.is_finite()) {
            return Err("invalid channel mixing weights".into());
        }
        let sum = weights.iter().sum::<f64>();
        if sum == 0. {
            weights = [1. / 3.; 3];
        } else if sum as f32 != 1. {
            weights = weights.map(|v| v / sum);
        }
        let mut prepared = Self::matrix(
            extent,
            [weights, weights, weights].concat().try_into().unwrap(),
            true,
        )?;
        // An exact integer mean avoids a downward bias at integer ties from
        // the paired Float32 approximation of one third. Native Double sums
        // can still differ by one code at those ties.
        prepared.shape[3] = u32::from(weights == [1. / 3.; 3]);
        Ok(prepared)
    }

    pub(crate) fn copy_red_to_blue(extent: [u32; 2]) -> Result<Self> {
        Self::matrix(extent, [1., 0., 0., 0., 1., 0., 1., 0., 0.], false)
    }

    fn matrix(extent: [u32; 2], matrix: [f64; 9], replicate: bool) -> Result<Self> {
        if extent.contains(&0)
            || u64::from(extent[0]) * u64::from(extent[1]) > u64::from(u32::MAX) / 3
            || matrix
                .iter()
                .any(|v| !v.is_finite() || !(*v as f32).is_finite())
        {
            return Err("invalid integral channel mapping".into());
        }
        Ok(Self {
            shape: [extent[0], extent[1], u32::from(replicate), 0],
            coefficients: matrix.map(|v| {
                let high = v as f32;
                [high, (v - f64::from(high)) as f32]
            }),
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.shape[0], self.shape[1]]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.shape).slice(&self.coefficients)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    p: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [w, h] = p.extent();
    if input.size() != u64::from(w) * u64::from(h) * 12 {
        return Err("channel mapping input geometry mismatch".into());
    }
    let uniform = gpu.uniform(&p.shape);
    let coefficients = gpu.upload("Integral channel mixing coefficients", &p.coefficients)?;
    let mut output = gpu.storage("Mapped integral RGB channels", input.size())?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::CodeChannelMap,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Read(&coefficients)),
            (3, Write(output.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "code_channel_map_test.rs"]
mod tests;
