//! Composes blue-speckle repair, channel-ratio correction and neighborhood filtering without changing its source.

use super::{chromatic_ratio, neighborhood_decorrelate, rgb_prefilter};
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    nodes::sensor::blue_despeckle,
    profile::Profile,
    recipe::Key,
    source::Result,
};

pub(crate) struct Prepared {
    extent: [u32; 2],
    despeckle: blue_despeckle::Prepared,
    ratio: chromatic_ratio::Prepared,
    decorrelate: neighborhood_decorrelate::Prepared,
    filter: rgb_prefilter::RangePrepared,
}

impl Prepared {
    pub(crate) fn new(
        extent: [u32; 2],
        despeckle: blue_despeckle::Prepared,
        ratio: chromatic_ratio::Prepared,
        decorrelate: neighborhood_decorrelate::Prepared,
        filter: rgb_prefilter::RangePrepared,
    ) -> Result<Self> {
        if extent.contains(&0)
            || [
                despeckle.extent(),
                ratio.extent(),
                decorrelate.extent(),
                filter.extent(),
            ]
            .iter()
            .any(|v| *v != extent)
        {
            return Err("channel noise stages require matching extents".into());
        }
        Ok(Self {
            extent,
            despeckle,
            ratio,
            decorrelate,
            filter,
        })
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        self.extent
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.extent)
            .join(self.despeckle.key())
            .join(self.ratio.key())
            .join(self.decorrelate.key())
            .join(self.filter.key())
    }
}

pub(crate) struct Output {
    pub(crate) rgb: ReadGuard,
    pub(crate) source_after: ReadGuard,
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    p: &Prepared,
    profile: &mut Profile,
) -> Result<Output> {
    let [w, h] = p.extent;
    let bytes = u64::from(w) * u64::from(h) * 4;
    if input.size() != bytes * 3 {
        return Err("channel noise input geometry mismatch".into());
    }
    let uniform = gpu.uniform(&[w, h, 0, 0]);
    let mut blue = gpu.storage("Extracted blue channel", bytes)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::ChannelNoiseExtract,
        &mut [
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(blue.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    let blue = blue_despeckle::apply(gpu, &blue, &p.despeckle, profile)?;
    let mut replaced = gpu.storage("Nonnegative repaired channels", bytes * 3)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::ChannelNoiseReplace,
        &mut [
            (0, Read(input)),
            (1, Read(&blue)),
            (2, Read(&uniform)),
            (3, Write(replaced.write())),
        ],
        [w.div_ceil(8), h.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    let corrected = chromatic_ratio::apply(gpu, &replaced, &p.ratio, profile)?;
    let corrected = neighborhood_decorrelate::apply(gpu, &corrected, &p.decorrelate, profile)?;
    let rgb = rgb_prefilter::apply_range(gpu, &corrected, &p.filter, profile)?;
    // Later reconstruction needs the original calibrated source. The preceding
    // corrections own fresh storage and leave this shared read claim unchanged.
    Ok(Output {
        rgb,
        source_after: input.clone(),
    })
}
