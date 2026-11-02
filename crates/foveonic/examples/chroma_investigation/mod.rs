//! Inspects small GPU sample tables around green outliers without changing photographic processing.

mod controls;
mod neutralization;
mod trials;

use super::RenderSession;
use crate::{
    color_space::{ColorSpace, TiffColorSpace},
    gpu::{Gpu, ReadGuard},
    image::{RenderedImage, View},
    inspection::Content,
    noise::Parameters,
    profile::Profile,
    render::{Engine, MerrillBandingMethod, MerrillNoiseMethod, Settings},
    source::{Result, Source},
};
use serde_json::{json, Value};
use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

fn storage([x, y]: [u32; 2], view: View) -> [u32; 2] {
    let w = view.crop[2] - view.crop[0];
    let h = view.crop[3] - view.crop[1];
    let p = match view.rotation {
        90 => [y, h - 1 - x],
        180 => [w - 1 - x, h - 1 - y],
        270 => [w - 1 - y, x],
        _ => [x, y],
    };
    [p[0] + view.crop[0], p[1] + view.crop[1]]
}
fn tile_index([x, y]: [u32; 2], p: Parameters) -> usize {
    let core = [p.source[0] / p.source[2], p.source[1] / p.source[3]];
    let tile = [x / core[0], y / core[1]];
    let local = [
        x - tile[0] * core[0] + p.tile[2],
        y - tile[1] * core[1] + p.tile[2],
    ];
    let tile = (tile[1] - p.window[1]) * p.window[2] + tile[0] - p.window[0];
    ((tile * p.tile[1] + local[1]) * p.tile[0] + local[0]) as usize
}
fn read_points(
    gpu: &Gpu,
    buffer: &ReadGuard,
    indices: &[usize],
    components: usize,
) -> Result<Vec<f32>> {
    let stride = (components * 4) as u64;
    let mut packed = gpu.storage(
        "Chroma investigation sample table",
        indices.len() as u64 * stride,
    )?;
    gpu.copy_ranges(
        buffer,
        packed.write(),
        indices
            .iter()
            .enumerate()
            .map(|(i, point)| [*point as u64 * stride, i as u64 * stride, stride]),
    );
    Ok(bytemuck::cast_slice::<u8, f32>(&gpu.readback(&packed)?).to_vec())
}
fn capture(gpu: &Gpu, image: &RenderedImage, points: &[[u32; 2]]) -> Result<Vec<[f32; 3]>> {
    let indices: Vec<_> = points
        .iter()
        .map(|p| {
            let q = storage(*p, image.view());
            (q[1] * image.description().extent[0] + q[0]) as usize
        })
        .collect();
    let data = read_points(gpu, image.buffer(), &indices, 3)?;
    let space = ColorSpace::from_metadata(image.source_metadata())?;
    let matrix = TiffColorSpace::Srgb.transform(space);
    Ok(data
        .as_chunks::<3>()
        .0
        .iter()
        .map(|v| std::array::from_fn(|c| (0..3).map(|k| matrix[c][k] * v[k]).sum()))
        .collect())
}
fn dot(v: [f32; 3], weights: [f32; 4]) -> f32 {
    (0..3).map(|c| v[c] * weights[c]).sum()
}
fn triplet(values: &[f32], i: usize) -> [f32; 3] {
    values[i * 3..i * 3 + 3].try_into().unwrap()
}
fn green(v: [f32; 3]) -> f32 {
    v[1] - (v[0] + v[2]) * 0.5
}
fn percentile(mut values: Vec<f32>, fraction: f32) -> f32 {
    values.sort_by(f32::total_cmp);
    values[((values.len() - 1) as f32 * fraction) as usize]
}

#[test]
#[ignore = "requires GPU, explicit source path and new diagnostic output directory"]
fn inspect_green_outliers() -> Result<()> {
    let source = Arc::new(Source::open(
        std::env::var_os("FOVEONIC_CHROMA_SOURCE").ok_or("source")?,
    )?);
    let output = PathBuf::from(std::env::var_os("FOVEONIC_CHROMA_OUTPUT").ok_or("output")?);
    if output.exists() {
        return Err("choose a new diagnostic output directory".into());
    }
    std::fs::create_dir_all(&output)?;
    let gpu = Gpu::new()?;
    let engine = Engine::from_device(
        gpu.device.clone(),
        gpu.queue.clone(),
        gpu.adapter_name.clone(),
    )?;
    let kernels = crate::gpu::chroma_investigation::Kernels::new(&gpu);
    let targets = [[539, 76], [583, 85], [595, 98], [509, 245], [597, 234]];
    let mut set = BTreeSet::new();
    for y in (20..300).step_by(4) {
        for x in (480..660).step_by(4) {
            set.insert([x, y]);
        }
    }
    for [x, y] in targets {
        for dy in -4..=4 {
            for dx in -4..=4 {
                set.insert([(x as i32 + dx) as u32, (y as i32 + dy) as u32]);
            }
        }
    }
    let points: Vec<_> = set.into_iter().collect();
    let mut runs = Vec::new();
    for luma in [0, 2] {
        let mut settings = Settings::new(source.metadata.recorded_white_balance()?, "Standard");
        settings.merrill_noise_method = MerrillNoiseMethod::MultiscaleRowsAndColumns;
        settings.merrill_banding.method = MerrillBandingMethod::Directional;
        settings.reconstruction = Some(crate::noise::Controls {
            chroma: 2,
            luminance: luma,
            ..Default::default()
        });
        let p = crate::noise::prepare(
            &source.metadata,
            &settings.white_balance,
            settings.reconstruction.unwrap(),
        )?;
        let mode = crate::nodes::color::color_mode::prepare(&source.metadata, "Standard")?;
        let camera = crate::nodes::sensor::calibration::color(
            &source.metadata,
            &settings.white_balance,
            &mode,
        )?
        .matrix;
        let display = TiffColorSpace::Srgb.transform(ColorSpace::from_metadata(&source.metadata)?);
        let matrix: [[f32; 3]; 3] = std::array::from_fn(|r| {
            std::array::from_fn(|c| (0..3).map(|k| display[r][k] * camera[k][c]).sum())
        });
        let mut session = RenderSession::new(source.clone());
        let mut upstream: Option<(Vec<f32>, Vec<f32>, Vec<f32>)> = None;
        for chroma in [2, 3, 4] {
            settings.reconstruction.as_mut().unwrap().chroma = chroma;
            engine
                .render_session(
                    &mut session,
                    &settings,
                    &["color.rgb", "linear.base"],
                    || false,
                    &mut Profile::default(),
                )?
                .ok_or("cancelled")?;
            // Diagnose the recorded pre-adoption algorithm independently of
            // the app's fused safeguard so the original mechanism stays reproducible.
            let raw = session
                .cache
                .get("column.tiles")
                .ok_or("conditioned tiles")?
                .buffer;
            let base = session.cache.get("noise.tiles").ok_or("base tiles")?.buffer;
            let ratio = session
                .cache
                .get("detail.ratio-tiles")
                .map(|v| v.buffer)
                .unwrap_or_else(|| raw.clone());
            let noise = gpu.uniform(&p);
            let texture = gpu.uniform(&trials::Texture {
                strength: settings.reconstruction.unwrap().merrill_effects().strength,
                steps: [1., 2., 4., 8.],
            });
            let baseline = kernels.baseline(
                &gpu,
                crate::nodes::color::texture::Inputs {
                    raw: &raw,
                    base: Some(&base),
                    ratio: Some(&ratio),
                },
                &noise,
                &texture,
                p.atlas_extent(),
            )?;
            trials::seed(&mut session, baseline)?;
            let artifacts = engine
                .render_session(
                    &mut session,
                    &settings,
                    &["color.rgb", "linear.base"],
                    || false,
                    &mut Profile::default(),
                )?
                .ok_or("cancelled")?;
            let images: Vec<_> = artifacts
                .iter()
                .map(|a| {
                    let Content::Image(image) = &a.content else {
                        panic!("image required")
                    };
                    image.as_ref()
                })
                .collect();
            let physical: Vec<_> = points
                .iter()
                .map(|q| storage(*q, images[0].view()))
                .collect();
            let indices: Vec<_> = physical.iter().map(|q| tile_index(*q, p)).collect();
            let cached = |name: &str| {
                session.cache.get(name).ok_or_else(|| {
                    format!(
                        "missing {name}, retained: {:?}",
                        session
                            .cache
                            .entries
                            .iter()
                            .map(|e| &e.name)
                            .collect::<Vec<_>>()
                    )
                })
            };
            let base = read_points(&gpu, &cached("noise.tiles")?.buffer, &indices, 3)?;
            let raw = read_points(&gpu, &cached("column.tiles")?.buffer, &indices, 3)?;
            let ratio = if luma == 0 {
                vec![1.; points.len()]
            } else {
                read_points(&gpu, &cached("detail.ratio-tiles")?.buffer, &indices, 1)?
            };
            if let Some(previous) = &upstream {
                assert_eq!(
                    (&raw, &base, &ratio),
                    (&previous.0, &previous.1, &previous.2),
                    "Chroma changed upstream estimates"
                );
            } else {
                upstream = Some((raw.clone(), base.clone(), ratio.clone()));
            }
            let before_dq = capture(&gpu, images[0], &points)?;
            let after_dq = capture(&gpu, images[1], &points)?;
            let mut probes = Vec::new();
            let mut floor = 0;
            let mut negative = 0;
            let mut affected = 0;
            let mut brightness_gains = Vec::new();
            let mut base_luminances = Vec::new();
            for (i, point) in points.iter().enumerate() {
                let a = triplet(&base, i);
                let d = triplet(&raw, i);
                let al = dot(a, p.weights);
                let dl = dot(d, p.weights);
                let positive = a.map(|v| v.max(0.));
                let pl = dot(positive, p.weights);
                let cleaned = (ratio[i] * (al.max(0.) + p.edge[2]) - p.edge[2]).max(0.);
                let selected = dl + (cleaned - dl) * (luma as f32 / 4.);
                brightness_gains.push(selected / al.max(0.0001));
                base_luminances.push(al);
                if al <= 0.0001 {
                    floor += 1;
                }
                if a.iter().any(|v| *v < 0.) {
                    negative += 1;
                }
                let signed = a.map(|v| v / al.max(0.0001));
                let guarded = positive.map(|v| v / pl.max(0.0001));
                if signed
                    .iter()
                    .zip(guarded)
                    .any(|(a, b)| (*a - b).abs() > 0.00001)
                {
                    affected += 1;
                }
                if targets.contains(point) {
                    let transform = |v: [f32; 3]| -> [f32; 3] {
                        std::array::from_fn(|r| (0..3).map(|c| matrix[r][c] * v[c]).sum())
                    };
                    let positive_raw = d.map(|v| v.max(0.));
                    let raw_positive_luma = dot(positive_raw, p.weights).max(0.0001);
                    let raw_proportion = positive_raw.map(|v| v / raw_positive_luma);
                    let weight = 1. - (1. - chroma as f32 / 4.).powi(2);
                    let predicted_layers = std::array::from_fn(|c| {
                        (raw_proportion[c] * (1. - weight) + signed[c] * weight)
                            * selected.max(0.0001)
                    });
                    let predicted_rgb = transform(predicted_layers);
                    let prediction_error = predicted_rgb
                        .iter()
                        .zip(before_dq[i])
                        .map(|(a, b)| (*a - b).abs())
                        .fold(0_f32, f32::max);
                    assert!(
                        prediction_error < 0.001,
                        "blend prediction differs by {prediction_error} at {point:?}"
                    );
                    let base_rgb = transform(a);
                    let conditioned_rgb = transform(d);
                    let neighbors: Vec<_> = points
                        .iter()
                        .enumerate()
                        .filter(|(_, q)| {
                            q[0].abs_diff(point[0]) <= 2 && q[1].abs_diff(point[1]) <= 2
                        })
                        .map(|(j, _)| green(before_dq[j]))
                        .collect();
                    probes.push(json!({"point":point,"raw_layers":d,"base_layers":a,"raw_luma":dl,"base_luma":al,"positive_base_luma":pl,"ratio":ratio[i],"selected_luma":selected,"brightness_gain":selected/al.max(0.0001),"normalization_difference":signed.iter().zip(guarded).map(|(a,b)|(*a-b).abs()).fold(0_f32,f32::max),"base_rgb_at_reference_brightness":base_rgb,"conditioned_rgb_at_input_brightness":conditioned_rgb,"predicted_rgb":predicted_rgb,"prediction_error":prediction_error,"rgb_before_dq":before_dq[i],"rgb_after_dq":after_dq[i],"neighbor_green_median":percentile(neighbors.clone(),0.5),"neighbor_green_p95":percentile(neighbors,0.95)}));
                }
            }
            let summary = json!({"luma":luma,"chroma":chroma,"sample_count":points.len(),"base_floor_count":floor,"negative_base_count":negative,"normalization_changes_count":affected,"gain_p50":percentile(brightness_gains.clone(),0.5),"gain_p99":percentile(brightness_gains,0.99),"base_luma_min":base_luminances.into_iter().fold(f32::INFINITY,f32::min),"before_dq_green_p99":percentile(before_dq.iter().map(|v|green(*v)).collect(),0.99),"after_dq_green_p99":percentile(after_dq.iter().map(|v|green(*v)).collect(),0.99),"probes":probes});
            println!("chroma investigation Luma {luma}, Chroma {chroma}: floor {floor}, negative base {negative}, affected {affected}");
            runs.push(summary);
        }
    }
    let result: Value = json!({"source":std::env::var_os("FOVEONIC_CHROMA_SOURCE").map(PathBuf::from),"settings_note":"Recorded WB, Standard, Multiscale Rows + Columns, default Directional Fine 1/27, Coarse 0, neutral detail, shadow neutralization off. Controlled recipe rather than exact screenshot edits.","weights":crate::noise::prepare(&source.metadata,&source.metadata.recorded_white_balance()?,Default::default())?.weights,"unfiltered_noise_model":crate::noise::prepare(&source.metadata,&source.metadata.recorded_white_balance()?,Default::default())?.model,"runs":runs});
    std::fs::write(
        output.join("diagnosis.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    Ok(())
}
