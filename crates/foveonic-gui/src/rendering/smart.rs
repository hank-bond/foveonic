//! Sizes native patches around the visible region and compares image regions.

// Keep small pans inside the native patch without another render.
const DETAIL_OVERSCAN_PIXELS: u32 = 64;

/// Expand the visible image region by the existing native-pixel margin.
pub(crate) fn render_region(visible: [u32; 4], window: [u32; 4]) -> [u32; 4] {
    [
        visible[0]
            .saturating_sub(DETAIL_OVERSCAN_PIXELS)
            .max(window[0]),
        visible[1]
            .saturating_sub(DETAIL_OVERSCAN_PIXELS)
            .max(window[1]),
        visible[2]
            .saturating_add(DETAIL_OVERSCAN_PIXELS)
            .min(window[2]),
        visible[3]
            .saturating_add(DETAIL_OVERSCAN_PIXELS)
            .min(window[3]),
    ]
}

pub fn contains(outer: [u32; 4], inner: [u32; 4]) -> bool {
    outer[0] <= inner[0] && outer[1] <= inner[1] && outer[2] >= inner[2] && outer[3] >= inner[3]
}
