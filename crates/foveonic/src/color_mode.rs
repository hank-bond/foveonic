//! Resolves camera color modes and their camera-ordered tone records.
//!
//! Adapted from Foveonic's color-mode preparation and matrix composition.

use crate::source::{Metadata, Result};

pub struct Settings {
    pub tone_index: u32,
    pub contrast: f64,
    pub compensation: Option<[f64; 9]>,
}

pub fn prepare(metadata: &Metadata, name: &str) -> Result<Settings> {
    let contrasts = metadata
        .lists
        .get("ColorModeContrastCompensations")
        .ok_or("missing color-mode contrast list")?;
    let index = contrasts
        .index(name)
        .ok_or_else(|| format!("unavailable color mode {name}"))?;
    let contrast = metadata.scalar(contrasts.get(name).unwrap())?;
    let key = metadata
        .lists
        .get("ColorModeCompensations")
        .and_then(|list| list.get(name))
        .ok_or("missing color-mode matrix selection")?;
    let matrix = metadata.fixed::<9>(key)?;
    Ok(Settings {
        tone_index: index as u32,
        contrast,
        compensation: (matrix[0] != 0.).then_some(matrix),
    })
}

impl Settings {
    pub fn camera_matrix(&self, camera: [f64; 9]) -> [f64; 9] {
        let Some(mode) = self.compensation else {
            return camera;
        };
        std::array::from_fn(|i| {
            let (row, col) = (i / 3, i % 3);
            (0..3)
                .map(|k| camera[row * 3 + k] * mode[k * 3 + col])
                .sum()
        })
    }
}
