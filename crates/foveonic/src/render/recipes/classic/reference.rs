//! Defines shared numerical conventions recovered from the Classic reference implementation.

// Preserve the recovered coefficients and arithmetic order. The exact standard
// color-space identity of this basis has not been established.
pub(super) const COLOR_BASIS: [f64; 9] = [
    1.4032, -0.2231, -0.1016, -0.5263, 1.4816, 0.017, -0.0112, 0.0183, 0.9113,
];
pub(super) const D65_WHITE_XYZ: [f64; 3] = [0.3127, 0.329, 0.3583];
pub(super) const NEUTRAL_ROW_FLOOR: f64 = 0.25;
pub(super) const Q14_SCALE: f64 = 16384.;
pub(super) const MICROSECONDS_PER_SECOND: f64 = 1_000_000.;
// The profile's recovered power exponent. It does not include the ROMM toe.
pub(super) const PROFILE_POWER: f64 = 461. / 256.;
// The reference switches reconstruction equations at this recorded capture ISO.
pub(super) const RANGE_RECONSTRUCTION_ISO: f64 = 400.;
pub(super) const RECURSIVE_CHROMA_THRESHOLD: f64 = 101.;

/// Reference neutral weighting, with the original green-first summation order.
pub(super) fn neutral_weight(v: [f64; 3]) -> f64 {
    v[1] * 0.55 + v[0] * 0.3 + v[2] * 0.15
}
