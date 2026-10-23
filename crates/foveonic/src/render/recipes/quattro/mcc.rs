//! Prepares color-mode chroma sectors from the selected Quattro or Tiger metadata table.

use crate::{
    nodes::color::sector_color::{Prepared, Sector},
    source::{MatrixEncoding, Metadata, Result},
};
const RGB_TO_CHROMA: [f32; 9] = [
    0.30078125,
    0.5859375,
    0.11328125,
    -0.16796875,
    -0.33203125,
    0.5,
    0.5,
    -0.41796875,
    -0.08203125,
];
const CHROMA_BIN_BOUNDARIES: [f32; 6] = [2048., 1546., 1186., 862., 502., 0.];
// Inverse chroma basis used to construct the two boundary colors around each sector.
const CHROMA_TO_RGB: [[f32; 2]; 3] = [
    [0.0030359796, 1.3994575],
    [-0.34422842, -0.7172017],
    [1.7724308, -0.0061365543],
];
const HUE_KNOTS: usize = 21;
const CONTROL_CURVES: usize = 5;
const SECTOR_DEGREES: f32 = 18.;

/// This stage was introduced at stored processing version 3.0.4.0. Earlier
/// files retain their raw-color result even when they contain named mode tables.
pub(super) fn enabled(metadata: &Metadata) -> bool {
    metadata
        .processing_version
        .is_some_and(|v| v >= [3, 0, 4, 0] && (v[0] == 3 || super::tiger::supported(metadata)))
}

pub(super) fn prepare(metadata: &Metadata, mode: &str) -> Result<Prepared> {
    let extent = super::calibration::full_extent(metadata)?;
    let sectors = sectors(metadata, mode)?;
    Prepared::new(extent, RGB_TO_CHROMA, CHROMA_BIN_BOUNDARIES, sectors)
}
fn sectors(metadata: &Metadata, mode: &str) -> Result<[Sector; 20]> {
    let table = metadata
        .matrix(&format!("MultiAxisTable_{mode}"))
        .ok_or("missing selected color-mode chroma-sector controls")?;
    if table.encoding != MatrixEncoding::Float32
        || table.dimensions != [2, CONTROL_CURVES, HUE_KNOTS]
        || table.values.len() != 2 * CONTROL_CURVES * HUE_KNOTS
        || table.values.iter().any(|v| !v.is_finite())
    {
        return Err("invalid color-mode chroma-sector controls".into());
    }
    // This ordinary caller uses the first hue and saturation control curves.
    // The remaining stored curves are retained metadata, not additional passes.
    let hue = &table.values[..HUE_KNOTS];
    let saturation = &table.values[CONTROL_CURVES * HUE_KNOTS..(CONTROL_CURVES + 1) * HUE_KNOTS];
    Ok(std::array::from_fn(|index| {
        let low = index as f32 * SECTOR_DEGREES;
        let high = (index + 1) as f32 * SECTOR_DEGREES;
        let original_high = vertex(high, 1.);
        let original_low = vertex(low, 1.);
        let mapped_high = vertex(high + hue[index + 1] as f32, saturation[index + 1] as f32);
        let mapped_low = vertex(low + hue[index] as f32, saturation[index] as f32);
        Sector {
            matrix: boundary_matrix(original_high, original_low, mapped_high, mapped_low),
            quadrant: (index / 5 + 1) as u32,
            bin: (index % 5 + 1) as u32,
        }
    }))
}
fn vertex(degrees: f32, saturation: f32) -> [f32; 3] {
    let (sine, cosine) = ((degrees / 180.) * std::f32::consts::PI).sin_cos();
    let denominator = cosine.abs() + sine.abs();
    let u = (saturation * cosine) / denominator;
    let v = (saturation * sine) / denominator;
    CHROMA_TO_RGB.map(|[a, b]| v.mul_add(b, u * a + 1.))
}
// Invert the basis formed by the two original boundary colors and neutral.
// Keep the reference implementation's Float32 FMA ordering during inversion.
fn boundary_matrix(
    high: [f32; 3],
    low: [f32; 3],
    mapped_high: [f32; 3],
    mapped_low: [f32; 3],
) -> [f32; 9] {
    let low_green_blue = low[1] - low[2];
    let high_green_blue = high[1] - high[2];
    let neutral_red = low[2].mul_add(high[1], -(high[2] * low[1]));
    let inverse = 1. / (neutral_red + high[0].mul_add(low_green_blue, -(low[0] * high_green_blue)));
    let high_basis = [
        low_green_blue * inverse,
        -((low[0] - low[2]) * inverse),
        (low[0] - low[1]) * inverse,
    ];
    let low_basis = [
        -(high_green_blue * inverse),
        (high[0] - high[2]) * inverse,
        -((high[0] - high[1]) * inverse),
    ];
    let neutral_basis = [
        neutral_red * inverse,
        -(low[2].mul_add(high[0], -(high[2] * low[0])) * inverse),
        low[1].mul_add(high[0], -(high[1] * low[0])) * inverse,
    ];
    std::array::from_fn(|index| {
        let channel = index / 3;
        let column = index % 3;
        neutral_basis[column]
            + mapped_high[channel]
                .mul_add(high_basis[column], mapped_low[channel] * low_basis[column])
    })
}

#[cfg(test)]
#[path = "mcc_test.rs"]
mod tests;
