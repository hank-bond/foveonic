//! Prepares TIFF destination transforms independently of the photographic working space.

use crate::image::ColorSpace;

/// TIFF destination primaries and transfer. None in Settings retains the camera space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[repr(u32)]
pub enum TiffColorSpace {
    Srgb = 0,
    AdobeRgb = 1,
    #[serde(rename = "prophoto-rgb")]
    ProPhotoRgb = 2,
}

impl From<ColorSpace> for TiffColorSpace {
    fn from(space: ColorSpace) -> Self {
        match space {
            ColorSpace::Srgb => Self::Srgb,
            ColorSpace::AdobeRgb => Self::AdobeRgb,
        }
    }
}

impl TiffColorSpace {
    pub const ALL: [Self; 3] = [Self::Srgb, Self::AdobeRgb, Self::ProPhotoRgb];

    pub fn label(self) -> &'static str {
        match self {
            Self::Srgb => "sRGB",
            Self::AdobeRgb => "Adobe RGB",
            Self::ProPhotoRgb => "ProPhoto RGB",
        }
    }

    pub(crate) fn profile(self) -> &'static [u8] {
        match self {
            Self::Srgb => include_bytes!("../../../assets/srgb-v4.icc"),
            Self::AdobeRgb => include_bytes!("../../../assets/adobe-rgb-1998-compat.icc"),
            Self::ProPhotoRgb => include_bytes!("../../../assets/romm-v4.icc"),
        }
    }

    // D50 colorants from the embedded CC0 Compact-ICC-Profiles assets. All
    // three already share the ICC connection-space white, so no additional
    // white adaptation belongs between these matrices.
    fn to_xyz(self) -> [f64; 9] {
        match self {
            Self::Srgb => [28576, 25238, 9376, 14578, 46985, 3973, 911, 6362, 46788],
            Self::AdobeRgb => [39960, 13453, 9777, 20389, 41004, 4143, 1276, 3989, 48796],
            Self::ProPhotoRgb => [52279, 8858, 2053, 18878, 46653, 5, 0, 1, 54060],
        }
        .map(|v| f64::from(v) / 65536.)
    }

    /// Row-major matrix padded to WGSL uniform vec4 rows. Clipping happens
    /// only after this transform, so out-of-gamut components can mix correctly.
    pub(crate) fn transform(self, source: ColorSpace) -> [[f32; 4]; 3] {
        let source = Self::from(source);
        let matrix = if self == source {
            [1., 0., 0., 0., 1., 0., 0., 0., 1.]
        } else {
            let a = inverse(self.to_xyz());
            let b = source.to_xyz();
            std::array::from_fn(|i| (0..3).map(|k| a[i / 3 * 3 + k] * b[k * 3 + i % 3]).sum())
        };
        std::array::from_fn(|r| {
            std::array::from_fn(|c| if c < 3 { matrix[r * 3 + c] as f32 } else { 0. })
        })
    }
}

fn inverse(m: [f64; 9]) -> [f64; 9] {
    let [a, b, c, d, e, f, g, h, i] = m;
    let cofactors = [
        e * i - f * h,
        c * h - b * i,
        b * f - c * e,
        f * g - d * i,
        a * i - c * g,
        c * d - a * f,
        d * h - e * g,
        b * g - a * h,
        a * e - b * d,
    ];
    let determinant = a * cofactors[0] + b * cofactors[3] + c * cofactors[6];
    cofactors.map(|v| v / determinant)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorants_match_embedded_profiles_and_transforms_preserve_pcs() {
        for destination in TiffColorSpace::ALL {
            let bytes = destination.profile();
            let word = |offset| u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap());
            let mut xyz = [0.; 9];
            for (c, tag) in [b"rXYZ", b"gXYZ", b"bXYZ"].into_iter().enumerate() {
                let row = (0..word(128) as usize)
                    .map(|i| 132 + i * 12)
                    .find(|&offset| &bytes[offset..offset + 4] == tag)
                    .unwrap();
                let offset = word(row + 4) as usize;
                for r in 0..3 {
                    xyz[r * 3 + c] = f64::from(word(offset + 8 + r * 4) as i32) / 65536.;
                }
            }
            assert_eq!(destination.to_xyz(), xyz);
            for source in [ColorSpace::Srgb, ColorSpace::AdobeRgb] {
                let transform = destination.transform(source);
                for input in [[1.; 3], [1., 0., 0.], [0.1, 0.7, 1.2], [-0.1, 0.3, 0.6]] {
                    let output: [f64; 3] = std::array::from_fn(|r| {
                        (0..3).map(|c| f64::from(transform[r][c]) * input[c]).sum()
                    });
                    for r in 0..3 {
                        let expected: f64 = (0..3)
                            .map(|c| TiffColorSpace::from(source).to_xyz()[r * 3 + c] * input[c])
                            .sum();
                        let actual: f64 = (0..3).map(|c| xyz[r * 3 + c] * output[c]).sum();
                        assert!((actual - expected).abs() < 1e-7);
                    }
                }
            }
        }
    }
}
