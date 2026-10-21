//! Repairs signed RGB code values with ordered packed records or a vertically ordered despeckle sweep.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    source::{Metadata, Result},
};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    source: [u32; 4],
    origin: [i32; 4],
}
pub(crate) struct Defects {
    params: Params,
    records: Vec<u32>,
}
impl Defects {
    pub(crate) fn key(&self) -> crate::recipe::Key {
        crate::recipe::Key::pod(&self.params).slice(&self.records)
    }
    pub(crate) fn prepare(metadata: &Metadata) -> Result<Self> {
        let mut records = metadata
            .vector("BadPixels")?
            .iter()
            .map(|v| *v as u32)
            .collect::<Vec<_>>();
        if let Some(badness) = metadata.matrix("BPBadness") {
            if badness.values.len() != records.len() {
                return Err("bad-pixel badness count differs".into());
            }
            let levels = metadata
                .fixed::<2>("BadPixThreshLevels")
                .unwrap_or([128., 30.])
                .map(|v| v as f32);
            let times = metadata
                .fixed::<2>("BadPixThreshTimes")
                .unwrap_or([2., 10.])
                .map(|v| v as f32);
            let shutter = metadata.scalar("CaptureShutter")? as f32 / 1e6;
            let threshold = if shutter <= times[0] {
                levels[0] as u32
            } else if shutter >= times[1] {
                levels[1] as u32
            } else {
                let slope = (levels[1] - levels[0]) / (times[1] - times[0]);
                (shutter.mul_add(slope, levels[0] - times[0] * slope) + 0.5) as u32
            };
            records = records
                .into_iter()
                .zip(&badness.values)
                .filter_map(|(r, b)| ((*b as u32) > threshold).then_some(r))
                .collect();
        }
        let keep = metadata.fixed::<4>("KeepImageArea")?;
        Ok(Self {
            params: Params {
                source: [
                    metadata.width as u32,
                    metadata.height as u32,
                    records.len() as u32,
                    0,
                ],
                origin: [-(keep[0] as i32), -(keep[1] as i32), 0, 0],
            },
            records,
        })
    }
    pub(crate) fn apply(
        &self,
        gpu: &Gpu,
        input: &ReadGuard,
        profile: &mut Profile,
    ) -> Result<ReadGuard> {
        if self.params.source[2] as usize != self.records.len() {
            return Err("ordered signed repair record count does not match its contract".into());
        }
        dispatch(
            gpu,
            input,
            &self.params,
            Some(&self.records),
            Kernel::SignedRepair,
            profile,
        )
    }
}

pub(crate) struct Despeckle {
    params: Params,
}

impl Despeckle {
    pub(crate) fn new(extent: [u32; 2], clamp_bounds: bool) -> Self {
        Self {
            params: Params {
                source: [extent[0], extent[1], 0, u32::from(clamp_bounds)],
                origin: [0; 4],
            },
        }
    }

    pub(crate) fn key(&self) -> crate::recipe::Key {
        crate::recipe::Key::pod(&self.params)
    }

    pub(crate) fn apply(
        &self,
        gpu: &Gpu,
        input: &ReadGuard,
        profile: &mut Profile,
    ) -> Result<ReadGuard> {
        dispatch(
            gpu,
            input,
            &self.params,
            None,
            Kernel::SignedDespeckle,
            profile,
        )
    }
}

fn dispatch(
    gpu: &Gpu,
    input: &ReadGuard,
    params: &Params,
    records: Option<&[u32]>,
    kernel: Kernel,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let bytes = u64::from(params.source[0]) * u64::from(params.source[1]) * 12;
    if bytes == 0 || input.size() < bytes {
        return Err("ordered signed repair input does not match its contract".into());
    }
    let mut output = gpu.storage("Ordered signed RGB repair", bytes)?;
    gpu.copy(input, output.write(), bytes);
    if records.is_none() && (params.source[0] < 3 || params.source[1] < 3) {
        return Ok(output);
    }
    let records = records
        .map(|records| {
            gpu.upload(
                "Packed signed repair records",
                if records.is_empty() { &[0] } else { records },
            )
        })
        .transpose()?;
    // Each batch starts from the completed preceding row. Independent column
    // tiles recompute a 32-column halo to retain exact vertical dependencies.
    let batches = if records.is_some() {
        1
    } else {
        params.source[1].saturating_sub(2).div_ceil(32)
    };
    let mut trace = gpu.trace(profile.mode.gpu(), batches);
    for batch in 0..batches {
        let mut params = *params;
        let groups = if records.is_some() {
            [1, 1]
        } else {
            params.source[2] = 1 + batch * 32;
            [params.source[0].saturating_sub(2).div_ceil(192), 1]
        };
        let uniform = gpu.uniform(&params);
        let mut bindings = vec![
            (0, Read(input)),
            (1, Read(&uniform)),
            (2, Write(output.write())),
        ];
        if let Some(records) = records.as_ref() {
            bindings.push((3, Read(records)));
        }
        gpu.dispatch(kernel, &mut bindings, groups, &mut trace);
    }
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a GPU adapter"]
    fn signed_despeckle_preserves_history_across_tile_and_batch_boundaries() {
        let (width, height) = (401usize, 71usize);
        let input: Vec<f32> = (0..width * height * 3)
            .map(|i| (((i * 7919 + i * i * 31) % 65536) as i32 - 32768) as f32)
            .collect();
        let mut expected = input.clone();
        // The ordered reference uses corrected upper neighbors and untouched
        // horizontal/lower neighbors. This image crosses both tile boundaries.
        for y in 1..height - 1 {
            for x in 1..width - 1 {
                for c in 0..3 {
                    let mut lo = f32::INFINITY;
                    let mut hi = f32::NEG_INFINITY;
                    for (dx, dy) in [
                        (-1, -1),
                        (0, -1),
                        (1, -1),
                        (-1, 0),
                        (1, 0),
                        (-1, 1),
                        (0, 1),
                        (1, 1),
                    ] {
                        let index =
                            (((y as isize + dy) as usize * width) + (x as isize + dx) as usize) * 3
                                + c;
                        let value = if dy < 0 {
                            expected[index]
                        } else {
                            input[index]
                        };
                        lo = lo.min(value);
                        hi = hi.max(value);
                    }
                    let index = (y * width + x) * 3 + c;
                    expected[index] = input[index].clamp(lo, hi);
                }
            }
        }
        let gpu = Gpu::new().unwrap();
        let source = gpu.upload("Tiled signed repair fixture", &input).unwrap();
        let output = Despeckle::new([width as u32, height as u32], true)
            .apply(&gpu, &source, &mut Default::default())
            .unwrap();
        let bytes = gpu.readback(&output).unwrap();
        assert_eq!(bytemuck::cast_slice::<u8, f32>(&bytes), expected);
    }

    #[test]
    #[ignore = "requires a GPU adapter"]
    fn ordered_signed_repairs_match_native_complete_images() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/data/signed-repair-reference.json"
        ))
        .unwrap();
        let gpu = Gpu::new().unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let width = case["width"].as_u64().unwrap() as u32;
            let height = case["height"].as_u64().unwrap() as u32;
            let mode = case["mode"].as_u64().unwrap();
            let input: Vec<f32> = case["input"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_i64().unwrap() as f32)
                .collect();
            let records: Vec<u32> = case["records"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u32)
                .collect();
            let input = gpu.upload("Signed repair fixture", &input).unwrap();
            let output = if mode == 0 {
                Defects {
                    params: Params {
                        source: [width, height, records.len() as u32, 0],
                        origin: [0; 4],
                    },
                    records,
                }
                .apply(&gpu, &input, &mut Default::default())
            } else {
                Despeckle::new([width, height], mode == 2).apply(
                    &gpu,
                    &input,
                    &mut Default::default(),
                )
            }
            .unwrap();
            let bytes = gpu.readback(&output).unwrap();
            let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
            for (i, (a, b)) in actual
                .iter()
                .zip(case["output"].as_array().unwrap())
                .enumerate()
            {
                assert_eq!(
                    *a,
                    b.as_i64().unwrap() as f32,
                    "mode {mode}, {width}x{height}, component {i}"
                );
            }
        }
    }
}
