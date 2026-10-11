//! Expands selected camera bad-pixel records into an immutable per-pixel repair map.
//!
//! Record coordinates, channel bits, and neighbor masks follow Foveonic's BadPixelsF20 decoding.

use crate::source::{Metadata, Result};

pub fn flags(metadata: &Metadata, minimum_badness: u32) -> Result<Vec<u32>> {
    if minimum_badness > 15 {
        return Err("repair badness must be in 0..15".into());
    }
    let matrix = metadata
        .matrix("BadPixelsF20")
        .ok_or("missing BadPixelsF20 metadata")?;
    if matrix.dimensions.len() != 2
        || matrix.dimensions[0] != 3
        || matrix.values.len() != matrix.dimensions[1] * 3
    {
        return Err("invalid BadPixelsF20 records".into());
    }
    let mut flags = vec![0_u32; metadata.width * metadata.height];
    for record in matrix.values.as_chunks::<3>().0 {
        if record
            .iter()
            .any(|v| *v < 0. || v.fract() != 0. || *v > u16::MAX as f64)
        {
            return Err("invalid bad-pixel record".into());
        }
        let [y, x, bits] = record.map(|v| v as u32);
        if x >= metadata.width as u32 || y >= metadata.height as u32 {
            return Err("bad-pixel record outside sensor".into());
        }
        if ((bits >> 8) & 15) >= minimum_badness {
            flags[y as usize * metadata.width + x as usize] |= bits & 0x70ff;
        }
    }
    Ok(flags)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{Matrix, MatrixEncoding};
    #[test]
    fn selection_keeps_layer_flags_and_camera_neighbors() {
        let metadata = Metadata {
            width: 3,
            height: 3,
            rotation: 0,
            properties: Default::default(),
            lists: Default::default(),
            matrices: [(
                "BadPixelsF20".into(),
                Matrix {
                    encoding: MatrixEncoding::Uint32,
                    representation: "u32",
                    dimensions: vec![3, 2],
                    values: vec![1., 1., 0x43ff as f64, 0., 0., 0x72ff as f64],
                },
            )]
            .into(),
        };
        let flags = flags(&metadata, 3).unwrap();
        assert_eq!(flags[4], 0x40ff);
        assert_eq!(flags[0], 0);
        assert!(super::flags(&metadata, 16).is_err());
    }
}
