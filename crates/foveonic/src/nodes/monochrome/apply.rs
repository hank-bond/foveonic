//! Dispatches monochrome nodes with separate immutable inputs and explicit guarded output storage.

use super::{Prepared, Stage};
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    source::Result,
};

pub(crate) fn apply(
    gpu: &Gpu,
    stage: Stage,
    inputs: &[&ReadGuard],
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let p = prepared.params;
    let [width, height, _, strips] = p.source;
    let components = if matches!(stage, Stage::Expand) { 3 } else { 1 };
    let mut output = gpu.storage(
        "Monochrome node output",
        u64::from(width) * u64::from(height) * components * 4,
    )?;
    if matches!(stage, Stage::Histogram) {
        let size = output.size();
        gpu.copy(inputs[1], output.write(), size);
    }
    let uniform = gpu.uniform(&p);
    let table = if matches!(stage, Stage::Directional | Stage::Histogram) {
        let (indexes, weights) = prepared.table(matches!(stage, Stage::Directional));
        Some((
            gpu.upload("Mono range rows", &indexes)?,
            gpu.upload("Mono range coefficients", &weights)?,
        ))
    } else {
        None
    };
    let mut histogram = if matches!(stage, Stage::Histogram) {
        // One count/sum pair per bin and interior column. The kernel reuses
        // this scratch across fixed strips instead of reserving eight copies.
        let bytes = u64::from(width - 18) * 1024 * 8;
        Some(gpu.storage("Mono histogram bins", bytes)?)
    } else {
        None
    };
    let kernel = match stage {
        Stage::Mix => Kernel::MonoMix,
        Stage::Directional => Kernel::MonoDirectional,
        Stage::Column => Kernel::MonoColumn,
        Stage::Histogram => Kernel::MonoHistogram,
        Stage::Despeckle => Kernel::MonoDespeckle,
        Stage::Signal => Kernel::MonoSignal,
        Stage::Expand => Kernel::MonoExpand,
    };
    let mut bindings = vec![
        (0, Read(inputs[0])),
        (2, Read(&uniform)),
        (3, Write(output.write())),
    ];
    if matches!(stage, Stage::Column | Stage::Despeckle) {
        bindings.push((1, Read(inputs[1])));
    }
    if let Some((indexes, weights)) = &table {
        bindings.extend([(4, Read(indexes)), (5, Read(weights))]);
    }
    if let Some(bins) = &mut histogram {
        bindings.push((6, Write(bins.write())));
    }
    let groups = match stage {
        Stage::Column => [width.div_ceil(64), strips * ((height / strips - 1) / 40)],
        Stage::Histogram => [(width - 18).div_ceil(64), 1],
        _ => [width.div_ceil(8), height.div_ceil(8)],
    };
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(kernel, &mut bindings, groups, &mut trace);
    drop(bindings);
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}
