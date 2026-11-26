//! Resolves a photograph's capture details once, for EXIF export, the app's readout and Spotlight.

use super::{
    capture_date, embedded_tags, property_text, valid_date, EmbeddedTag, BODY_SERIAL_NUMBER,
    DATE_TIME_ORIGINAL, EXPOSURE_BIAS, EXPOSURE_PROGRAM, EXPOSURE_TIME, FOCAL_LENGTH,
    FOCAL_LENGTH_35MM, F_NUMBER, ISO_SPEED, ISO_SPEED_RATINGS, LENS_MODEL, METERING_MODE,
    MICROSECONDS_PER_SECOND,
};
use std::collections::BTreeMap;

const MAKE: u16 = 0x010f;
const MODEL: u16 = 0x0110;
/// How close the focal length used must be to a zoom's end for the recorded widest
/// aperture to count as that end's, in millimetres.
const ZOOM_END_TOLERANCE_MM: f64 = 0.5;

/// Capture details as recorded. The camera's own EXIF, embedded in Quattro previews,
/// wins over the X3F properties. Absent values are None.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CaptureFacts {
    pub make: String,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub firmware: Option<String>,
    pub lens: Option<String>,
    /// From the X3F properties; the camera's own EXIF carries its lens specification.
    pub lens_range: Option<LensRange>,
    /// Millimetres.
    pub focal_length: Option<f64>,
    pub focal_length_35mm: Option<u16>,
    pub f_number: Option<f64>,
    pub exposure_seconds: Option<f64>,
    pub iso: Option<u32>,
    /// The frame's exposure bias in EV, including any bracket offset.
    pub exposure_bias: Option<f64>,
    /// EXIF exposure program code: 1 manual, 2 program, 3 aperture, 4 shutter priority.
    pub exposure_program: Option<u16>,
    /// EXIF metering mode code: 1 average, 2 center-weighted, 5 multi-segment.
    pub metering: Option<u16>,
    /// The camera's wall-clock time in EXIF form, `YYYY:MM:DD HH:MM:SS`.
    pub taken: Option<String>,
}

/// A lens's focal range in millimetres and its widest f-number at the short and long
/// ends, each when known.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LensRange {
    pub focal: [f64; 2],
    pub widest: [Option<f64>; 2],
}

impl LensRange {
    /// X3F records one widest aperture, which on a variable-aperture zoom may hold only
    /// at the focal length used. It is kept for an end only when it certainly applies.
    fn from_record(focal: [f64; 2], apertures: Option<[f64; 2]>, used: Option<f64>) -> Self {
        let prime = focal[0] == focal[1];
        let at = |end: f64| {
            apertures
                .filter(|_| {
                    prime || used.is_some_and(|used| (used - end).abs() < ZOOM_END_TOLERANCE_MM)
                })
                .map(|[widest, _]| widest)
        };
        Self {
            focal,
            widest: focal.map(at),
        }
    }

    /// "30mm F2.8" or "18-50mm", in the style the cameras' own EXIF uses. Only primes
    /// name their widest f-number, so one lens keeps one name. EXIF stores lens names
    /// as ASCII, so the range takes a plain hyphen.
    fn description(&self) -> String {
        let [near, far] = self.focal;
        match self.widest[0] {
            _ if near != far => format!("{near}-{far}mm"),
            Some(widest) => format!("{near}mm F{widest}"),
            None => format!("{near}mm"),
        }
    }
}

impl CaptureFacts {
    pub fn from_properties(properties: &BTreeMap<String, String>) -> Self {
        Self::resolve(properties, &embedded_tags(properties))
    }

    pub(super) fn resolve(properties: &BTreeMap<String, String>, embedded: &[EmbeddedTag]) -> Self {
        let text = |key: &str| property_text(properties, key);
        let number = |key: &str| text(key)?.parse::<f64>().ok().filter(|v| v.is_finite());
        let positive = |key: &str| number(key).filter(|v| *v > 0.);
        let tag = |ifd: u8, id: u16| embedded.iter().find(|e| e.ifd == ifd && e.id == id);
        let tag_text = |ifd, id| tag(ifd, id).and_then(ascii);
        let tag_number = |id| tag(1, id).and_then(first_number);
        let shutter = text("SH_DESC")
            .and_then(|v| match v.split_once('/') {
                Some((n, d)) => Some(n.trim().parse::<f64>().ok()? / d.trim().parse::<f64>().ok()?),
                None => v.trim_end_matches('"').parse().ok(),
            })
            .filter(|v: &f64| v.is_finite() && *v > 0.)
            .or_else(|| positive("SHUTTER"))
            .or_else(|| positive("EXPTIME").map(|v| v / MICROSECONDS_PER_SECOND));
        let compensation = crate::source::CaptureExposureCompensation::from_properties(properties);
        let focal_length = tag_number(FOCAL_LENGTH)
            .or_else(|| number("FLENGTH"))
            .filter(|v| *v > 0.);
        let lens_range = text("LENSFRANGE").and_then(range).map(|focal| {
            LensRange::from_record(focal, text("LENSARANGE").and_then(range), focal_length)
        });
        Self {
            make: tag_text(0, MAKE)
                .or_else(|| text("CAMMANUF").map(str::to_owned))
                .unwrap_or_else(|| "SIGMA".into()),
            model: tag_text(0, MODEL).or_else(|| text("CAMMODEL").map(str::to_owned)),
            serial: tag_text(1, BODY_SERIAL_NUMBER)
                .or_else(|| text("CAMSERIAL").map(str::to_owned)),
            firmware: text("FIRMVERS").map(str::to_owned),
            // X3F records a lens ID, not a name; its focal and aperture ranges describe it.
            lens: tag_text(1, LENS_MODEL).or_else(|| lens_range.map(|lens| lens.description())),
            lens_range,
            focal_length,
            focal_length_35mm: tag_number(FOCAL_LENGTH_35MM)
                .or_else(|| number("FLEQ35MM"))
                .filter(|v| v.fract() == 0. && (1. ..=f64::from(u16::MAX)).contains(v))
                .map(|v| v as u16),
            f_number: tag_number(F_NUMBER)
                .or_else(|| positive("EXIF_FNUMBER"))
                // The camera's aperture description may read "f/2.8".
                .or_else(|| {
                    text("AP_DESC")?
                        .trim_start_matches("f/")
                        .parse::<f64>()
                        .ok()
                        .filter(|v| v.is_finite() && *v > 0.)
                })
                .or_else(|| positive("APERTURE"))
                .filter(|v| *v > 0.),
            exposure_seconds: tag_number(EXPOSURE_TIME).filter(|v| *v > 0.).or(shutter),
            iso: tag_number(ISO_SPEED)
                .or_else(|| tag_number(ISO_SPEED_RATINGS))
                .or_else(|| number("ISO"))
                .filter(|v| v.fract() == 0. && (1. ..=f64::from(u32::MAX)).contains(v))
                .map(|v| v as u32),
            exposure_bias: tag_number(EXPOSURE_BIAS).or(compensation.net),
            exposure_program: tag_number(EXPOSURE_PROGRAM).map(|v| v as u16).or_else(
                || match text("PMODE")? {
                    "M" => Some(1),
                    "P" => Some(2),
                    "A" => Some(3),
                    "S" => Some(4),
                    _ => None,
                },
            ),
            metering: tag_number(METERING_MODE).map(|v| v as u16).or_else(|| {
                match text("AEMODE")? {
                    "A" => Some(1),
                    "C" => Some(2),
                    "8" => Some(5),
                    _ => None,
                }
            }),
            taken: tag_text(1, DATE_TIME_ORIGINAL)
                .filter(|v| valid_date(v))
                .or_else(|| text("TIME")?.parse().ok().and_then(capture_date))
                .or_else(|| {
                    text("EXIF_DATETIME")
                        .filter(|v| valid_date(v))
                        .map(str::to_owned)
                }),
        }
    }
}

/// A recorded range such as "18 to 50", in either order, or one value.
fn range(text: &str) -> Option<[f64; 2]> {
    let (a, b) = text.split_once(" to ").unwrap_or((text, text));
    let [a, b] = [a, b].map(|v| {
        v.trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v > 0.)
    });
    let [a, b] = [a?, b?];
    Some([a.min(b), a.max(b)])
}

fn ascii(tag: &EmbeddedTag) -> Option<String> {
    let text = std::str::from_utf8(&tag.data)
        .ok()?
        .split('\0')
        .next()?
        .trim();
    (tag.kind == 2 && !text.is_empty()).then(|| text.to_owned())
}

/// The first value of a numeric tag. Embedded data is stored in native byte order.
fn first_number(tag: &EmbeddedTag) -> Option<f64> {
    let word = |at: usize| -> Option<[u8; 4]> { tag.data.get(at..at + 4)?.try_into().ok() };
    let value = match tag.kind {
        3 => f64::from(u16::from_ne_bytes(tag.data.get(..2)?.try_into().ok()?)),
        4 => f64::from(u32::from_ne_bytes(word(0)?)),
        5 => f64::from(u32::from_ne_bytes(word(0)?)) / f64::from(u32::from_ne_bytes(word(4)?)),
        10 => f64::from(i32::from_ne_bytes(word(0)?)) / f64::from(i32::from_ne_bytes(word(4)?)),
        _ => return None,
    };
    value.is_finite().then_some(value)
}

#[cfg(test)]
#[path = "capture_facts_test.rs"]
mod tests;
