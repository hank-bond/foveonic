//! Resamples RGB by one half or two using the reference triangular pyramid kernels.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Ratio {
    Half,
    Double,
}

/// Standard-deviation ratio for independent native noise after complete 2D half levels.
///
/// The separable half phase is [1, 3, 3, 1]/8. Composing levels at divisor d
/// gives three convolved length-d boxes; their 1D squared-weight sum is also
/// the 2D standard-deviation ratio. This describes interior samples before
/// edge replication and excludes correlations introduced by native corrections
/// and signed-code rounding. Signal levels and heuristic controls keep their units.
pub(crate) fn independent_noise_std(scale: crate::render::Scale) -> f32 {
    let inverse = 1. / f64::from(scale.divisor());
    let squared = inverse * inverse;
    ((11. + 5. * squared + 4. * squared * squared) * inverse / 20.) as f32
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Prepared {
    // Source and destination physical dimensions.
    shape: [u32; 4],
    // Doubling flag, horizontal/vertical pass, signed-code rounding flag, reserved.
    controls: [u32; 4],
    // Logical sensor origin inside replicated padding.
    origin: [u32; 4],
}

impl Prepared {
    pub(crate) fn new(extent: [u32; 2], ratio: Ratio) -> Result<Self> {
        if extent.contains(&0) || extent.iter().any(|&v| v > i32::MAX as u32 / 2) {
            return Err("invalid RGB pyramid source extent".into());
        }
        let (output, doubling) = match ratio {
            Ratio::Half => (extent.map(|v| v / 2), 0),
            Ratio::Double => return Self::double_to(extent, extent.map(|v| v * 2)),
        };
        if output.contains(&0)
            || u64::from(output[0]) * u64::from(output[1]) > u64::from(u32::MAX) / 3
        {
            return Err("invalid RGB pyramid destination extent".into());
        }
        Ok(Self {
            shape: [extent[0], extent[1], output[0], output[1]],
            controls: [doubling, 0, 0, 0],
            origin: [0; 4],
        })
    }

    /// Extend the covered cells with replicated edges for downstream alignment.
    pub(crate) fn half_to(extent: [u32; 2], output: [u32; 2]) -> Result<Self> {
        if extent.contains(&0)
            || extent.iter().any(|&v| v > i32::MAX as u32 / 2)
            || output.contains(&0)
            || output
                .iter()
                .zip(extent)
                .any(|(&out, input)| out < input.div_ceil(2))
        {
            return Err("invalid covered RGB pyramid extent".into());
        }
        if u64::from(output[0]) * u64::from(output[1]) > u64::from(u32::MAX) / 3 {
            return Err("invalid RGB pyramid destination extent".into());
        }
        Ok(Self {
            shape: [extent[0], extent[1], output[0], output[1]],
            controls: [0, 0, 0, 0],
            origin: [0; 4],
        })
    }

    /// Restore a floor-halved odd grid with the existing replicated-edge phases.
    pub(crate) fn double_to(extent: [u32; 2], output: [u32; 2]) -> Result<Self> {
        if extent.contains(&0) || extent.iter().any(|&v| v > i32::MAX as u32 / 2) {
            return Err("invalid RGB pyramid source extent".into());
        }
        let doubled = extent.map(|v| v * 2);
        if output
            .iter()
            .zip(doubled)
            .any(|(&actual, base)| actual != base && actual != base + 1)
            || u64::from(output[0]) * u64::from(output[1]) > u64::from(u32::MAX) / 3
        {
            return Err("invalid RGB pyramid restored extent".into());
        }
        Ok(Self {
            shape: [extent[0], extent[1], output[0], output[1]],
            controls: [1, 0, 0, 0],
            origin: [0; 4],
        })
    }

    /// Keep Classic signed linear codes integral after each completed 2D level.
    /// Intermediate horizontal sums retain fractional precision.
    pub(crate) fn with_signed_codes(mut self) -> Self {
        self.controls[2] = 1;
        self
    }

    /// Graph-level sensor resampling declares its processing grid explicitly.
    pub(crate) fn with_working_scale(mut self, scale: crate::render::Scale) -> Self {
        self.controls[3] = scale.divisor();
        self
    }
    pub(crate) fn working_scale(&self) -> Option<crate::render::Scale> {
        crate::render::Scale::new(self.controls[3]).ok()
    }
    pub(crate) fn with_origin(mut self, origin: [u32; 2]) -> Self {
        self.origin[..2].copy_from_slice(&origin);
        self
    }
    pub(crate) fn map_view(&self, mut view: crate::image::View) -> crate::image::View {
        if self.controls[0] == 0 {
            let half = crate::render::Scale::new(2).expect("power of two");
            view.crop = half.bounds(view.crop);
            view.extent = half.extent(view.extent);
        } else {
            view.crop = view.crop.map(|v| v * 2);
            view.extent = view.extent.map(|v| v * 2);
        }
        for axis in 0..2 {
            view.crop[axis] += self.origin[axis];
            view.crop[axis + 2] += self.origin[axis];
        }
        view
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.shape[0], self.shape[1]]
    }
    pub(crate) fn output_extent(&self) -> [u32; 2] {
        [self.shape[2], self.shape[3]]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(self)
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height] = prepared.extent();
    let [out_width, out_height] = prepared.output_extent();
    if input.size() != u64::from(width) * u64::from(height) * 12 {
        return Err("RGB pyramid input does not match its extent".into());
    }
    let mut horizontal = gpu.storage(
        "Horizontal RGB pyramid",
        u64::from(out_width) * u64::from(height) * 12,
    )?;
    let mut output = gpu.storage(
        "Resampled RGB pyramid",
        u64::from(out_width) * u64::from(out_height) * 12,
    )?;
    let mut vertical_params = *prepared;
    vertical_params.controls[1] = 1;
    let horizontal_params = gpu.uniform(prepared);
    let vertical_params = gpu.uniform(&vertical_params);
    let mut trace = gpu.trace(profile.mode.gpu(), 2);
    gpu.dispatch(
        Kernel::PyramidResample,
        &mut [
            (0, Read(input)),
            (1, Read(&horizontal_params)),
            (2, Write(horizontal.write())),
        ],
        [out_width.div_ceil(8), height.div_ceil(8)],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::PyramidResample,
        &mut [
            (0, Read(&horizontal)),
            (1, Read(&vertical_params)),
            (2, Write(output.write())),
        ],
        [out_width.div_ceil(8), out_height.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[path = "pyramid_resample_test.rs"]
mod tests;
