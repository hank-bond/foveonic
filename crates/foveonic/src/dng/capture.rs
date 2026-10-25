//! Carries available camera capture details into DNG EXIF without rereading the original.

use super::{signed_rationals, tag, unsigned_rationals};
use crate::source::{Metadata, Result};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    io::{Seek, Write},
};
use tiff::{
    encoder::{DirectoryEncoder, TiffEncoder, TiffKindStandard, TiffValue},
    tags::Type,
};

type Directory<'a, W> = DirectoryEncoder<'a, W, TiffKindStandard>;
const EXIF_IFD: u16 = 0x8769;
const EXIF_VERSION: u16 = 0x9000;
const DATE_TIME_ORIGINAL: u16 = 0x9003;
const DATE_TIME_DIGITIZED: u16 = 0x9004;
const EXPOSURE_TIME: u16 = 0x829a;
const F_NUMBER: u16 = 0x829d;
const ISO_SPEED_RATINGS: u16 = 0x8827;
const EXPOSURE_PROGRAM: u16 = 0x8822;
const EXPOSURE_BIAS: u16 = 0x9204;
const METERING_MODE: u16 = 0x9207;
const FOCAL_LENGTH: u16 = 0x920a;
const BODY_SERIAL_NUMBER: u16 = 0xa431;
const LENS_MODEL: u16 = 0xa434;
const SENSITIVITY_TYPE: u16 = 0x8830;
const ISO_SPEED: u16 = 0x8833;
const FOCAL_LENGTH_35MM: u16 = 0xa405;
const PIXEL_X_DIMENSION: u16 = 0xa002;
const PIXEL_Y_DIMENSION: u16 = 0xa003;
const DATE_TIME: u16 = 0x0132;
const CAMERA_SERIAL_NUMBER: u16 = 50735;
const ARTIST: u16 = 0x013b;
const COPYRIGHT: u16 = 0x8298;
const MICROSECONDS_PER_SECOND: f64 = 1_000_000.;
const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
// Last second representable by EXIF's four-digit year.
const MAX_TIMESTAMP: u64 = 253_402_300_799;

struct ExifVersion;
impl TiffValue for ExifVersion {
    const BYTE_LEN: u8 = 1;
    const FIELD_TYPE: Type = Type::UNDEFINED;
    fn count(&self) -> usize {
        4
    }
    fn data(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(b"0230")
    }
}

pub(super) struct Capture<'a> {
    properties: &'a BTreeMap<String, String>,
    date: Option<String>,
}
impl<'a> Capture<'a> {
    pub fn new(metadata: &'a Metadata) -> Self {
        let properties = &metadata.properties;
        let date = properties
            .get("TIME")
            .and_then(|value| value.trim().parse().ok())
            .and_then(capture_date)
            .or_else(|| {
                properties
                    .get("EXIF_DATETIME")
                    .filter(|value| valid_date(value))
                    .cloned()
            });
        Self { properties, date }
    }

    fn text(&self, key: &str) -> Option<&str> {
        self.properties
            .get(key)
            .map(|v| v.trim())
            .filter(|v| !v.is_empty() && v.is_ascii() && !v.contains('\0'))
    }
    fn number(&self, key: &str) -> Option<f64> {
        self.text(key)?
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
    }
    fn positive(&self, key: &str) -> Option<f64> {
        self.number(key).filter(|v| *v > 0.)
    }
    fn shutter(&self) -> Option<f64> {
        self.text("SH_DESC")
            .and_then(|v| {
                if let Some((n, d)) = v.split_once('/') {
                    Some(n.trim().parse::<f64>().ok()? / d.trim().parse::<f64>().ok()?)
                } else {
                    v.trim_end_matches('"').parse().ok()
                }
            })
            .filter(|v| v.is_finite() && *v > 0.)
            .or_else(|| self.positive("SHUTTER"))
            .or_else(|| {
                self.positive("EXPTIME")
                    .map(|v| v / MICROSECONDS_PER_SECOND)
            })
    }

    pub fn write_exif<W: Write + Seek>(
        &self,
        encoder: &mut TiffEncoder<W>,
        size: [u32; 2],
    ) -> Result<u32> {
        let mut directory = encoder.extra_directory()?;
        tag(&mut directory, EXIF_VERSION, ExifVersion)?;
        for (id, value) in [
            (EXPOSURE_TIME, self.shutter()),
            (
                F_NUMBER,
                self.positive("EXIF_FNUMBER")
                    .or_else(|| self.positive("AP_DESC"))
                    .or_else(|| self.positive("APERTURE")),
            ),
            (FOCAL_LENGTH, self.number("FLENGTH")),
        ] {
            if let Some(values) = value
                .filter(|v| *v > 0.)
                .and_then(|v| unsigned_rationals(&[v]).ok())
            {
                tag(&mut directory, id, values.as_slice())?;
            }
        }
        if let Some(values) = self
            .number("EXIF_EXPOSURE_BIAS")
            .or_else(|| self.number("EXPCOMP"))
            .and_then(|v| signed_rationals(&[v]).ok())
        {
            tag(&mut directory, EXPOSURE_BIAS, values.as_slice())?;
        }
        if let Some(iso) = self
            .number("ISO")
            .filter(|v| v.fract() == 0. && (1. ..=u32::MAX as f64).contains(v))
        {
            let iso = iso as u32;
            tag(
                &mut directory,
                ISO_SPEED_RATINGS,
                iso.min(u16::MAX as u32) as u16,
            )?;
            tag(&mut directory, SENSITIVITY_TYPE, 3_u16)?; // ISO speed.
            tag(&mut directory, ISO_SPEED, iso)?;
        }
        if let Some(date) = &self.date {
            tag(&mut directory, DATE_TIME_ORIGINAL, date.as_str())?;
            tag(&mut directory, DATE_TIME_DIGITIZED, date.as_str())?;
        }
        if let Some(program) = self.text("PMODE").and_then(|v| match v {
            "M" => Some(1_u16),
            "P" => Some(2),
            "A" => Some(3),
            "S" => Some(4),
            _ => None,
        }) {
            tag(&mut directory, EXPOSURE_PROGRAM, program)?;
        }
        if let Some(mode) = self.text("AEMODE").and_then(|v| match v {
            "A" => Some(1_u16),
            "C" => Some(2),
            "8" => Some(5),
            _ => None,
        }) {
            tag(&mut directory, METERING_MODE, mode)?;
        }
        for (id, key) in [(BODY_SERIAL_NUMBER, "CAMSERIAL"), (LENS_MODEL, "LENSMODEL")] {
            if let Some(value) = self.text(key) {
                tag(&mut directory, id, value)?;
            }
        }
        if let Some(value) = self
            .number("FLEQ35MM")
            .filter(|v| v.fract() == 0. && (1. ..=u16::MAX as f64).contains(v))
        {
            tag(&mut directory, FOCAL_LENGTH_35MM, value as u16)?;
        }
        // EXIF geometry describes the exported, already-oriented image.
        tag(&mut directory, PIXEL_X_DIMENSION, size[0])?;
        tag(&mut directory, PIXEL_Y_DIMENSION, size[1])?;
        Ok(directory.finish_with_offsets()?.offset)
    }

    pub fn write_primary<W: Write + Seek>(
        &self,
        directory: &mut Directory<'_, W>,
        exif: u32,
    ) -> Result<()> {
        tag(directory, EXIF_IFD, exif)?;
        if let Some(date) = &self.date {
            tag(directory, DATE_TIME, date.as_str())?;
        }
        for (id, key) in [
            (CAMERA_SERIAL_NUMBER, "CAMSERIAL"),
            (ARTIST, "ARTIST"),
            (COPYRIGHT, "COPYRIGHT"),
        ] {
            if let Some(value) = self.text(key) {
                tag(directory, id, value)?;
            }
        }
        Ok(())
    }
}

fn leap(year: u32) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}
fn months(year: u32) -> [u64; 12] {
    [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ]
}
fn capture_date(seconds: u64) -> Option<String> {
    if seconds > MAX_TIMESTAMP {
        return None;
    }
    let mut days = seconds / SECONDS_PER_DAY;
    let mut year = 1970;
    while days >= if leap(year) { 366 } else { 365 } {
        days -= if leap(year) { 366 } else { 365 };
        year += 1;
    }
    let mut month = 0;
    let lengths = months(year);
    while days >= lengths[month] {
        days -= lengths[month];
        month += 1;
    }
    // Camera TIME is already its recorded wall clock. Do not apply host timezone conversion.
    Some(format!(
        "{year:04}:{:02}:{:02} {:02}:{:02}:{:02}",
        month + 1,
        days + 1,
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    ))
}
fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 19
        || bytes[4] != b':'
        || bytes[7] != b':'
        || bytes[10] != b' '
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return false;
    }
    if bytes
        .iter()
        .enumerate()
        .any(|(i, b)| !matches!(i, 4 | 7 | 10 | 13 | 16) && !b.is_ascii_digit())
    {
        return false;
    }
    let Some(parts) = value
        .split([':', ' '])
        .map(|part| part.parse::<u32>().ok())
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    let Ok([year, month, day, hour, minute, second]) = <[u32; 6]>::try_from(parts) else {
        return false;
    };
    (1..=9999).contains(&year)
        && (1..=12).contains(&month)
        && day > 0
        && u64::from(day) <= months(year)[month as usize - 1]
        && hour < 24
        && minute < 60
        && second < 60
}

#[cfg(test)]
#[path = "capture_test.rs"]
mod tests;
