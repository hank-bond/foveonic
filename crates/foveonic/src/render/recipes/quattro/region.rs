//! Plans aligned regional reconstruction after full-frame Quattro calibration and color preparation.

use super::stage2::Signals;
use crate::{
    graph::{Graph, Operation},
    nodes::finish::{code_crop, image_region, tone_equalizer},
    render::Settings,
    source::{Metadata, Result},
};

// Six 5/3 lifting levels reach 126 pixels in each transform direction, with
// another 64 for lowest-band filtering. 512 also covers reconstruction,
// bilateral filtering, signed-detail support, sharpening and final resampling.
const CONTEXT_MARGIN: u32 = 512;
// Preserve six-level lifting phase and the ordinary 128-pixel padding origin.
const GRID_ALIGNMENT: u32 = 128;

pub(super) struct Prepared {
    pub physical: [u32; 4],
    pub crop: code_crop::Prepared,
}
impl Prepared {
    pub fn new(metadata: &Metadata, native_extent: [u32; 2], output: [u32; 4]) -> Result<Self> {
        let crop = metadata.fixed::<4>("ActiveImageArea")?.map(|v| v as u32);
        let crop = code_crop::Prepared::new(
            [metadata.width as u32, metadata.height as u32],
            crop,
            native_extent,
            metadata.rotation,
            true,
        )?;
        let (crop, physical) = crop.region(output, CONTEXT_MARGIN, GRID_ALIGNMENT)?;
        Ok(Self { physical, crop })
    }
    pub fn extent(&self) -> [u32; 2] {
        [
            self.physical[2] - self.physical[0],
            self.physical[3] - self.physical[1],
        ]
    }
    pub fn build(
        &self,
        graph: &mut Graph,
        metadata: &Metadata,
        signals: &Signals,
        mask: usize,
    ) -> Result<(Signals, usize)> {
        let high = [metadata.width as u32, metadata.height as u32];
        let low = high.map(|v| v / 2);
        let rect = self.physical.map(|v| v / 2);
        let mut extract = |input: usize,
                           source: [u32; 2],
                           rect: [u32; 4],
                           components: u32,
                           output: &'static str| {
            // The immediate whole-frame continuation can restart here instead
            // of repeating calibration, repair and global color preparation.
            graph.retain_for_session(input);
            graph.add(
                Operation::ImageRegion {
                    prepared: image_region::Prepared::new(source, rect, components)?,
                    data: graph.outputs[input].data,
                    output,
                },
                &[input],
            )
        };
        let filtered = extract(signals.filtered, low, rect, 3, "region.filtered")?;
        let calibrated = extract(signals.calibrated, low, rect, 3, "region.calibrated")?;
        let processed = extract(signals.processed, low, rect, 3, "region.processed")?;
        let blue = extract(signals.blue, high, self.physical, 1, "region.blue")?;
        let mask = extract(mask, low, rect, 1, "region.mask")?;
        Ok((
            Signals {
                filtered,
                calibrated,
                processed,
                blue,
            },
            mask,
        ))
    }
}

/// Extend the output tile for shared pre-curve filters; sensor context is planned separately.
pub(super) fn expand_shared_edits(
    metadata: &Metadata,
    settings: &mut Settings,
    names: &[&str],
) -> Result<Option<[u32; 4]>> {
    if names != ["final"] {
        return Ok(None);
    }
    let Some(rect) = settings.preview_region else {
        return Ok(None);
    };
    let margin = super::super::linear_edits::support(settings)
        + if !settings.tonal_ranges.is_neutral() {
            tone_equalizer::support()
        } else {
            0
        };
    if margin == 0 {
        return Ok(None);
    }
    let encode =
        crate::nodes::sensor::calibration::encode(metadata, crate::color_space::ColorSpace::Srgb)?;
    let alignment = tone_equalizer::GUIDE_ALIGNMENT;
    let expanded = [
        rect[0].saturating_sub(margin) / alignment * alignment,
        rect[1].saturating_sub(margin) / alignment * alignment,
        rect[2]
            .saturating_add(margin)
            .div_ceil(alignment)
            .saturating_mul(alignment)
            .min(encode.output[0]),
        rect[3]
            .saturating_add(margin)
            .div_ceil(alignment)
            .saturating_mul(alignment)
            .min(encode.output[1]),
    ];
    settings.preview_region = Some(expanded);
    Ok(Some([
        rect[0] - expanded[0],
        rect[1] - expanded[1],
        rect[2] - expanded[0],
        rect[3] - expanded[1],
    ]))
}

pub(super) fn crop_shared_edits(
    recipe: &mut super::BuiltRecipe,
    rect: Option<[u32; 4]>,
) -> Result<()> {
    let Some(rect) = rect else {
        return Ok(());
    };
    let data = recipe.graph.outputs[recipe.final_id].data;
    recipe.final_id = recipe.graph.add(
        Operation::ImageRegion {
            prepared: image_region::Prepared::new(recipe.rgb_extent, rect, 3)?,
            data,
            output: "region.output",
        },
        &[recipe.final_id],
    )?;
    let extent = [rect[2] - rect[0], rect[3] - rect[1]];
    recipe.rgb_extent = extent;
    recipe.params.output[..2].copy_from_slice(&extent);
    recipe.params.crop = [0, 0, extent[0], extent[1]];
    Ok(())
}
