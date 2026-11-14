//! Describes power-of-two sensor sampling and the covered, padded working grid.

use crate::source::Result;

/// Native sensor pixels represented by one working pixel along each axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct Scale(u32);

impl Default for Scale {
    fn default() -> Self {
        Self::NATIVE
    }
}

impl<'de> serde::Deserialize<'de> for Scale {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let divisor = <u32 as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(divisor).map_err(serde::de::Error::custom)
    }
}

impl Scale {
    pub const NATIVE: Self = Self(1);

    pub fn new(divisor: u32) -> Result<Self> {
        if !divisor.is_power_of_two() {
            return Err("working scale requires a positive power-of-two divisor".into());
        }
        Ok(Self(divisor))
    }
    pub fn divisor(self) -> u32 {
        self.0
    }
    pub fn coarsest(self, other: Self) -> Self {
        Self(self.0.max(other.0))
    }
    pub fn factor(self) -> f32 {
        1. / self.0 as f32
    }
    pub fn is_native(self) -> bool {
        self == Self::NATIVE
    }
    pub fn is_reduced(self) -> bool {
        !self.is_native()
    }
    /// Cover the final partial sensor cell without discarding its edge.
    pub fn extent(self, native: [u32; 2]) -> [u32; 2] {
        native.map(|v| v.div_ceil(self.0))
    }
    pub fn bounds(self, native: [u32; 4]) -> [u32; 4] {
        [
            native[0] / self.0,
            native[1] / self.0,
            native[2].div_ceil(self.0),
            native[3].div_ceil(self.0),
        ]
    }
}

/// Logical sensor coverage and algorithmic padding remain distinct. Camera
/// recipes interpret native framing once and pass this geometry to operations.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WorkingGrid {
    pub scale: Scale,
    pub native: [u32; 2],
    pub logical: [u32; 2],
    pub extent: [u32; 2],
    pub origin: [u32; 2],
    alignment: u32,
}
impl WorkingGrid {
    pub fn new(native: [u32; 2], scale: Scale, alignment: u32) -> Result<Self> {
        if native.contains(&0) || !alignment.is_power_of_two() {
            return Err("invalid sensor working grid".into());
        }
        let logical = scale.extent(native);
        let mut extent = logical;
        if scale.is_reduced() {
            for axis in &mut extent {
                *axis = axis
                    .div_ceil(alignment)
                    .checked_mul(alignment)
                    .ok_or("working grid padding overflow")?;
            }
        }
        Ok(Self {
            scale,
            native,
            logical,
            extent,
            origin: [0; 2],
            alignment,
        })
    }

    /// Replicated sensor edges supply filter support around the logical image.
    /// The native recipe keeps its original geometry and boundary behavior.
    pub fn with_halo(mut self, pixels: u32) -> Result<Self> {
        if self.scale.is_reduced() {
            self.origin = [pixels; 2];
            for (extent, logical) in self.extent.iter_mut().zip(self.logical) {
                *extent = pixels
                    .checked_mul(2)
                    .and_then(|halo| logical.checked_add(halo))
                    .and_then(|size| size.div_ceil(self.alignment).checked_mul(self.alignment))
                    .ok_or("working grid halo overflow")?;
            }
        }
        Ok(self)
    }

    pub fn bounds(self, native: [u32; 4]) -> [u32; 4] {
        let mut bounds = self.scale.bounds(native);
        for axis in 0..2 {
            bounds[axis] += self.origin[axis];
            bounds[axis + 2] += self.origin[axis];
        }
        bounds
    }

    /// Map normalized photograph coordinates from native crop edges to working
    /// sample indices. Pooled samples sit at block centers, including the halo.
    pub fn frame_mapping(self, native: [u32; 4]) -> [[f32; 4]; 2] {
        let divisor = self.scale.divisor() as f32;
        let [left, top, right, bottom] = native.map(|edge| edge as f32 / divisor);
        [
            [right - left, 0., left + self.origin[0] as f32 - 0.5, 1.],
            [0., bottom - top, top + self.origin[1] as f32 - 0.5, 1.],
        ]
    }

    /// Padding repeats a covered edge cell, including its eligibility for
    /// filtering. Photographic framing still uses the unexpanded bounds above.
    pub fn filter_bounds(self, native: [u32; 4]) -> [u32; 4] {
        let logical = self.scale.bounds(native);
        let mut bounds = self.bounds(native);
        if native[0] >= native[2] || native[1] >= native[3] {
            return bounds;
        }
        for axis in 0..2 {
            if logical[axis] == 0 {
                bounds[axis] = 0;
            }
            if logical[axis + 2] == self.logical[axis] {
                bounds[axis + 2] = self.extent[axis];
            }
        }
        bounds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_cover_odd_edges_and_keep_padding_separate() {
        for divisor in [1, 2, 4, 8, 16, 64, 1 << 31] {
            let scale = Scale::new(divisor).unwrap();
            let grid = WorkingGrid::new([257, 193], scale, 8).unwrap();
            assert_eq!(
                grid.logical,
                [257_u32.div_ceil(divisor), 193_u32.div_ceil(divisor)]
            );
            assert!(grid
                .extent
                .into_iter()
                .zip(grid.logical)
                .all(|(padded, logical)| padded >= logical));
            if scale.is_reduced() {
                assert!(grid.extent.into_iter().all(|v| v.is_multiple_of(8)));
            } else {
                assert_eq!(grid.extent, [257, 193]);
            }
        }
        for divisor in [0, 3, 6, u32::MAX] {
            assert!(Scale::new(divisor).is_err());
        }
    }

    #[test]
    fn replicated_halo_keeps_framing_and_filter_coverage_distinct() {
        let grid = WorkingGrid::new([257, 193], Scale::new(512).unwrap(), 8)
            .unwrap()
            .with_halo(16)
            .unwrap();
        assert_eq!(grid.logical, [1, 1]);
        assert_eq!(grid.origin, [16, 16]);
        assert_eq!(grid.extent, [40, 40]);
        assert_eq!(grid.bounds([7, 5, 249, 187]), [16, 16, 17, 17]);
        assert_eq!(grid.filter_bounds([7, 5, 249, 187]), [0, 0, 40, 40]);
        let native = WorkingGrid::new([257, 193], Scale::NATIVE, 8)
            .unwrap()
            .with_halo(16)
            .unwrap();
        assert_eq!(native.origin, [0, 0]);
        assert_eq!(native.extent, [257, 193]);
    }
}
