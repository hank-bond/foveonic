//! Checks local color guards against known bad references and supported weak colored details.

use super::trials::{stats, Study, Texture};
use super::*;
use crate::{
    gpu::chroma_investigation::{Inputs, Kernels, Uniforms},
    nodes::color::texture,
};

fn params() -> Parameters {
    Parameters {
        source: [64, 64, 1, 1],
        tile: [64, 64, 0, 11],
        area: [0, 0, 64, 64],
        filter: [0.08, 1., 0.6, 0.92],
        edge: [1., 10., 5., 100.],
        weights: [1. / 3., 1. / 3., 1. / 3., 0.],
        model: [[16., 16., 16., 0.], [0.; 4], [0.; 4]],
        mono: [1.; 4],
        diffusion: [0.25, 1., 2., 0.],
        column_area: [0, 0, 64, 64],
        column: [0.375, 27., 0., 0.],
        reduction: [1., 0., 0., 0.],
        geometry: [8, 8, 21, 0],
        window: [0, 0, 1, 1],
        region: [0, 0, 64, 64],
    }
}
#[test]
#[ignore = "requires GPU and an explicit synthetic-results path"]
fn local_reference_known_color_controls() -> Result<()> {
    let output = PathBuf::from(
        std::env::var_os("FOVEONIC_CHROMA_CONTROLS").ok_or("synthetic-results path")?,
    );
    if output.exists() {
        return Err("choose a new synthetic-results path".into());
    }
    let grid = std::env::var_os("FOVEONIC_CHROMA_CONTROL_GRID")
        .map(std::fs::read)
        .transpose()?
        .map(|bytes| serde_json::from_slice::<Value>(&bytes))
        .transpose()?;
    let variants: Vec<(String, u32, [f32; 4], [f32; 4])> = if let Some(config) = &grid {
        config["candidates"]
            .as_array()
            .ok_or("grid candidates")?
            .iter()
            .map(|c| {
                Ok((
                    c["id"].as_str().ok_or("candidate id")?.to_owned(),
                    c["mode"].as_u64().ok_or("mode")? as u32,
                    serde_json::from_value(c["limits"].clone())?,
                    serde_json::from_value(c["reliability"].clone())?,
                ))
            })
            .collect::<Result<_>>()?
    } else {
        (0..6)
            .map(|mode| {
                (
                    format!("mode-{mode}"),
                    mode,
                    [2., 6., 1., 4.],
                    [0.04, 0.14, 1., 0.],
                )
            })
            .collect()
    };
    let gpu = Gpu::new()?;
    let kernels = Kernels::new(&gpu);
    let p = params();
    let noise = gpu.uniform(&p);
    let controls = crate::noise::Controls {
        chroma: 4,
        luminance: 0,
        ..Default::default()
    };
    let texture = gpu.uniform(&Texture {
        strength: controls.merrill_effects().strength,
        steps: [1., 2., 4., 8.],
    });
    let mut results = Vec::new();
    let mut scenes = vec![
        "unsupported-reference",
        "dim-green-point",
        "dim-blue-point",
        "colored-edge",
        "brightness-grain",
        "bright-color",
    ];
    if grid.is_some() {
        scenes.extend([
            "noisy-green-point",
            "noisy-blue-point",
            "noisy-colored-edge",
        ]);
    }
    for name in scenes {
        let mut base = vec![1_f32; 64 * 64 * 3];
        let mut raw = vec![20_f32; 64 * 64 * 3];
        for y in 0..64 {
            for x in 0..64 {
                let i = (y * 64 + x) * 3;
                let b = match name {
                    "colored-edge" | "noisy-colored-edge" => {
                        if x > 32 {
                            [0.5, 1.4, 1.1]
                        } else {
                            [1.6, 0.7, 0.7]
                        }
                    }
                    "bright-color" => [80., 140., 80.],
                    _ => [1.; 3],
                };
                let gain = if name == "brightness-grain" {
                    10. + 3. * (x as f32 * 0.73 + y as f32 * 0.31).sin()
                } else if name == "bright-color" {
                    1.
                } else {
                    20.
                };
                for c in 0..3 {
                    base[i + c] = b[c];
                    raw[i + c] = b[c] * gain;
                    if name == "noisy-colored-edge" {
                        raw[i + c] +=
                            8. * (x as f32 * 0.73 + y as f32 * 0.31 + c as f32 * 2.1).sin();
                    }
                }
            }
        }
        let i = (32 * 64 + 32) * 3;
        match name {
            "unsupported-reference" => {
                base[i..i + 3].copy_from_slice(&[0.5, 1.7, 0.8]);
                raw[i..i + 3].copy_from_slice(&[30., 8., 25.]);
            }
            "dim-green-point" => {
                base[i..i + 3].copy_from_slice(&[0.7, 1.6, 0.7]);
                raw[i..i + 3].copy_from_slice(&[7., 16., 7.]);
            }
            "dim-blue-point" => {
                base[i..i + 3].copy_from_slice(&[0.8, 0.8, 1.4]);
                raw[i..i + 3].copy_from_slice(&[8., 8., 14.]);
            }
            "noisy-green-point" => {
                base[i..i + 3].copy_from_slice(&[0.7, 1.6, 0.7]);
                raw[i..i + 3].copy_from_slice(&[12., 12., 12.]);
            }
            "noisy-blue-point" => {
                base[i..i + 3].copy_from_slice(&[0.8, 0.8, 1.4]);
                raw[i..i + 3].copy_from_slice(&[12., 12., 12.]);
            }
            _ => {}
        }
        let input = gpu.upload("Known conditioned layers", &raw)?;
        let reference = gpu.upload("Known filtered layers", &base)?;
        let ratio = gpu.upload("Unused cleanup ratio", &vec![1_f32; 64 * 64])?;
        let original = kernels.baseline(
            &gpu,
            texture::Inputs {
                raw: &input,
                base: Some(&reference),
                ratio: Some(&ratio),
            },
            &noise,
            &texture,
            p.atlas_extent(),
        )?;
        let original_values: Vec<f32> =
            bytemuck::cast_slice::<u8, f32>(&gpu.readback(&original)?).to_vec();
        for (id, mode, limits, reliability) in &variants {
            let mode = *mode;
            let study = gpu.uniform(&Study {
                selection: [mode, 2, 0, 0],
                active: [0, 0, 64, 64],
                rgb_y: p.weights,
                neutral_y: [1., 1., 1., 0.],
                limits: *limits,
                reliability: *reliability,
            });
            let (corrected, mask) = kernels.apply(
                &gpu,
                Inputs {
                    raw: &input,
                    base: &reference,
                    ratio: &ratio,
                    original: &original,
                },
                Uniforms {
                    noise: &noise,
                    texture: &texture,
                    study: &study,
                },
                [64, 64],
            )?;
            let summary = stats(&gpu, &kernels, &original, &corrected, &mask, &noise, &study)?;
            let values: Vec<f32> =
                bytemuck::cast_slice::<u8, f32>(&gpu.readback(&corrected)?).to_vec();
            assert!(summary["max_weighted_luma_change"].as_f64().unwrap() < 0.0001);
            if mode == 0 {
                assert_eq!(values, original_values);
            }
            if mode >= 3 && name != "unsupported-reference" && !name.starts_with("noisy-") {
                assert_eq!(
                    values, original_values,
                    "supported structure changed: {name}, mode {mode}"
                );
            }
            if grid.is_none() && mode >= 3 && name == "unsupported-reference" {
                let pixel: [f32; 3] = values[i..i + 3].try_into().unwrap();
                let maximum = if mode == 5 {
                    green(original_values[i..i + 3].try_into().unwrap()).abs() * 0.1
                } else {
                    0.0001
                };
                assert!(
                    green(pixel).abs() < maximum,
                    "bad color reference was not reduced: {name}, mode {mode}, {pixel:?}"
                );
                assert_eq!(summary["changed_atlas_pixels"], 1);
            }
            let original_center = &original_values[i..i + 3];
            let corrected_center = &values[i..i + 3];
            let chroma = |v: &[f32]| {
                let mean = v.iter().sum::<f32>() / 3.;
                v.iter().map(|x| (x - mean).powi(2)).sum::<f32>().sqrt()
            };
            let center_chroma_retained = if chroma(original_center) > 0.00001 {
                Some(chroma(corrected_center) / chroma(original_center))
            } else {
                None
            };
            results.push(json!({"scene":name,"candidate":id,"mode":mode,"limits":limits,"reliability":reliability,"original_center":original_center,"corrected_center":corrected_center,"center_chroma_retained":center_chroma_retained,"technical":summary}));
        }
        assert_eq!(bytemuck::cast_slice::<u8, f32>(&gpu.readback(&input)?), raw);
        assert_eq!(
            bytemuck::cast_slice::<u8, f32>(&gpu.readback(&reference)?),
            base
        );
    }
    std::fs::write(
        output,
        serde_json::to_vec_pretty(
            &json!({"fixture_chroma":4,"scope":"Known inputs at the texture-restoration boundary with a fixed Chroma-4 original, not a full sensor simulation. Mode-zero entries test unchanged shader copies of that original rather than separate Chroma amounts. Supported point and edge colors share input/reference chromaticity. Noisy-color controls deliberately make the filtered color correct while the input disagrees. These expose ambiguity rather than establish the frequency of that case in real photographs.","results":results}),
        )?,
    )?;
    Ok(())
}
