//! Maps unsigned integral RGB codes through supplied linear photographic response tables.

use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    recipe::Key,
    source::Result,
};

const TABLE_LENGTH: usize = 65536;

#[derive(Clone, Debug)]
pub(crate) struct Prepared {
    tables: Vec<f32>,
}

impl Prepared {
    /// Three channel tables. Entries already express linear output components.
    pub(crate) fn new(tables: Vec<f32>) -> Result<Self> {
        if tables.len() != 3 * TABLE_LENGTH || tables.iter().any(|v| !v.is_finite()) {
            return Err("unsigned code response requires three finite 65536-entry tables".into());
        }
        Ok(Self { tables })
    }

    pub(crate) fn key(&self) -> Key {
        Key::default().slice(&self.tables)
    }

    /// Evaluate the same clamped integral-code lookup used by the GPU. Profile
    /// fitting samples this response between stored image coordinates, but does
    /// not smooth over its recovered integer lookup boundaries.
    pub(crate) fn evaluate_codes(&self, codes: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|channel| {
            let code = codes[channel].clamp(0., (TABLE_LENGTH - 1) as f64) as usize;
            f64::from(self.tables[channel * TABLE_LENGTH + code])
        })
    }

    #[cfg(test)]
    pub(crate) fn evaluate(&self, codes: [u16; 3]) -> [f32; 3] {
        std::array::from_fn(|c| self.tables[c * TABLE_LENGTH + usize::from(codes[c])])
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    source: &ReadGuard,
    extent: [u32; 2],
    prepared: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    let [width, height] = extent;
    if extent.contains(&0) || source.size() != u64::from(width) * u64::from(height) * 12 {
        return Err("unsigned code response input does not match its extent".into());
    }
    let tables = gpu.upload("linear code response tables", &prepared.tables)?;
    let uniform = gpu.uniform(&[width, height, 0, 0]);
    let mut output = gpu.storage("linear photographic response", source.size())?;
    let mut trace = gpu.trace(profile.mode.gpu(), 1);
    gpu.dispatch(
        Kernel::CodeResponse,
        &mut [
            (0, Read(source)),
            (1, Read(&tables)),
            (2, Read(&uniform)),
            (3, Write(output.write())),
        ],
        [width.div_ceil(8), height.div_ceil(8)],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_complete_finite_channels() {
        assert!(Prepared::new(vec![0.; TABLE_LENGTH]).is_err());
        let mut tables = vec![0.; 3 * TABLE_LENGTH];
        tables[TABLE_LENGTH] = f32::NAN;
        assert!(Prepared::new(tables).is_err());
    }

    #[test]
    #[ignore = "requires a GPU"]
    fn all_codes_select_their_channel_table_without_mutating_input() {
        let gpu = Gpu::new().unwrap();
        let tables: Vec<f32> = (0..3 * TABLE_LENGTH).map(|v| v as f32 / 65536.).collect();
        let prepared = Prepared::new(tables).unwrap();
        let values: Vec<f32> = (0..TABLE_LENGTH).flat_map(|v| [v as f32; 3]).collect();
        let input = gpu.upload("unsigned RGB test", &values).unwrap();
        let output = apply(&gpu, &input, [256, 256], &prepared, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual: &[f32] = bytemuck::cast_slice(&bytes);
        for code in 0..TABLE_LENGTH {
            assert_eq!(
                &actual[code * 3..code * 3 + 3],
                &prepared.evaluate([code as u16; 3])
            );
        }
        assert!(gpu.readback(&input).unwrap() == bytemuck::cast_slice::<f32, u8>(&values));
    }
}
