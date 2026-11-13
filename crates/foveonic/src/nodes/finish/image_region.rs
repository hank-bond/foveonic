//! Copies a rectangular numerical signal on the GPU while preserving every component bit.

use crate::{
    gpu::{Gpu, ReadGuard},
    recipe::Key,
    source::Result,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Prepared {
    source: [u32; 2],
    rect: [u32; 4],
    components: u32,
    reserved: u32,
}
impl Prepared {
    pub(crate) fn new(source: [u32; 2], rect: [u32; 4], components: u32) -> Result<Self> {
        if rect[0] >= rect[2]
            || rect[1] >= rect[3]
            || rect[2] > source[0]
            || rect[3] > source[1]
            || ![1, 3].contains(&components)
            || u64::from(source[0]) * u64::from(source[1])
                > u64::from(u32::MAX) / u64::from(components)
        {
            return Err("invalid numerical region geometry".into());
        }
        Ok(Self {
            source,
            rect,
            components,
            reserved: 0,
        })
    }
    pub(crate) fn input_bounds(&self, output: Option<[u32; 4]>) -> Result<[u32; 4]> {
        let bounds = output.unwrap_or([0, 0, self.extent()[0], self.extent()[1]]);
        if bounds[0] >= bounds[2]
            || bounds[1] >= bounds[3]
            || bounds[2] > self.extent()[0]
            || bounds[3] > self.extent()[1]
        {
            return Err("requested numerical region exceeds its output".into());
        }
        Ok(std::array::from_fn(|i| bounds[i] + self.rect[i % 2]))
    }

    pub(crate) fn with_input_bounds(self, bounds: [u32; 4]) -> Result<Self> {
        if bounds[0] > self.rect[0]
            || bounds[1] > self.rect[1]
            || bounds[2] < self.rect[2]
            || bounds[3] < self.rect[3]
        {
            return Err("numerical extraction exceeds its available input".into());
        }
        Self::new(
            [bounds[2] - bounds[0], bounds[3] - bounds[1]],
            std::array::from_fn(|i| self.rect[i] - bounds[i % 2]),
            self.components,
        )
    }

    pub(crate) fn extent(&self) -> [u32; 2] {
        [self.rect[2] - self.rect[0], self.rect[3] - self.rect[1]]
    }
    pub(crate) fn key(&self) -> Key {
        Key::pod(self)
    }
}
/// A compact RGB result keeps its bounds in the unchanged input image domain.
/// Input and output bounds are separate so a retained broad input remains usable.
#[derive(Clone, Copy, Debug, Default)]
pub struct Window {
    pub(crate) input: Option<[u32; 4]>,
    pub(crate) output: Option<[u32; 4]>,
}
impl Window {
    pub(crate) fn storage(self) -> Option<[u32; 4]> {
        self.output.or(self.input)
    }

    pub(crate) fn key(self, key: Key) -> Key {
        match self.output {
            Some(bounds) => key.with(&bounds),
            None => key,
        }
    }

    pub(crate) fn crop(self, extent: [u32; 2]) -> Result<Option<Prepared>> {
        let Some(output) = self.output else {
            return Ok(None);
        };
        let input = self.input.unwrap_or([0, 0, extent[0], extent[1]]);
        if input[2] - input[0] != extent[0]
            || input[3] - input[1] != extent[1]
            || output[0] < input[0]
            || output[1] < input[1]
            || output[2] > input[2]
            || output[3] > input[3]
        {
            return Err("regional RGB output exceeds its available input".into());
        }
        if output == input {
            return Ok(None);
        }
        Prepared::new(extent, std::array::from_fn(|i| output[i] - input[i % 2]), 3).map(Some)
    }
}

/// Keep framing inside the newly available storage. A useful crop retains its
/// oriented output dimensions when removing only its surrounding support.
pub(crate) fn map_view(mut view: crate::image::View, bounds: [u32; 4]) -> crate::image::View {
    let original = view.crop;
    view.crop = [
        original[0].max(bounds[0]).min(bounds[2]) - bounds[0],
        original[1].max(bounds[1]).min(bounds[3]) - bounds[1],
        original[2].max(bounds[0]).min(bounds[2]) - bounds[0],
        original[3].max(bounds[1]).min(bounds[3]) - bounds[1],
    ];
    if original[0] < bounds[0]
        || original[1] < bounds[1]
        || original[2] > bounds[2]
        || original[3] > bounds[3]
    {
        let extent = [view.crop[2] - view.crop[0], view.crop[3] - view.crop[1]];
        view.extent = if view.rotation.is_multiple_of(180) {
            extent
        } else {
            [extent[1], extent[0]]
        };
    }
    view
}

pub(crate) fn apply(gpu: &Gpu, input: &ReadGuard, prepared: &Prepared) -> Result<ReadGuard> {
    let pixel_bytes = u64::from(prepared.components) * std::mem::size_of::<f32>() as u64;
    if input.size() != u64::from(prepared.source[0]) * u64::from(prepared.source[1]) * pixel_bytes {
        return Err("numerical region input geometry mismatch".into());
    }
    let [width, height] = prepared.extent();
    let row_bytes = u64::from(width) * pixel_bytes;
    let mut output = gpu.storage("Regional numerical signal", row_bytes * u64::from(height))?;
    gpu.copy_ranges(
        input,
        output.write(),
        (0..height).map(|y| {
            [
                (u64::from(y + prepared.rect[1]) * u64::from(prepared.source[0])
                    + u64::from(prepared.rect[0]))
                    * pixel_bytes,
                u64::from(y) * row_bytes,
                row_bytes,
            ]
        }),
    );
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_output_uses_available_storage_coordinates() {
        let window = Window {
            input: Some([96, 64, 192, 144]),
            output: Some([103, 69, 182, 141]),
        };
        let crop = window.crop([96, 80]).unwrap().unwrap();
        assert_eq!(crop.extent(), [79, 72]);
        assert_eq!(crop.input_bounds(None).unwrap(), [7, 5, 86, 77]);
        assert!(window.crop([100, 80]).is_err());
        assert!(Window {
            output: Some([90, 69, 182, 141]),
            ..window
        }
        .crop([96, 80])
        .is_err());
    }

    #[test]
    fn extraction_maps_demands_and_rebases_against_a_broader_backing() {
        let full = Prepared::new([300, 200], [100, 80, 180, 140], 3).unwrap();
        assert_eq!(
            full.input_bounds(Some([7, 5, 79, 59])).unwrap(),
            [107, 85, 179, 139]
        );
        let compact = full.with_input_bounds([96, 64, 192, 144]).unwrap();
        assert_eq!(compact.input_bounds(None).unwrap(), [4, 16, 84, 76]);
        assert!(full.with_input_bounds([101, 64, 192, 144]).is_err());
    }

    #[test]
    fn removing_support_preserves_the_useful_oriented_view() {
        for rotation in [0, 90, 180, 270] {
            let extent = if rotation % 180 == 0 {
                [79, 72]
            } else {
                [72, 79]
            };
            let view = crate::image::View {
                crop: [103, 69, 182, 141],
                rotation,
                extent,
            };
            let mapped = map_view(view, [96, 64, 192, 144]);
            assert_eq!(mapped.crop, [7, 5, 86, 77]);
            assert_eq!(mapped.extent, extent);
            assert_eq!(mapped.rotation, rotation);
        }
    }
}
