//! Packages GPU-encoded RGB16 samples into an oriented, color-profiled TIFF.

use crate::calibration::{ColorSpace, EncodeParams};
use crate::source::Result;
use std::io::{Seek, Write};

const SRGB_ICC: &[u8] = include_bytes!("../assets/srgb-v4.icc");
const ADOBE_ICC: &[u8] = include_bytes!("../assets/adobe-rgb-1998-compat.icc");

pub fn write_tiff<W: Write + Seek>(
    writer: W,
    bytes: &[u8],
    params: &EncodeParams,
    space: ColorSpace,
    profile: &mut crate::profile::Profile,
) -> Result<()> {
    let [width, height, _, _] = params.output;
    if bytes.len() != width as usize * height as usize * 8 {
        return Err("invalid packed RGB16 output".into());
    }
    let start = std::time::Instant::now();
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 3);
    for pixel in bytes.as_chunks::<8>().0 {
        for offset in [0, 2, 4] {
            pixels.push(u16::from_le_bytes(pixel[offset..offset + 2].try_into()?));
        }
    }
    profile.record("TIFF RGB16 repack", start);
    let start = std::time::Instant::now();
    let mut encoder = tiff::encoder::TiffEncoder::new(writer)?;
    let mut image = encoder.new_image::<tiff::encoder::colortype::RGB16>(width, height)?;
    let icc = match space {
        ColorSpace::Srgb => SRGB_ICC,
        ColorSpace::AdobeRgb => ADOBE_ICC,
    };
    image
        .encoder()
        .write_tag(tiff::tags::Tag::Unknown(34675), icc)?;
    image
        .encoder()
        .write_tag(tiff::tags::Tag::Orientation, 1_u16)?;
    image
        .encoder()
        .write_tag(tiff::tags::Tag::Software, "Foveonic GPU")?;
    image.write_data(&pixels)?;
    profile.record("TIFF write", start);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tiff_preserves_packed_rgb16_and_matching_profile() {
        for space in [ColorSpace::Srgb, ColorSpace::AdobeRgb] {
            let params = EncodeParams {
                source: [2, 1, 0, 0],
                crop: [0, 0, 2, 1],
                output: [2, 1, 0, 0],
            };
            let values = [0_u16, 1, 65535, 0, 128, 32768, 64000, 0];
            let bytes: Vec<u8> = values
                .iter()
                .flat_map(|value| value.to_le_bytes())
                .collect();
            let mut writer = std::io::Cursor::new(Vec::new());
            write_tiff(
                &mut writer,
                &bytes,
                &params,
                space,
                &mut crate::profile::Profile::default(),
            )
            .unwrap();
            writer.set_position(0);
            let mut decoder = tiff::decoder::Decoder::new(writer).unwrap();
            assert_eq!(decoder.dimensions().unwrap(), (2, 1));
            let tiff::decoder::DecodingResult::U16(pixels) = decoder.read_image().unwrap() else {
                panic!("expected RGB16");
            };
            assert_eq!(pixels, [0, 1, 65535, 128, 32768, 64000]);
            assert_eq!(
                decoder
                    .get_tag_u8_vec(tiff::tags::Tag::Unknown(34675))
                    .unwrap(),
                match space {
                    ColorSpace::Srgb => SRGB_ICC,
                    ColorSpace::AdobeRgb => ADOBE_ICC,
                }
            );
        }
    }
}
