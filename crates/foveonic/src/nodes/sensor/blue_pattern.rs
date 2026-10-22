//! Removes directional blue phase patterns through explicit coarse masks and ordered pair corrections.

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
    // Full blue width/height, processing stage, vertical-pair flag.
    shape: [u32; 4],
    // Correction strength and SensorISO/100 normalization. Remaining words pad.
    controls: [f32; 4],
}
#[derive(Clone, Debug)]
pub struct Prepared {
    params: Params,
}
impl Prepared {
    pub(crate) fn new(extent: [u32; 2], strength: f32, sensor_iso: f32) -> Result<Self> {
        if extent.iter().any(|v| *v < 16 || !v.is_multiple_of(4))
            || !strength.is_finite()
            || !(0. ..=1.).contains(&strength)
            || !sensor_iso.is_finite()
            || sensor_iso <= 0.
        {
            return Err("invalid blue-pattern geometry, strength or sensor ISO".into());
        }
        Ok(Self {
            params: Params {
                shape: [extent[0], extent[1], 0, 0],
                controls: [strength, sensor_iso / 100., 0., 0.],
            },
        })
    }
    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.params.shape[0], self.params.shape[1]]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(&self.params)
    }
}
#[derive(Clone, Copy)]
#[repr(u32)]
enum Stage {
    Normalize = 0,
    Downsample = 1,
    Smooth = 2,
    Contrast = 3,
    DetectContrast = 4,
    ResizeBlockers = 5,
    Field = 6,
    CleanRuns = 7,
    Refine = 8,
    RemoveIsolated = 9,
    Dilate = 10,
    Erode = 11,
    ResizeMask = 12,
    CorrectPairs = 13,
    RestoreUnits = 14,
}

struct Run<'a> {
    gpu: &'a Gpu,
    prepared: &'a Prepared,
    profile: &'a mut Profile,
    dummy: ReadGuard,
}
impl Run<'_> {
    #[allow(clippy::too_many_arguments)]
    fn step(
        &mut self,
        stage: Stage,
        vertical: bool,
        source: &ReadGuard,
        work: Option<&ReadGuard>,
        mask: Option<&ReadGuard>,
        auxiliary: Option<&ReadGuard>,
        shape: [u32; 2],
        floats: u64,
        masks: u64,
    ) -> Result<ReadGuard> {
        let mut params = self.prepared.params;
        params.shape[2] = stage as u32;
        params.shape[3] = u32::from(vertical);
        let uniform = self.gpu.uniform(&params);
        let mut output = self
            .gpu
            .storage("Blue-pattern samples", floats.max(1) * 4)?;
        let mut output_mask = self.gpu.storage("Blue-pattern mask", masks.max(1) * 4)?;
        let mut trace = self.gpu.trace(self.profile.mode.gpu(), 1);
        self.gpu.dispatch(
            Kernel::BluePattern,
            &mut [
                (0, Read(&uniform)),
                (1, Read(source)),
                (2, Read(work.unwrap_or(&self.dummy))),
                (3, Read(mask.unwrap_or(&self.dummy))),
                (4, Read(auxiliary.unwrap_or(&self.dummy))),
                (5, Write(output.write())),
                (6, Write(output_mask.write())),
            ],
            [shape[0].div_ceil(8), shape[1].div_ceil(8)],
            &mut trace,
        );
        self.gpu.finish_trace(&trace, self.profile)?;
        Ok(if masks != 0 { output_mask } else { output })
    }
    fn phase(
        &mut self,
        vertical: bool,
        source: &ReadGuard,
        smooth: &ReadGuard,
        blockers: &ReadGuard,
    ) -> Result<ReadGuard> {
        let [w, h] = self.prepared.extent();
        let n = u64::from(w) * u64::from(h);
        let compact = if vertical { [w, h / 4] } else { [w / 4, h] };
        let field = self.step(
            Stage::Field,
            vertical,
            source,
            None,
            None,
            None,
            compact,
            n / 4,
            0,
        )?;
        let clean = self.step(
            Stage::CleanRuns,
            vertical,
            source,
            Some(&field),
            None,
            None,
            if vertical { compact } else { [w / 4, 1] },
            n / 4,
            0,
        )?;
        let refined = self.step(
            Stage::Refine,
            vertical,
            source,
            Some(&clean),
            Some(blockers),
            None,
            [if vertical { h / 4 } else { w / 4 }, 1],
            0,
            n / 4,
        )?;
        let isolated = self.step(
            Stage::RemoveIsolated,
            vertical,
            source,
            None,
            Some(&refined),
            None,
            compact,
            0,
            n / 4,
        )?;
        let dilated = self.step(
            Stage::Dilate,
            vertical,
            source,
            None,
            Some(&isolated),
            None,
            compact,
            0,
            n / 4,
        )?;
        let closed = if vertical {
            dilated
        } else {
            self.step(
                Stage::Erode,
                false,
                source,
                None,
                Some(&dilated),
                None,
                compact,
                0,
                n / 4,
            )?
        };
        let full = self.step(
            Stage::ResizeMask,
            vertical,
            source,
            None,
            Some(&closed),
            None,
            [w, h],
            0,
            n,
        )?;
        // The native horizontal pass reads the next scratch plane after its
        // last mask row. Preserve those quarter-mask values as an owned input.
        self.step(
            Stage::CorrectPairs,
            vertical,
            source,
            Some(smooth),
            Some(&full),
            Some(&closed),
            [w, h],
            n,
            0,
        )
    }
}
pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    p: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [w, h] = p.extent();
    let n = u64::from(w) * u64::from(h);
    if input.size() != n * 4 {
        return Err("blue-pattern input does not match its extent".into());
    }
    let dummy = gpu.upload("Blue-pattern unused input", &[0u32])?;
    let mut run = Run {
        gpu,
        prepared: p,
        profile,
        dummy,
    };
    let normalized = run.step(
        Stage::Normalize,
        false,
        input,
        None,
        None,
        None,
        [w, h],
        n,
        0,
    )?;
    let half = run.step(
        Stage::Downsample,
        false,
        &normalized,
        None,
        None,
        None,
        [w / 2, h / 2],
        n / 4,
        0,
    )?;
    let smooth = run.step(Stage::Smooth, false, &half, None, None, None, [w, h], n, 0)?;
    let contrast = run.step(
        Stage::Contrast,
        false,
        &normalized,
        Some(&smooth),
        None,
        None,
        [w, h / 4],
        n,
        0,
    )?;
    let coarse = run.step(
        Stage::DetectContrast,
        false,
        &normalized,
        Some(&contrast),
        None,
        None,
        [w / 4 + h / 4, 1],
        0,
        n / 2,
    )?;
    let blockers = run.step(
        Stage::ResizeBlockers,
        false,
        &normalized,
        None,
        Some(&coarse),
        None,
        [w, h],
        0,
        n,
    )?;
    let horizontal = run.phase(false, &normalized, &smooth, &blockers)?;
    let vertical = run.phase(true, &horizontal, &smooth, &blockers)?;
    run.step(
        Stage::RestoreUnits,
        false,
        &vertical,
        None,
        None,
        None,
        [w, h],
        n,
        0,
    )
}
#[cfg(test)]
#[path = "blue_pattern_test.rs"]
mod tests;
