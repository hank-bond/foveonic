//! Compares local weak-color candidates through the unchanged photographic continuation and matched display views.

use super::*;
use crate::{
    gpu::chroma_investigation::{Inputs, Kernels, Uniforms},
    nodes::color::texture,
};
use image::ImageEncoder;
use serde::Serialize;
use std::time::Instant;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Study {
    pub(super) selection: [u32; 4],
    pub(super) active: [u32; 4],
    pub(super) rgb_y: [f32; 4],
    pub(super) neutral_y: [f32; 4],
    pub(super) limits: [f32; 4],
    pub(super) reliability: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Texture {
    pub(super) strength: [f32; 4],
    pub(super) steps: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Display {
    source: [u32; 4],
    view: [u32; 4],
    crop: [u32; 4],
    matrix: [[f32; 4]; 3],
}
#[derive(Clone, Copy, Serialize)]
struct Candidate {
    id: &'static str,
    label: &'static str,
    chroma: u32,
    mode: u32,
    radius: u32,
}
const CANDIDATES: [Candidate; 8] = [
    Candidate {
        id: "current2",
        label: "Current · Chroma 2",
        chroma: 2,
        mode: 0,
        radius: 2,
    },
    Candidate {
        id: "current4",
        label: "Current · Chroma 4",
        chroma: 4,
        mode: 0,
        radius: 2,
    },
    Candidate {
        id: "cap",
        label: "Local blend cap",
        chroma: 4,
        mode: 1,
        radius: 2,
    },
    Candidate {
        id: "pooled",
        label: "Local pool · weak signal only",
        chroma: 4,
        mode: 2,
        radius: 2,
    },
    Candidate {
        id: "guarded",
        label: "Local pool · agreement checks",
        chroma: 4,
        mode: 3,
        radius: 2,
    },
    Candidate {
        id: "rgb-lock",
        label: "Local pool · RGB luminance preserved",
        chroma: 4,
        mode: 4,
        radius: 2,
    },
    Candidate {
        id: "neutral-grain",
        label: "Neutral restored grain · RGB luminance preserved",
        chroma: 4,
        mode: 5,
        radius: 2,
    },
    Candidate {
        id: "wide",
        label: "Local pool · wider support",
        chroma: 4,
        mode: 3,
        radius: 4,
    },
];
#[derive(serde::Deserialize, Serialize)]
struct Trial {
    id: String,
    label: String,
    chroma: u32,
    mode: u32,
    radius: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default = "default_limits")]
    limits: [f32; 4],
    #[serde(default = "default_reliability")]
    reliability: [f32; 4],
}
fn default_limits() -> [f32; 4] {
    [2., 6., 1., 4.]
}
fn default_reliability() -> [f32; 4] {
    [0.04, 0.14, 1., 0.]
}
fn neutral_axis(matrix: [[f32; 3]; 3], weights: [f32; 4]) -> Result<[f32; 4]> {
    let determinant = |m: [[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let matrix = matrix.map(|row| row.map(f64::from));
    let divisor = determinant(matrix);
    if !divisor.is_finite() || divisor.abs() < 1e-12 {
        return Err("singular camera matrix".into());
    }
    let mut axis: [f32; 4] = std::array::from_fn(|c| {
        if c == 3 {
            0.
        } else {
            let mut changed = matrix;
            for row in &mut changed {
                row[c] = 1.;
            }
            (determinant(changed) / divisor) as f32
        }
    });
    let y = (0..3).map(|c| axis[c] * weights[c]).sum::<f32>();
    if !y.is_finite() || y <= 0. || axis[..3].iter().any(|v| *v <= 0.) {
        return Err("invalid neutral sensor direction".into());
    }
    axis.iter_mut().for_each(|v| *v /= y);
    Ok(axis)
}
fn save_image(picture: image::RgbImage, path: &std::path::Path) -> Result<()> {
    let mut encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(std::fs::File::create(path)?, 98);
    encoder.set_icc_profile(TiffColorSpace::Srgb.profile().to_vec())?;
    encoder.encode_image(&picture)?;
    Ok(())
}
fn views(
    gpu: &Gpu,
    kernels: &Kernels,
    image: &RenderedImage,
    crop: [u32; 4],
    overview: bool,
    stem: &str,
    output: &std::path::Path,
) -> Result<[String; 3]> {
    let native = [crop[2] - crop[0], crop[3] - crop[1]];
    if crop[0] >= crop[2]
        || crop[1] >= crop[3]
        || crop[2] > image.view().extent[0]
        || crop[3] > image.view().extent[1]
    {
        return Err("invalid review crop".into());
    }
    let factor = if overview {
        (1280_f32 / native[0].max(native[1]) as f32).min(1.)
    } else {
        1.
    };
    let extent = native.map(|v| (v as f32 * factor).round() as u32);
    let view = image.view();
    let params = gpu.uniform(&Display {
        source: [
            image.description().extent[0],
            u32::from(overview),
            view.crop[0],
            view.crop[1],
        ],
        view: [
            view.crop[2] - view.crop[0],
            view.crop[3] - view.crop[1],
            view.rotation,
            extent[1].div_ceil(8),
        ],
        crop,
        matrix: TiffColorSpace::Srgb.transform(ColorSpace::from_metadata(image.source_metadata())?),
    });
    let encoded = kernels.present(gpu, image.buffer(), &params, extent)?;
    let bytes = gpu.readback(&encoded)?;
    let pixels: &[u32] = bytemuck::cast_slice(&bytes);
    let count = (extent[0] * extent[1]) as usize;
    let names = std::array::from_fn(|i| format!("assets/{stem}-ev{}.jpg", i * 2));
    for plane in 0..3 {
        let values: Vec<u8> = pixels[plane * count..(plane + 1) * count]
            .iter()
            .flat_map(|p| p.to_le_bytes().into_iter().take(3))
            .collect();
        save_image(
            image::RgbImage::from_raw(extent[0], extent[1], values).unwrap(),
            &output.join(&names[plane]),
        )?;
    }
    Ok(names)
}
pub(super) fn seed(session: &mut RenderSession, buffer: ReadGuard) -> Result<()> {
    let name = "texture.layers-tiles";
    let generation = session.cache.working[session.cache.active]
        .versions
        .get(name)
        .ok_or("texture checkpoint")?
        .generation;
    session
        .cache
        .entries
        .retain(|entry| entry.generation < generation);
    session.cache.entries.push(super::super::Entry {
        name: name.into(),
        generation,
        value: crate::render::Value {
            buffer,
            description: None,
        },
    });
    Ok(())
}
pub(super) fn stats(
    gpu: &Gpu,
    kernels: &Kernels,
    original: &ReadGuard,
    corrected: &ReadGuard,
    mask: &ReadGuard,
    noise: &ReadGuard,
    study: &ReadGuard,
) -> Result<Value> {
    let data = kernels.summarize(gpu, original, corrected, mask, noise, study)?;
    let records = data.as_chunks::<3>().0;
    let max_luma = records.iter().map(|s| s[0][0]).fold(0_f32, f32::max);
    let rgb_luma = records.iter().map(|s| s[1][0]).fold(0_f32, f32::max);
    let outside = records.iter().map(|s| s[0][1]).fold(0_f32, f32::max);
    let peak = records
        .iter()
        .map(|s| s[2])
        .max_by(|a, b| a[0].total_cmp(&b[0]))
        .unwrap_or_default();
    assert_eq!(outside, 0., "candidate changed a pixel outside its mask");
    Ok(
        json!({"max_weighted_luma_change":max_luma,"max_linear_rgb_luma_change":rgb_luma,"max_outside_mask_change":outside,"changed_atlas_pixels":records.iter().map(|s|s[0][2]as u64).sum::<u64>(),"mean_mask":records.iter().map(|s|f64::from(s[0][3])).sum::<f64>()/(mask.size()/4)as f64,"peak_layer_change":peak[0],"peak_native_position":[peak[1]as u32,peak[2]as u32]}),
    )
}
fn image(artifacts: Vec<crate::inspection::Artifact>) -> Result<RenderedImage> {
    let Content::Image(i) = artifacts.into_iter().next().ok_or("render")?.content else {
        return Err("image")?;
    };
    Ok(*i)
}

#[test]
#[ignore = "requires GPU and an explicit local-color study configuration"]
fn compare_local_color() -> Result<()> {
    let config = std::env::var_os("FOVEONIC_CHROMA_TRIALS").ok_or("study configuration")?;
    let config: Value = serde_json::from_slice(&std::fs::read(config)?)?;
    let measure_only = config["measure_only"].as_bool().unwrap_or(false);
    let sample_step = config["sample_step"].as_u64().unwrap_or(2).max(1) as usize;
    let candidates: Vec<Trial> = serde_json::from_value(
        config
            .get("candidates")
            .cloned()
            .unwrap_or(json!(CANDIDATES)),
    )?;
    for c in &candidates {
        if c.limits
            .iter()
            .chain(&c.reliability)
            .any(|v| !v.is_finite())
            || c.limits[0] >= c.limits[1]
            || c.limits[2] >= c.limits[3]
            || c.reliability[0] >= c.reliability[1]
            || !(0. ..=1.).contains(&c.reliability[2])
        {
            return Err("invalid research thresholds".into());
        }
    }
    let output = PathBuf::from(config["output"].as_str().ok_or("output")?);
    if output.exists() {
        return Err("choose a new output directory".into());
    }
    std::fs::create_dir_all(output.join("assets"))?;
    let gpu = Gpu::new()?;
    let kernels = Kernels::new(&gpu);
    let engine = Engine::from_device(
        gpu.device.clone(),
        gpu.queue.clone(),
        gpu.adapter_name.clone(),
    )?;
    let mut scenes = Vec::new();
    for item in config["photos"].as_array().ok_or("photos")? {
        let id = item["id"].as_str().ok_or("photo id")?;
        let source = Arc::new(Source::open(item["path"].as_str().ok_or("source")?)?);
        let mut settings = Settings::new(source.metadata.recorded_white_balance()?, "Standard");
        settings.merrill_noise_method = MerrillNoiseMethod::MultiscaleRowsAndColumns;
        settings.merrill_banding.method = MerrillBandingMethod::Directional;
        settings.reconstruction = Some(crate::noise::Controls {
            chroma: 4,
            luminance: item["luma"].as_u64().unwrap_or(2) as u32,
            ..Default::default()
        });
        settings.exposure = item["exposure"].as_f64().unwrap_or(0.) as f32;
        let crop: [u32; 4] = serde_json::from_value(item["crop"].clone())?;
        let measure: [u32; 4] = item
            .get("measure")
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()?
            .unwrap_or(crop);
        let probes: Vec<[u32; 2]> = item
            .get("probes")
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()?
            .unwrap_or_default();
        let mut session = RenderSession::new(source.clone());
        let initial = image(
            engine
                .render_session(
                    &mut session,
                    &settings,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )?
                .ok_or("cancelled")?,
        )?;
        let base = session.cache.get("noise.tiles").ok_or("base tiles")?.buffer;
        let raw = session
            .cache
            .get("column.tiles")
            .ok_or("conditioned tiles")?
            .buffer;
        let ratio = session
            .cache
            .get("detail.ratio-tiles")
            .map(|v| v.buffer)
            .unwrap_or_else(|| raw.clone());
        let p = crate::noise::prepare(
            &source.metadata,
            &settings.white_balance,
            settings.reconstruction.unwrap(),
        )?;
        let noise = gpu.uniform(&p);
        let mode = crate::nodes::color::color_mode::prepare(&source.metadata, "Standard")?;
        let camera = crate::nodes::sensor::calibration::color(
            &source.metadata,
            &settings.white_balance,
            &mode,
        )?
        .matrix;
        let display = TiffColorSpace::Srgb.transform(ColorSpace::from_metadata(&source.metadata)?);
        let rgb_y = std::array::from_fn(|c| {
            if c == 3 {
                0.
            } else {
                (0..3)
                    .map(|r| {
                        [0.2126, 0.7152, 0.0722][r]
                            * (0..3).map(|k| display[r][k] * camera[k][c]).sum::<f32>()
                    })
                    .sum()
            }
        });
        let combined: [[f32; 3]; 3] = std::array::from_fn(|r| {
            std::array::from_fn(|c| (0..3).map(|k| display[r][k] * camera[k][c]).sum())
        });
        let neutral_y = neutral_axis(combined, rgb_y)?;
        let extent = initial.view().extent;
        if measure[0] >= measure[2]
            || measure[1] >= measure[3]
            || measure[2] > extent[0]
            || measure[3] > extent[1]
            || probes
                .iter()
                .any(|q| q[0] >= extent[0] || q[1] >= extent[1])
        {
            return Err("invalid diagnostic measurement coordinates".into());
        }
        let probe_indices: Vec<_> = probes
            .iter()
            .map(|q| tile_index(storage(*q, initial.view()), p))
            .collect();
        let active =
            crate::nodes::sensor::calibration::rectangle(&source.metadata, "ActiveImageArea")?;
        let mut sample_points = Vec::new();
        for y in (measure[1]..measure[3]).step_by(sample_step) {
            for x in (measure[0]..measure[2]).step_by(sample_step) {
                sample_points.push([x, y]);
            }
        }
        let sample_indices: Vec<_> = sample_points
            .iter()
            .map(|q| tile_index(storage(*q, initial.view()), p))
            .collect();
        let raw_values = read_points(
            &gpu,
            &raw,
            &sample_points
                .iter()
                .map(|q| tile_index(storage(*q, initial.view()), p))
                .collect::<Vec<_>>(),
            3,
        )?;
        let mut runs = Vec::new();
        let mut reference_rgb = None;
        for candidate in &candidates {
            settings.reconstruction.as_mut().unwrap().chroma = candidate.chroma;
            let control = settings.reconstruction.unwrap();
            let texture = gpu.uniform(&Texture {
                strength: control.merrill_effects().strength,
                steps: [1., 2., 4., 8.],
            });
            let original = kernels.baseline(
                &gpu,
                texture::Inputs {
                    raw: &raw,
                    base: Some(&base),
                    ratio: Some(&ratio),
                },
                &noise,
                &texture,
                p.atlas_extent(),
            )?;
            let study = gpu.uniform(&Study {
                selection: [candidate.mode, candidate.radius, 0, 0],
                active,
                rgb_y,
                neutral_y,
                limits: candidate.limits,
                reliability: candidate.reliability,
            });
            let mut elapsed = Vec::new();
            let mut result = None;
            for _ in 0..if measure_only { 1 } else { 3 } {
                let began = Instant::now();
                let pair = kernels.apply(
                    &gpu,
                    Inputs {
                        raw: &raw,
                        base: &base,
                        ratio: &ratio,
                        original: &original,
                    },
                    Uniforms {
                        noise: &noise,
                        texture: &texture,
                        study: &study,
                    },
                    p.atlas_extent(),
                )?;
                gpu.device.poll(wgpu::Maintain::Wait);
                elapsed.push(began.elapsed().as_secs_f64() * 1000.);
                result = Some(pair);
            }
            elapsed.sort_by(f64::total_cmp);
            let (corrected, mask) = result.ok_or("candidate")?;
            let summary = stats(&gpu, &kernels, &original, &corrected, &mask, &noise, &study)?;
            let approved = candidate.mode == 5
                && candidate.limits == [1., 2., 0.5, 2.]
                && candidate.reliability == [0.20, 0.45, 1., 0.];
            let production_parity = if approved {
                let color = texture::Color::new(
                    camera,
                    ColorSpace::from_metadata(&source.metadata)?,
                    active,
                )?;
                let output = texture::apply(
                    &gpu,
                    texture::Inputs {
                        raw: &raw,
                        base: Some(&base),
                        ratio: Some(&ratio),
                    },
                    p,
                    texture::Prepared::new(control, 1., color)?,
                    texture::Requested {
                        layers: true,
                        change: false,
                    },
                    &mut Profile::default(),
                )?
                .layers
                .ok_or("fused texture")?;
                let mut comparison =
                    stats(&gpu, &kernels, &corrected, &output, &mask, &noise, &study)?;
                assert!(
                    comparison["peak_layer_change"].as_f64().unwrap() < 0.001,
                    "fused texture differs from approved method: {comparison}"
                );
                if config["benchmark_fused"].as_bool().unwrap_or(false) && candidate.chroma == 4 {
                    let mut timing = [Vec::new(), Vec::new()];
                    let prepared = texture::Prepared::new(control, 1., color)?;
                    for round in 0..15 {
                        for step in 0..2 {
                            let which = (round + step) % 2;
                            let began = Instant::now();
                            let output = if which == 0 {
                                let n = gpu.uniform(&p);
                                let t = gpu.uniform(&Texture {
                                    strength: control.merrill_effects().strength,
                                    steps: [1., 2., 4., 8.],
                                });
                                kernels.baseline(
                                    &gpu,
                                    texture::Inputs {
                                        raw: &raw,
                                        base: Some(&base),
                                        ratio: Some(&ratio),
                                    },
                                    &n,
                                    &t,
                                    p.atlas_extent(),
                                )?
                            } else {
                                texture::apply(
                                    &gpu,
                                    texture::Inputs {
                                        raw: &raw,
                                        base: Some(&base),
                                        ratio: Some(&ratio),
                                    },
                                    p,
                                    prepared,
                                    texture::Requested {
                                        layers: true,
                                        change: false,
                                    },
                                    &mut Profile::default(),
                                )?
                                .layers
                                .ok_or("timed texture")?
                            };
                            gpu.device.poll(wgpu::Maintain::Wait);
                            let ms = began.elapsed().as_secs_f64() * 1000.;
                            drop(output);
                            if round >= 3 {
                                timing[which].push(ms);
                            }
                        }
                    }
                    comparison["timing"] = json!({"baseline_ms":timing[0],"fused_ms":timing[1],"scope":"Three warmup and twelve alternating completed rounds with output/uniform allocation. Neutral Detail and Chroma 4. Excludes upstream noise, metadata preparation and photographic continuation. Other GPU use uncontrolled."});
                }
                Some(comparison)
            } else {
                None
            };
            let locked = if candidate.mode >= 4 && candidate.mode <= 5 {
                "max_linear_rgb_luma_change"
            } else {
                "max_weighted_luma_change"
            };
            assert!(
                summary[locked].as_f64().unwrap() < 0.001,
                "candidate changed its protected luminance: {summary}"
            );
            if candidate.mode == 0 {
                assert_eq!(summary["changed_atlas_pixels"], 0);
            }
            let mut probe_records = Vec::new();
            if !probes.is_empty() {
                let layers = read_points(&gpu, &corrected, &probe_indices, 3)?;
                let amounts = read_points(&gpu, &mask, &probe_indices, 1)?;
                for (i, point) in probes.iter().enumerate() {
                    let layers = triplet(&layers, i);
                    let rgb: [f32; 3] =
                        std::array::from_fn(|r| (0..3).map(|c| combined[r][c] * layers[c]).sum());
                    probe_records.push(json!({"point":point,"mask":amounts[i],"layers":layers,"linear_rgb_before_color_dq":rgb}));
                }
            }
            if measure_only {
                let before = read_points(&gpu, &original, &sample_indices, 3)?;
                let after = read_points(&gpu, &corrected, &sample_indices, 3)?;
                let masks = read_points(&gpu, &mask, &sample_indices, 1)?;
                let mut green_values = Vec::new();
                let mut rgb_delta = 0_f64;
                let mut max_rgb_delta = 0_f32;
                for i in 0..sample_indices.len() {
                    let a = triplet(&after, i);
                    let b = triplet(&before, i);
                    let rgb: [f32; 3] =
                        std::array::from_fn(|r| (0..3).map(|c| combined[r][c] * a[c]).sum());
                    green_values.push(green(rgb));
                    for row in combined {
                        let delta = (0..3).map(|c| row[c] * (a[c] - b[c])).sum::<f32>().abs();
                        rgb_delta += f64::from(delta);
                        max_rgb_delta = max_rgb_delta.max(delta);
                    }
                }
                runs.push(json!({"candidate":candidate,"technical":summary,"production_parity":production_parity,"probes":probe_records,"completed_ms":elapsed[0],"metrics":{"stage":"texture RGB before ColorDQ, code units","sample_step":sample_step,"sample_count":sample_indices.len(),"sample_mask_fraction":masks.iter().filter(|v|**v>0.).count()as f64/masks.len()as f64,"sample_mean_mask":masks.iter().map(|v|f64::from(*v)).sum::<f64>()/masks.len()as f64,"mean_rgb_change":rgb_delta/(sample_indices.len()*3)as f64,"max_rgb_change":max_rgb_delta,"green_p99":percentile(green_values.clone(),0.99),"green_p999":percentile(green_values,0.999)}}));
                println!("numeric chroma {id} {}", candidate.id);
                continue;
            }
            // Update recipe versions normally, then replace only the texture result for this comparison.
            engine
                .render_session(
                    &mut session,
                    &settings,
                    &["final"],
                    || false,
                    &mut Profile::default(),
                )?
                .ok_or("cancelled")?;
            seed(&mut session, corrected)?;
            let rendered = image(
                engine
                    .render_session(
                        &mut session,
                        &settings,
                        &["final"],
                        || false,
                        &mut Profile::default(),
                    )?
                    .ok_or("cancelled")?,
            )?;
            if candidate.id == "current4" {
                seed(&mut session, original.clone())?;
                let replay = image(
                    engine
                        .render_session(
                            &mut session,
                            &settings,
                            &["final"],
                            || false,
                            &mut Profile::default(),
                        )?
                        .ok_or("cancelled")?,
                )?;
                assert_eq!(
                    capture(&gpu, &replay, &sample_points)?,
                    capture(&gpu, &rendered, &sample_points)?,
                    "baseline photographic replay differs"
                );
            }
            let pictures = views(
                &gpu,
                &kernels,
                &rendered,
                crop,
                false,
                &format!("{id}-{}", candidate.id),
                &output,
            )?;
            let overview = views(
                &gpu,
                &kernels,
                &rendered,
                [0, 0, rendered.view().extent[0], rendered.view().extent[1]],
                true,
                &format!("{id}-{}-overview", candidate.id),
                &output,
            )?;
            let rgb = capture(&gpu, &rendered, &sample_points)?;
            if approved && candidate.chroma == 4 {
                let current = capture(&gpu, &initial, &sample_points)?;
                let error = rgb
                    .iter()
                    .zip(&current)
                    .flat_map(|(a, b)| (0..3).map(move |c| (a[c] - b[c]).abs()))
                    .fold(0_f32, f32::max);
                assert!(error < 0.000001, "photographic production parity: {error}");
            }
            if !probes.is_empty() {
                for (record, rgb) in probe_records
                    .iter_mut()
                    .zip(capture(&gpu, &rendered, &probes)?)
                {
                    record["final_linear_srgb"] = json!(rgb);
                }
            }
            let green_values: Vec<_> = rgb.iter().map(|v| green(*v)).collect();
            let baseline = if candidate.id == "current4" {
                reference_rgb = Some(rgb.clone());
                None
            } else {
                reference_rgb.as_ref()
            };
            let metrics = json!({"display_green_p99":percentile(green_values.clone(),0.99),"display_green_p999":percentile(green_values,0.999),"mean_final_rgb_luma_change":baseline.map(|b|rgb.iter().zip(b).map(|(a,b)| ((a[0]-b[0])*0.2126+(a[1]-b[1])*0.7152+(a[2]-b[2])*0.0722).abs()as f64).sum::<f64>()/rgb.len()as f64),"mean_rgb_change_to_current4":baseline.map(|b|rgb.iter().zip(b).map(|(a,b)|(0..3).map(|c|(a[c]-b[c]).abs()as f64).sum::<f64>()).sum::<f64>()/(rgb.len()*3)as f64),"sampled_conditioned_max":raw_values.iter().copied().fold(0_f32,f32::max)});
            runs.push(json!({"candidate":candidate,"images":pictures,"overviews":overview,"completed_ms":elapsed[1],"timing_samples":elapsed,"technical":summary,"production_parity":production_parity,"metrics":metrics,"probes":probe_records}));
            println!(
                "local chroma {id} {} {:.1} ms {}",
                candidate.id, elapsed[1], summary
            );
        }
        scenes.push(json!({"id":id,"description":item["description"],"crop":crop,"measure":measure,"extent":initial.view().extent,"native_crop":initial.view().crop,"rotation":initial.view().rotation,"settings":settings,"runs":runs}));
        std::fs::write(
            output.join("progress.json"),
            serde_json::to_vec_pretty(&scenes)?,
        )?;
    }
    let data = json!({"candidates":candidates,"scenes":scenes,"assessment":config.get("assessment"),"default_candidate":config.get("default_candidate"),"sweep":config.get("sweep"),"measure_only":measure_only,"sample_step":sample_step,"method":"Local color-reference research. Fixed native noise processing and brightness reconstruction. Photographic views apply native, +2 and +4 EV display gains to float renders before JPEG encoding. Numerical-only screening stops before ColorDQ and produces no photographic views. Methods are unoptimized prototypes. Timings exclude upstream noise processing and photographic continuation. Whole-photo views use bounded nearest sampling for context. Natural photographs have no clean target. No app processing or defaults changed."});
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&data)?,
    )?;
    if !measure_only {
        let page = include_str!("report.html").replace(
            "STUDY_JSON",
            &serde_json::to_string(&data)?.replace('<', "\\u003c"),
        );
        std::fs::write(output.join("index.html"), page)?;
    }
    println!(
        "{}",
        output
            .join(if measure_only {
                "report.json"
            } else {
                "index.html"
            })
            .display()
    );
    Ok(())
}
