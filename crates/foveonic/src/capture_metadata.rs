//! Translates source capture details into shared EXIF directories for all exports.

use crate::{
    color_space::TiffColorSpace,
    source::{Metadata, Result},
};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    io::{Seek, Write},
};
#[path = "capture_facts.rs"]
mod facts;
pub use facts::CaptureFacts;
use tiff::{
    encoder::{DirectoryEncoder, Rational, SRational, TiffEncoder, TiffKindStandard, TiffValue},
    tags::{Tag, Type},
};

fn tag<W: Write + Seek, T: TiffValue>(
    directory: &mut DirectoryEncoder<'_, W, TiffKindStandard>,
    number: u16,
    value: T,
) -> Result<()> {
    directory.write_tag(Tag::Unknown(number), value)?;
    Ok(())
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

// Continued fractions retain simple camera values such as 1/30 and 63/10.
// At the integer limits, choose the nearest representable fraction instead.
fn fraction(value: f64, max_n: u32, max_d: u32) -> (u32, u32) {
    let (mut p0, mut q0, mut p1, mut q1) = (0_u64, 1_u64, 1_u64, 0_u64);
    let mut rest = value;
    loop {
        let limit = (u64::from(max_n) - p0)
            .checked_div(p1)
            .unwrap_or(u64::MAX)
            .min((u64::from(max_d) - q0).checked_div(q1).unwrap_or(u64::MAX));
        let a = rest.floor();
        let bounded = a > limit as f64;
        let k = if bounded { limit } else { a as u64 };
        let (p2, q2) = (p0 + k * p1, q0 + k * q1);
        if bounded {
            let error = |p: u64, q: u64| {
                if q == 0 {
                    f64::INFINITY
                } else {
                    (p as f64 / q as f64 - value).abs()
                }
            };
            return if error(p1, q1) < error(p2, q2) || (error(p1, q1) == error(p2, q2) && q1 < q2) {
                (p1 as u32, q1 as u32)
            } else {
                (p2 as u32, q2 as u32)
            };
        }
        if p2 as f64 / q2 as f64 == value || rest == a {
            return (p2 as u32, q2 as u32);
        }
        (p0, q0, p1, q1) = (p1, q1, p2, q2);
        rest = 1. / (rest - a);
    }
}

fn unsigned_rationals(values: &[f64]) -> Result<Vec<Rational>> {
    values
        .iter()
        .map(|&v| {
            if !v.is_finite() || v <= 0. || v > u32::MAX as f64 {
                return Err("unrepresentable capture rational".into());
            }
            let (n, d) = fraction(v, u32::MAX, u32::MAX);
            if n == 0 {
                return Err("unrepresentable capture rational".into());
            }
            Ok(Rational { n, d })
        })
        .collect()
}
fn signed_rationals(values: &[f64]) -> Result<Vec<SRational>> {
    values
        .iter()
        .map(|&v| {
            if !v.is_finite() || v.abs() > i32::MAX as f64 {
                return Err("unrepresentable capture rational".into());
            }
            let (n, d) = fraction(v.abs(), i32::MAX as u32, i32::MAX as u32);
            Ok(SRational {
                n: if v < 0. { -(n as i32) } else { n as i32 },
                d: d as i32,
            })
        })
        .collect()
}

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
const LENS_SPECIFICATION: u16 = 0xa432;
const SENSITIVITY_TYPE: u16 = 0x8830;
const ISO_SPEED: u16 = 0x8833;
const FOCAL_LENGTH_35MM: u16 = 0xa405;
const PIXEL_X_DIMENSION: u16 = 0xa002;
const PIXEL_Y_DIMENSION: u16 = 0xa003;
const DATE_TIME: u16 = 0x0132;
const ARTIST: u16 = 0x013b;
const COPYRIGHT: u16 = 0x8298;
const MICROSECONDS_PER_SECOND: f64 = 1_000_000.;
const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
// Last second representable by EXIF's four-digit year.
const MAX_TIMESTAMP: u64 = 253_402_300_799;

pub(crate) struct Undefined<'a>(pub &'a [u8]);
impl TiffValue for Undefined<'_> {
    const BYTE_LEN: u8 = 1;
    const FIELD_TYPE: Type = Type::UNDEFINED;
    fn count(&self) -> usize {
        self.0.len()
    }
    fn data(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(self.0)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ExifOutput {
    Tiff(TiffColorSpace),
    Jpeg(TiffColorSpace),
    Embedded(TiffColorSpace),
    Dng,
}
impl ExifOutput {
    fn color_space(self) -> u16 {
        match self {
            Self::Tiff(TiffColorSpace::Srgb)
            | Self::Jpeg(TiffColorSpace::Srgb)
            | Self::Embedded(TiffColorSpace::Srgb) => 1,
            _ => 65535,
        }
    }
}

/// Safe standard fields retained from the embedded JPEG, grouped by IFD.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct EmbeddedTag {
    pub ifd: u8,
    pub id: u16,
    pub kind: u16,
    pub count: usize,
    pub data: Vec<u8>,
}

impl EmbeddedTag {
    pub(crate) fn normalize_unique_id(&mut self) {
        // Some camera previews pad this fixed-size ASCII value with extra NULs.
        if (self.ifd, self.id, self.kind) == (1, 0xa420, 2)
            && self.count == self.data.len()
            && self.data.len() >= 33
            && self.data[..32].iter().all(u8::is_ascii_hexdigit)
            && self.data[32..].iter().all(|b| *b == 0)
        {
            self.data.truncate(33);
            self.count = 33;
        }
    }

    pub(crate) fn valid(&self) -> bool {
        if !crate::x3f::exif::retained(self.ifd, self.id) {
            return false;
        }
        let unit: usize = match self.kind {
            1 | 2 | 7 => 1,
            3 => 2,
            4 | 9 => 4,
            5 | 10 => 8,
            _ => return false,
        };
        if self.count == 0
            || self.data.len() > 65_536
            || self.count.checked_mul(unit) != Some(self.data.len())
        {
            return false;
        }
        if self.kind == 2 && (self.data.last() != Some(&0) || !self.data.is_ascii()) {
            return false;
        }
        if matches!(self.kind, 5 | 10)
            && self
                .data
                .as_chunks::<8>()
                .0
                .iter()
                .any(|x| x[4..] == [0; 4])
        {
            return false;
        }
        if self.ifd == 1
            && matches!(self.id, EXPOSURE_TIME | F_NUMBER | FOCAL_LENGTH)
            && self.data.get(..4).is_some_and(|n| n == [0; 4])
        {
            return false;
        }
        if self.ifd == 1
            && matches!(self.id, ISO_SPEED_RATINGS | ISO_SPEED)
            && self.data.iter().all(|b| *b == 0)
        {
            return false;
        }
        if matches!(
            (self.ifd, self.id),
            (0, 0x010f | 0x0110) | (1, BODY_SERIAL_NUMBER | LENS_MODEL)
        ) && std::str::from_utf8(&self.data)
            .is_ok_and(|s| s.split('\0').next().unwrap_or("").trim().is_empty())
        {
            return false;
        }
        let expected = match (self.ifd, self.id) {
            (0, 0x9c9b..=0x9c9f) => (1, None),
            (0, _) => (2, None),
            (
                1,
                0x829a | 0x829d | 0x9202 | 0x9205 | 0x9206 | 0x920a | 0x9401 | 0x9402 | 0x9404
                | 0xa20b | 0xa20e | 0xa20f | 0xa215 | 0xa404,
            ) => (5, Some(1)),
            (1, 0x9201 | 0x9203 | 0x9204 | 0x9400 | 0x9403 | 0x9405) => (10, Some(1)),
            (1, 0xa432) => (5, Some(4)),
            (1, 0xa461) => (3, Some(2)),
            (1, 0xa462) => (7, None),
            (
                1,
                0x8822
                | 0x8830
                | 0x9207..=0x9209
                | 0xa210
                | 0xa217
                | 0xa402
                | 0xa403
                | 0xa405..=0xa40a
                | 0xa40c
                | 0xa460,
            ) => (3, Some(1)),
            (1, 0x8827) => (3, None),
            (1, 0x8831..=0x8835) => (4, Some(1)),
            (1, 0x9000) => (7, Some(4)),
            (1, 0xa300 | 0xa301) => (7, Some(1)),
            (1, 0xa420) => (2, Some(33)),
            (1, 0x8828 | 0x9286 | 0xa20c | 0xa40b) => (7, None),
            (1, _) => (2, None),
            (2, 0) => (1, Some(4)),
            (2, 5) => (1, Some(1)),
            (2, 2 | 4 | 7 | 0x14 | 0x16) => (5, Some(3)),
            (2, 6 | 0xb | 0xd | 0xf | 0x11 | 0x18 | 0x1a | 0x1f) => (5, Some(1)),
            (2, 1 | 3 | 9 | 0xa | 0xc | 0xe | 0x10 | 0x13 | 0x15 | 0x17 | 0x19) => (2, Some(2)),
            (2, 0x1b | 0x1c) => (7, None),
            (2, 0x1d) => (2, Some(11)),
            (2, 0x1e) => (3, Some(1)),
            (2, _) => (2, None),
            _ => return false,
        };
        if self.kind != expected.0 || expected.1.is_some_and(|n| self.count != n) {
            return false;
        }
        if self.ifd == 1 && self.id == 0xa420 && !self.data[..32].iter().all(u8::is_ascii_hexdigit)
        {
            return false;
        }
        if self.ifd == 1 && self.id == EXIF_VERSION && !self.data.iter().all(u8::is_ascii_digit) {
            return false;
        }
        if matches!((self.ifd, self.id), (0, 0x0132) | (1, 0x9003 | 0x9004)) {
            return self.count == 20 && std::str::from_utf8(&self.data[..19]).is_ok_and(valid_date);
        }
        if self.ifd == 1 && self.id == 0x9286 {
            let Some(prefix) = self.data.get(..8) else {
                return false;
            };
            return match prefix {
                b"ASCII\0\0\0" => self.data[8..].is_ascii(),
                b"UNICODE\0" => {
                    let body = &self.data[8..];
                    if !body.len().is_multiple_of(2) {
                        return false;
                    }
                    let big = match body.get(..2) {
                        Some([0xfe, 0xff]) => true,
                        Some([0xff, 0xfe]) => false,
                        _ => cfg!(target_endian = "big"),
                    };
                    char::decode_utf16(body.as_chunks::<2>().0.iter().map(|b| {
                        let word = [b[0], b[1]];
                        if big {
                            u16::from_be_bytes(word)
                        } else {
                            u16::from_le_bytes(word)
                        }
                    }))
                    .all(|c| c.is_ok())
                }
                b"JIS\0\0\0\0\0" | b"\0\0\0\0\0\0\0\0" => true,
                _ => false,
            };
        }
        true
    }

    fn reduce_rationals(&mut self) {
        if !matches!(self.kind, 5 | 10) {
            return;
        }
        for pair in self.data.as_chunks_mut::<8>().0 {
            let n = u32::from_ne_bytes(pair[..4].try_into().unwrap());
            let d = u32::from_ne_bytes(pair[4..].try_into().unwrap());
            let (n, d) = if self.kind == 5 {
                let g = gcd(n, d);
                (n / g, d / g)
            } else {
                let (n, d) = (i64::from(n as i32), i64::from(d as i32));
                let g = i64::from(gcd(n.unsigned_abs() as u32, d.unsigned_abs() as u32));
                let (mut n, mut d) = (n / g, d / g);
                if n == 0 {
                    d = 1;
                } else if d < 0 && -n <= i64::from(i32::MAX) && -d <= i64::from(i32::MAX) {
                    n = -n;
                    d = -d;
                }
                (n as i32 as u32, d as i32 as u32)
            };
            pair[..4].copy_from_slice(&n.to_ne_bytes());
            pair[4..].copy_from_slice(&d.to_ne_bytes());
        }
    }
}

macro_rules! raw_value {
    ($name:ident, $kind:ident) => {
        struct $name<'a>(&'a EmbeddedTag);
        impl TiffValue for $name<'_> {
            const BYTE_LEN: u8 = 1;
            const FIELD_TYPE: Type = Type::$kind;
            fn count(&self) -> usize {
                self.0.count
            }
            fn data(&self) -> Cow<'_, [u8]> {
                Cow::Borrowed(&self.0.data)
            }
            fn bytes(&self) -> usize {
                self.0.data.len()
            }
        }
    };
}
raw_value!(RawByte, BYTE);
raw_value!(RawAscii, ASCII);
raw_value!(RawShort, SHORT);
raw_value!(RawLong, LONG);
raw_value!(RawRational, RATIONAL);
raw_value!(RawUndefined, UNDEFINED);
raw_value!(RawSlong, SLONG);
raw_value!(RawSrational, SRATIONAL);

fn write_embedded<W: Write + Seek>(
    directory: &mut Directory<'_, W>,
    entry: &EmbeddedTag,
) -> Result<()> {
    match entry.kind {
        1 => tag(directory, entry.id, RawByte(entry)),
        2 => tag(directory, entry.id, RawAscii(entry)),
        3 => tag(directory, entry.id, RawShort(entry)),
        4 => tag(directory, entry.id, RawLong(entry)),
        5 => tag(directory, entry.id, RawRational(entry)),
        7 => tag(directory, entry.id, RawUndefined(entry)),
        9 => tag(directory, entry.id, RawSlong(entry)),
        10 => tag(directory, entry.id, RawSrational(entry)),
        _ => Ok(()),
    }
}

pub(crate) struct Capture<'a> {
    properties: &'a BTreeMap<String, String>,
    embedded: Vec<EmbeddedTag>,
    facts: CaptureFacts,
}

/// A property's text as EXIF can store it: trimmed, ASCII, without NULs.
fn property_text<'a>(properties: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    properties
        .get(key)
        .map(|v| v.trim())
        .filter(|v| !v.is_empty() && v.is_ascii() && !v.contains('\0'))
}

/// The camera's own EXIF fields retained from the embedded JPEG, if any.
pub(crate) fn embedded_tags(properties: &BTreeMap<String, String>) -> Vec<EmbeddedTag> {
    let embedded: Vec<EmbeddedTag> = properties
        .get("EXIF_CAPTURE_TAGS")
        .filter(|s| s.len() <= 1_000_000)
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    embedded
        .into_iter()
        .map(|mut entry| {
            entry.normalize_unique_id();
            entry
        })
        .filter(EmbeddedTag::valid)
        .map(|mut e| {
            e.reduce_rationals();
            e
        })
        .collect()
}

impl<'a> Capture<'a> {
    pub fn new(metadata: &'a Metadata) -> Self {
        let properties = &metadata.properties;
        let embedded = embedded_tags(properties);
        Self {
            properties,
            facts: CaptureFacts::resolve(properties, &embedded),
            embedded,
        }
    }

    pub fn payload(&self, size: [u32; 2], output: ExifOutput) -> Result<Vec<u8>> {
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut encoder = TiffEncoder::new(&mut out)?;
            let exif = self.write_exif(&mut encoder, size, output)?;
            let mut directory = encoder.image_directory()?;
            self.write_primary(&mut directory, exif)?;
            tag(&mut directory, 274, 1_u16)?;
            tag(&mut directory, 305, "Foveonic GPU")?;
            if matches!(output, ExifOutput::Jpeg(_)) {
                tag(&mut directory, 0x0213, 1_u16)?;
            }
            directory.finish()?;
        }
        Ok(out.into_inner())
    }

    fn text(&self, key: &str) -> Option<&str> {
        property_text(self.properties, key)
    }
    pub(crate) fn facts(&self) -> &CaptureFacts {
        &self.facts
    }
    fn has(&self, ifd: u8, id: u16) -> bool {
        self.embedded
            .iter()
            .any(|entry| entry.ifd == ifd && entry.id == id)
    }
    pub fn write_exif<W: Write + Seek>(
        &self,
        encoder: &mut TiffEncoder<W>,
        size: [u32; 2],
        output: ExifOutput,
    ) -> Result<(u32, Option<u32>)> {
        let gps = if self.embedded.iter().any(|entry| entry.ifd == 2) {
            let mut directory = encoder.extra_directory()?;
            for entry in self.embedded.iter().filter(|entry| entry.ifd == 2) {
                write_embedded(&mut directory, entry)?;
            }
            Some(directory.finish_with_offsets()?.offset)
        } else {
            None
        };
        let mut directory = encoder.extra_directory()?;
        for entry in self.embedded.iter().filter(|entry| entry.ifd == 1) {
            write_embedded(&mut directory, entry)?;
        }
        let v3 = self.facts.firmware.is_some()
            || self
                .embedded
                .iter()
                .any(|e| e.ifd == 1 && (0xa436..=0xa439).contains(&e.id));
        let minimum = if v3 {
            b"0300"
        } else if self
            .embedded
            .iter()
            .any(|e| e.ifd == 1 && (0xa460..=0xa462).contains(&e.id))
        {
            b"0232"
        } else {
            b"0230"
        };
        let version = self
            .embedded
            .iter()
            .find(|e| e.ifd == 1 && e.id == EXIF_VERSION);
        if version.is_none_or(|e| e.data.as_slice() < minimum.as_slice()) {
            tag(&mut directory, EXIF_VERSION, Undefined(minimum))?;
        }
        tag(&mut directory, 0xa000, Undefined(b"0100"))?;
        tag(&mut directory, 0xa001, output.color_space())?;
        if matches!(output, ExifOutput::Jpeg(_)) {
            tag(&mut directory, 0x9101, Undefined(&[1, 2, 3, 0]))?;
        }
        for (id, value) in [
            (EXPOSURE_TIME, self.facts.exposure_seconds),
            (F_NUMBER, self.facts.f_number),
            (FOCAL_LENGTH, self.facts.focal_length),
        ] {
            if self.has(1, id) {
                continue;
            }
            if let Some(values) = value
                .filter(|v| *v > 0.)
                .and_then(|v| unsigned_rationals(&[v]).ok())
            {
                tag(&mut directory, id, values.as_slice())?;
            }
        }
        if let Some(values) = self
            .facts
            .exposure_bias
            .and_then(|v| signed_rationals(&[v]).ok())
            .filter(|_| !self.has(1, EXPOSURE_BIAS))
        {
            tag(&mut directory, EXPOSURE_BIAS, values.as_slice())?;
        }
        if let Some(iso) = self.facts.iso.filter(|_| !self.has(1, ISO_SPEED_RATINGS)) {
            tag(
                &mut directory,
                ISO_SPEED_RATINGS,
                iso.min(u16::MAX as u32) as u16,
            )?;
            if !self.has(1, SENSITIVITY_TYPE) {
                tag(&mut directory, SENSITIVITY_TYPE, 3_u16)?;
            }
            if !self.has(1, ISO_SPEED) {
                tag(&mut directory, ISO_SPEED, iso)?;
            }
        }
        if let Some(date) = &self.facts.taken {
            for id in [DATE_TIME_ORIGINAL, DATE_TIME_DIGITIZED] {
                if !self
                    .embedded
                    .iter()
                    .any(|entry| entry.ifd == 1 && entry.id == id)
                {
                    tag(&mut directory, id, date.as_str())?;
                }
            }
        }
        if let Some(program) = self.facts.exposure_program {
            if !self.has(1, EXPOSURE_PROGRAM) {
                tag(&mut directory, EXPOSURE_PROGRAM, program)?;
            }
        }
        if let Some(mode) = self.facts.metering {
            if !self.has(1, METERING_MODE) {
                tag(&mut directory, METERING_MODE, mode)?;
            }
        }
        for (id, value) in [
            (BODY_SERIAL_NUMBER, &self.facts.serial),
            (LENS_MODEL, &self.facts.lens),
            (0xa439, &self.facts.firmware),
        ] {
            if let Some(value) = value.as_deref().filter(|_| !self.has(1, id)) {
                tag(&mut directory, id, value)?;
            }
        }
        if let Some(value) = self
            .facts
            .focal_length_35mm
            .filter(|_| !self.has(1, FOCAL_LENGTH_35MM))
        {
            tag(&mut directory, FOCAL_LENGTH_35MM, value)?;
        }
        if let Some(lens) = self
            .facts
            .lens_range
            .filter(|_| !self.has(1, LENS_SPECIFICATION))
        {
            if let Ok(mut values) = unsigned_rationals(&lens.focal) {
                // EXIF records an unknown f-number as 0/0.
                values.extend(lens.widest.map(|v| {
                    v.and_then(|v| unsigned_rationals(&[v]).ok()?.pop())
                        .unwrap_or(Rational { n: 0, d: 0 })
                }));
                tag(&mut directory, LENS_SPECIFICATION, values.as_slice())?;
            }
        }
        if let Some(wb) = self.text("WB_DESC") {
            let setting = match wb.to_ascii_lowercase().as_str() {
                "auto" => Some((0_u16, None)),
                "sunlight" | "daylight" => Some((1, Some(1_u16))),
                "fluorescent" => Some((1, Some(2))),
                "incandescent" => Some((1, Some(3))),
                "flash" => Some((1, Some(4))),
                "cloudy" | "overcast" => Some((1, Some(10))),
                "shade" => Some((1, Some(11))),
                "custom" => Some((1, None)),
                _ => None,
            };
            if let Some((white_balance, light_source)) = setting {
                if !self.embedded.iter().any(|e| e.ifd == 1 && e.id == 0xa403) {
                    tag(&mut directory, 0xa403, white_balance)?;
                }
                if let Some(value) = light_source {
                    if !self.embedded.iter().any(|e| e.ifd == 1 && e.id == 0x9208) {
                        tag(&mut directory, 0x9208, value)?;
                    }
                }
            }
        }
        // EXIF geometry describes the exported, already-oriented image.
        if !matches!(output, ExifOutput::Tiff(_)) {
            tag(&mut directory, PIXEL_X_DIMENSION, size[0])?;
            tag(&mut directory, PIXEL_Y_DIMENSION, size[1])?;
        }
        Ok((directory.finish_with_offsets()?.offset, gps))
    }

    pub fn write_primary<W: Write + Seek>(
        &self,
        directory: &mut Directory<'_, W>,
        offsets: (u32, Option<u32>),
    ) -> Result<()> {
        for entry in self.embedded.iter().filter(|entry| entry.ifd == 0) {
            write_embedded(directory, entry)?;
        }
        tag(directory, EXIF_IFD, offsets.0)?;
        if let Some(gps) = offsets.1 {
            tag(directory, 0x8825, gps)?;
        }
        if !self.has(0, 271) {
            tag(directory, 271, self.facts.make.as_str())?;
        }
        if let Some(model) = self.facts.model.as_deref().filter(|_| !self.has(0, 272)) {
            tag(directory, 272, model)?;
        }
        if let Some(date) = self
            .facts
            .taken
            .as_ref()
            .filter(|_| !self.has(0, DATE_TIME))
        {
            tag(directory, DATE_TIME, date.as_str())?;
        }
        for (id, key) in [(ARTIST, "ARTIST"), (COPYRIGHT, "COPYRIGHT")] {
            if let Some(value) = self.text(key).filter(|_| !self.has(0, id)) {
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
#[path = "capture_metadata_test.rs"]
mod tests;
