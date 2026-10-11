//! Resolves metadata-derived calibration, white balance, camera color, and image geometry.
//!
//! Gain factors, spatial-table selection, shading geometry, and ISO scaling follow Foveonic's metadata preparation.

use crate::source::{Metadata, Result};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CalibrationParams {
    pub source: [u32; 4],
    pub dark: [u32; 4],
    pub gains: [f32; 4],
    pub shading: [f32; 4],
    pub shading_geometry: [f32; 4],
    pub grids: [[u32; 4]; 3],
}

pub struct Calibration {
    pub params: CalibrationParams,
    pub grid_values: Vec<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorSpace {
    Srgb,
    AdobeRgb,
}

impl ColorSpace {
    pub fn from_metadata(metadata: &Metadata) -> Result<Self> {
        match metadata.properties.get("COLORSPACE").map(String::as_str) {
            Some("sRGB") => Ok(Self::Srgb),
            Some("AdobeRGB") => Ok(Self::AdobeRgb),
            _ => Err("unsupported camera color space".into()),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ColorParams {
    pub source: [u32; 4],
    pub matrix: [[f32; 4]; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EncodeParams {
    pub source: [u32; 4],
    pub crop: [u32; 4],
    pub output: [u32; 4],
}

pub fn rectangle(metadata: &Metadata, name: &str) -> Result<[u32; 4]> {
    let values = metadata.fixed::<4>(name)?;
    if values
        .iter()
        .any(|v| *v < 0. || v.fract() != 0. || *v >= u32::MAX as f64)
    {
        return Err(format!("invalid {name} rectangle").into());
    }
    let [left, top, right, bottom] = values.map(|v| v as u32);
    if left > right
        || top > bottom
        || right >= metadata.width as u32
        || bottom >= metadata.height as u32
    {
        return Err(format!("invalid {name} bounds").into());
    }
    Ok([left, top, right + 1, bottom + 1])
}

pub fn prepare(metadata: &Metadata, wb: &str) -> Result<Calibration> {
    let aperture: f32 = metadata
        .properties
        .get("APERTURE")
        .ok_or("missing aperture")?
        .parse()?;
    let tables = crate::spatial_tables::prepare(metadata, aperture)?;
    let key = metadata
        .lists
        .get("WhiteBalanceGains")
        .and_then(|list| list.get(wb))
        .ok_or_else(|| format!("unavailable white balance {wb}"))?;
    let raw = metadata.fixed::<3>(key)?;
    let optional = |name: &str| -> Result<[f64; 3]> {
        if metadata.matrix(name).is_some() {
            metadata.fixed(name)
        } else {
            Ok([1.; 3])
        }
    };
    let fnumber = optional("FnumberGainFact")?;
    let mut temperature = optional("TempGainFact")?;
    if temperature.iter().any(|v| !(0.5..=1.5).contains(v)) {
        temperature = [1.; 3];
    }
    let sensor = optional("SensorAdjustmentGainFact")?;
    let gains: [f32; 3] =
        std::array::from_fn(|c| (2. * raw[c] * fnumber[c] * temperature[c] * sensor[c]) as f32);
    if gains.iter().any(|v| !v.is_finite() || *v <= 0.) {
        return Err("invalid white-balance factors".into());
    }
    let shading = metadata
        .lists
        .get("WhiteBalanceColorShadingFactor")
        .and_then(|list| list.get(wb))
        .filter(|key| metadata.matrix(key).is_some())
        .map(|key| metadata.fixed::<4>(key))
        .transpose()?
        .unwrap_or([0.; 4]);
    let active = rectangle(metadata, "ActiveImageArea")?;
    let columns = metadata.fixed::<4>("DarkShieldColRange")?;
    let left = columns[1] as f32 + 6.;
    let right = columns[2] as f32 - 8.;
    let top = active[1] as f32 - 32.;
    let bottom = active[3] as f32 + 32.;
    let half_width = ((right - left) * 0.5).floor();
    let half_height = ((bottom - top) * 0.5).floor();
    let radius = half_width.hypot(half_height);
    if radius <= 0. || !radius.is_finite() {
        return Err("invalid shading geometry".into());
    }
    let dark = rectangle(metadata, "DarkShieldTop")?;
    let dark_count = (dark[2] - dark[0]) * (dark[3] - dark[1]);
    // Histogram sums use u32 and raw Merrill samples are twelve-bit integers.
    if !(3..=u32::MAX / 4095).contains(&dark_count) {
        return Err("unsupported dark-shield count".into());
    }
    let mut grid_values = Vec::new();
    let mut grids = [[0; 4]; 3];
    for (c, table) in tables.into_iter().enumerate() {
        grids[c] = [
            grid_values.len() as u32,
            table.width as u32,
            table.height as u32,
            0,
        ];
        grid_values.extend(table.values);
    }
    Ok(Calibration {
        params: CalibrationParams {
            source: [metadata.width as u32, metadata.height as u32, dark_count, 0],
            dark,
            gains: [gains[0], gains[1], gains[2], 0.],
            shading: shading.map(|v| v as f32),
            shading_geometry: [left + half_width, top + half_height, radius, 0.],
            grids,
        },
        grid_values,
    })
}

pub fn color(
    metadata: &Metadata,
    wb: &str,
    mode: &crate::color_mode::Settings,
) -> Result<ColorParams> {
    let key = metadata
        .lists
        .get("WhiteBalanceColorCorrections")
        .and_then(|list| list.get(wb))
        .ok_or("missing white-balance color matrix")?;
    let capture = match metadata
        .matrix("CaptureISO")
        .map(|m| m.values[0])
        .filter(|v| *v > 0.)
    {
        Some(value) => value,
        None => metadata
            .properties
            .get("ISO")
            .ok_or("missing ISO")?
            .parse::<f64>()?,
    };
    let mut processing_iso = capture;
    if metadata
        .matrix("ISOrange")
        .is_some_and(|m| m.values[0] != 0.)
    {
        processing_iso = match capture {
            800. => 666.,
            1600. => 1200.,
            3200. => 2133.,
            6400. => 4266.,
            _ => capture,
        };
    }
    let sensor_iso = metadata.matrix("SensorISO").map_or(100., |m| {
        if m.values[0] == 0. {
            100.
        } else {
            m.values[0]
        }
    });
    let matrix = mode.camera_matrix(
        metadata
            .fixed::<9>(key)?
            .map(|v| v * processing_iso / sensor_iso),
    );
    if matrix.iter().any(|v| !v.is_finite()) {
        return Err("invalid camera color matrix".into());
    }
    Ok(ColorParams {
        source: [metadata.width as u32, metadata.height as u32, 0, 0],
        matrix: std::array::from_fn(|row| {
            [
                matrix[row * 3] as f32,
                matrix[row * 3 + 1] as f32,
                matrix[row * 3 + 2] as f32,
                0.,
            ]
        }),
    })
}

pub fn encode(metadata: &Metadata, space: ColorSpace) -> Result<EncodeParams> {
    let crop = rectangle(metadata, "ActiveImageArea")?;
    let rotation = metadata.rotation % 360;
    if ![0, 90, 180, 270].contains(&rotation) {
        return Err("unsupported image orientation".into());
    }
    let (w, h) = (crop[2] - crop[0], crop[3] - crop[1]);
    let (width, height) = if rotation == 90 || rotation == 270 {
        (h, w)
    } else {
        (w, h)
    };
    Ok(EncodeParams {
        source: [metadata.width as u32, metadata.height as u32, rotation, 0],
        crop,
        output: [
            width,
            height,
            if space == ColorSpace::Srgb { 0 } else { 1 },
            0,
        ],
    })
}
