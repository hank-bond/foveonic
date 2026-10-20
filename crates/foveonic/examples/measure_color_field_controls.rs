//! Measures highlight selectivity in fresh/cached float outputs and RGB16, with matched preview regions.
use foveonic::{
    image::{RenderedImage, View},
    inspection::{Artifact, Content},
    profile::{Mode, Profile},
    render::{
        ColorFieldApproach, ColorFieldControls, Engine, HighlightMode, RenderSession, Settings,
    },
    sidecar,
    source::{Result, Source},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Deserialize, Serialize)]
struct Region {
    id: String,
    label: String,
    #[serde(rename = "box")]
    bounds: [u32; 4],
}
struct Samples {
    layers: Vec<f32>,
    rgb: Vec<f32>,
    encoded: Vec<u16>,
}
#[derive(Serialize)]
struct Metrics {
    pixels: u64,
    maximum: f64,
    mean_absolute: f64,
    rms: f64,
    thresholds: Vec<f64>,
    pixels_above: Vec<u64>,
    maximum_box: Option<[u32; 4]>,
    nonfinite_components: u64,
}
impl Metrics {
    fn new(thresholds: &[f64]) -> Self {
        Self {
            pixels: 0,
            maximum: 0.,
            mean_absolute: 0.,
            rms: 0.,
            thresholds: thresholds.to_vec(),
            pixels_above: vec![0; thresholds.len()],
            maximum_box: None,
            nonfinite_components: 0,
        }
    }
    fn pixel(&mut self, x: u32, y: u32, differences: [f64; 3]) {
        self.pixels += 1;
        let mut maximum = 0_f64;
        for d in differences {
            if !d.is_finite() {
                self.nonfinite_components += 1;
                continue;
            }
            maximum = maximum.max(d.abs());
            self.mean_absolute += d.abs();
            self.rms += d * d;
        }
        self.maximum = self.maximum.max(maximum);
        for (i, t) in self.thresholds.iter().enumerate() {
            self.pixels_above[i] += u64::from(maximum > *t);
        }
        if maximum > *self.thresholds.last().unwrap() {
            let b = self.maximum_box.get_or_insert([x, y, x + 1, y + 1]);
            b[0] = b[0].min(x);
            b[1] = b[1].min(y);
            b[2] = b[2].max(x + 1);
            b[3] = b[3].max(y + 1);
        }
    }
    fn finish(mut self) -> Self {
        let n = (self.pixels * 3).max(1) as f64;
        self.mean_absolute /= n;
        self.rms = (self.rms / n).sqrt();
        self
    }
}
fn physical(view: View, x: u32, y: u32, width: u32) -> usize {
    let w = view.crop[2] - view.crop[0];
    let h = view.crop[3] - view.crop[1];
    let (x, y) = match view.rotation {
        90 => (y, h - 1 - x),
        180 => (w - 1 - x, h - 1 - y),
        270 => (w - 1 - y, x),
        _ => (x, y),
    };
    ((y + view.crop[1]) * width + x + view.crop[0]) as usize * 3
}
fn float_metrics(
    a: &[f32],
    b: &[f32],
    view: View,
    width: u32,
    region: [u32; 4],
    thresholds: &[f64],
) -> Metrics {
    assert_eq!(a.len(), b.len());
    let mut result = Metrics::new(thresholds);
    for y in region[1]..region[3] {
        for x in region[0]..region[2] {
            let i = physical(view, x, y, width);
            result.pixel(
                x,
                y,
                std::array::from_fn(|c| f64::from(a[i + c]) - f64::from(b[i + c])),
            );
        }
    }
    result.finish()
}
fn encoded_metrics(a: &[u16], b: &[u16], size: [u32; 2], region: [u32; 4]) -> Metrics {
    assert_eq!(a.len(), b.len());
    let mut result = Metrics::new(&[0., 1., 16., 257.]);
    for y in region[1]..region[3] {
        for x in region[0]..region[2] {
            let i = (y * size[0] + x) as usize * 3;
            result.pixel(
                x,
                y,
                std::array::from_fn(|c| f64::from(a[i + c]) - f64::from(b[i + c])),
            );
        }
    }
    result.finish()
}
fn compare(a: &Samples, b: &Samples, view: View, width: u32, regions: &[Region]) -> Value {
    let mut regions = regions.to_vec();
    regions.insert(
        0,
        Region {
            id: "full".into(),
            label: "Full active view".into(),
            bounds: [0, 0, view.extent[0], view.extent[1]],
        },
    );
    Value::Array(regions.iter().map(|r|json!({"region":r,"layers_calibrated_codes":float_metrics(&a.layers,&b.layers,view,width,r.bounds,&[0.,0.01,0.1,0.5,1.]),"final_linear_working_rgb":float_metrics(&a.rgb,&b.rgb,view,width,r.bounds,&[0.,1e-7,1e-6,1e-5,1e-4,1e-3]),"encoded_native_profile_rgb16":if a.encoded.is_empty()||b.encoded.is_empty(){Value::Null}else{serde_json::to_value(encoded_metrics(&a.encoded,&b.encoded,view.extent,r.bounds)).unwrap()}})).collect())
}
fn images(mut artifacts: Vec<Artifact>) -> Result<(RenderedImage, RenderedImage)> {
    if artifacts.len() != 2 {
        return Err("expected the two requested outputs".into());
    }
    // The final alias carries its concrete graph-output name. Results retain
    // requested order, so do not look for an artifact literally named final.
    let final_image = artifacts.pop().unwrap();
    let layers = artifacts.pop().unwrap();
    if layers.name != "highlight.layers" {
        return Err("unexpected highlight output".into());
    }
    match (layers.content, final_image.content) {
        (Content::Image(layers), Content::Image(image)) => Ok((*layers, *image)),
        _ => Err("expected floating images".into()),
    }
}
fn read(engine: &Engine, layers: &RenderedImage, rgb: &RenderedImage) -> Result<Samples> {
    let read = |image: &RenderedImage| -> Result<Vec<f32>> {
        let bytes = engine.readback_image(image)?;
        Ok(bytemuck::cast_slice::<u8, f32>(&bytes).to_vec())
    };
    Ok(Samples {
        layers: read(layers)?,
        rgb: read(rgb)?,
        encoded: Vec::new(),
    })
}
fn encode(engine: &Engine, image: &RenderedImage, path: &Path) -> Result<Vec<u16>> {
    engine.write_tiff(image, path, &mut Profile::default())?;
    let mut decoder = tiff::decoder::Decoder::new(std::io::BufReader::new(fs::File::open(path)?))?;
    match decoder.read_image()? {
        tiff::decoder::DecodingResult::U16(data) => Ok(data),
        _ => Err("expected native RGB16 TIFF".into()),
    }
}
fn preview(python: &Path, path: &Path, scene: &str, variant: &str) -> Result<Value> {
    let result = std::process::Command::new(python)
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/color_field_report.py"))
        .arg("--stage")
        .arg(path)
        .arg("--scene")
        .arg(scene)
        .arg("--variant")
        .arg(variant)
        .output()?;
    if !result.status.success() {
        return Err(format!(
            "preview failed: {}",
            String::from_utf8_lossy(&result.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&result.stdout)?)
}
fn record(log: &mut fs::File, event: Value) -> Result<()> {
    writeln!(log, "{event}")?;
    log.flush()?;
    Ok(())
}
fn variants(base: &Settings) -> Vec<(&'static str, Settings)> {
    let mut result = Vec::new();
    for (name, selectivity) in [
        ("default", 0.),
        ("default-repeat", 0.),
        ("selection-025", 0.25),
        ("selection-05", 0.5),
        ("selection-max", 1.),
        ("default-return", 0.),
    ] {
        let mut settings = base.clone();
        settings.color_field.selectivity = selectivity;
        result.push((name, settings));
    }
    result
}
fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let corpus = PathBuf::from(
        args.next()
            .ok_or("measure_color_field_controls CORPUS NEW_OUTPUT PYTHON [scene,...]")?,
    );
    let output = PathBuf::from(args.next().ok_or("missing new output directory")?);
    let python = PathBuf::from(args.next().ok_or("missing preview Python")?);
    let selected = args
        .next()
        .map(|s| s.split(',').map(String::from).collect::<Vec<_>>());
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    if output.exists() {
        return Err("measurement directory must be new".into());
    }
    fs::create_dir_all(&output)?;
    let mut log = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output.join("measurements.jsonl"))?;
    let engine = Engine::new()?;
    let registration: Value =
        serde_json::from_str(include_str!("../../../scripts/color_field_pr_regions.json"))?;
    record(
        &mut log,
        json!({"event":"session","adapter":engine.adapter_name(),"method":"Selection strengths 0, 0.25, 0.5 and 1 with repeated/restored zero and other controls at default. Fresh and edit-session outputs. Repeated/restored zero measures numerical variation and reversibility. Original neutral weights are preserved. No timing claim. All floating measurements are active-view samples in original storage. RGB16 is native-profile encoded output, not a perceptual display metric. Previews use the existing color-managed eight-bit staging workflow."}),
    )?;
    let mut failures = 0usize;
    let mut inputs = fs::read_dir(&corpus)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    inputs.sort();
    for path in inputs {
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("x3f"))
        {
            continue;
        }
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        if selected.as_ref().is_some_and(|s| !s.contains(&stem)) {
            continue;
        }
        let saved = sidecar::load(&path)?;
        let mut base = saved.clone().unwrap_or_else(|| {
            let mut s = Settings::new("Sunlight", "Standard");
            s.exposure = -1.5;
            s
        });
        base.highlight_mode = HighlightMode::ColorField;
        base.highlight_restoration = true;
        base.output_size = None;
        let directory = output.join(&stem);
        fs::create_dir(&directory)?;
        fs::write(
            directory.join("base-settings.json"),
            serde_json::to_vec_pretty(&base)?,
        )?;
        let regions: Vec<Region> = registration["scenes"][&stem]["regions"]
            .as_array()
            .map(|_| serde_json::from_value(registration["scenes"][&stem]["regions"].clone()))
            .transpose()?
            .unwrap_or_default();
        for (method, approach) in [
            ("global", ColorFieldApproach::Faithful),
            ("local", ColorFieldApproach::LocalGradient),
        ] {
            base.color_field = ColorFieldControls {
                approach,
                ..Default::default()
            };
            let mut session = RenderSession::new(Source::open(&path)?);
            let mut baseline: Option<Samples> = None;
            let mut retained: Option<(RenderedImage, RenderedImage, Samples)> = None;
            for (name, settings) in variants(&base) {
                let variant = format!("{method}-{name}");
                eprintln!("{stem} {variant}");
                let rendered = engine.render_artifacts(
                    session.source(),
                    &settings,
                    &["highlight.layers", "final"],
                    &mut Profile::default(),
                );
                let (layers, image) = match rendered.and_then(images) {
                    Ok(v) => v,
                    Err(e) => {
                        record(
                            &mut log,
                            json!({"event":"error","scene":stem,"variant":variant,"settings":settings,"phase":"fresh","error":e.to_string()}),
                        )?;
                        failures += 1;
                        break;
                    }
                };
                let view = image.view();
                let width = image.description().extent[0];
                assert_eq!(layers.description().extent, image.description().extent);
                if let Some(dimensions) = registration["scenes"][&stem]["dimensions"].as_array() {
                    assert_eq!(
                        serde_json::to_value(view.extent)?,
                        Value::Array(dimensions.clone())
                    );
                }
                let mut fresh = read(&engine, &layers, &image)?;
                let assets = if !matches!(name, "default-repeat" | "default-return") {
                    let staging = directory.join(format!(".staging-{variant}.tiff"));
                    fresh.encoded = encode(&engine, &image, &staging)?;
                    Some(preview(&python, &staging, &stem, &variant)?)
                } else {
                    None
                };
                let mut profile = Profile::new(Mode::Wall);
                let cached = engine.render_session(
                    &mut session,
                    &settings,
                    &["highlight.layers", "final"],
                    || false,
                    &mut profile,
                );
                let cached = cached.and_then(|a| images(a.ok_or("unexpected cancellation")?));
                let (cached_layers, cached_image) = match cached {
                    Ok(v) => v,
                    Err(e) => {
                        record(
                            &mut log,
                            json!({"event":"error","scene":stem,"variant":variant,"settings":settings,"phase":"cached","error":e.to_string()}),
                        )?;
                        failures += 1;
                        break;
                    }
                };
                let cached = read(&engine, &cached_layers, &cached_image)?;
                let cache_difference = compare(&fresh, &cached, view, width, &regions);
                let default_difference = baseline
                    .as_ref()
                    .map(|b| compare(b, &fresh, view, width, &regions));
                record(
                    &mut log,
                    json!({"event":"measurement","scene":stem,"variant":variant,"settings":settings,"sidecar_used":saved.is_some(),"physical_extent":image.description().extent,"view":{"crop":view.crop,"rotation":view.rotation,"extent":view.extent},"assets":assets,"fresh_vs_cached":cache_difference,"default_vs_fresh":default_difference,"cached_profile":profile}),
                )?;
                if name == "default" {
                    baseline = Some(fresh);
                    retained = Some((cached_layers, cached_image, cached));
                }
            }
            if let Some((layers, image, original)) = retained {
                let now = read(&engine, &layers, &image)?;
                let unchanged = bytemuck::cast_slice::<f32, u8>(&now.layers)
                    == bytemuck::cast_slice::<f32, u8>(&original.layers)
                    && bytemuck::cast_slice::<f32, u8>(&now.rgb)
                        == bytemuck::cast_slice::<f32, u8>(&original.rgb);
                record(
                    &mut log,
                    json!({"event":"retained","scene":stem,"method":method,"unchanged":unchanged}),
                )?;
                if !unchanged {
                    return Err("retained image changed during edit sequence".into());
                }
            }
        }
    }
    if failures > 0 {
        return Err(format!(
            "{failures} sensitivity sequences failed; see preserved error records"
        )
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selectivity_steps_preserve_other_controls_and_return_to_zero() {
        let base = Settings::new("Sunlight", "Standard");
        let v = variants(&base);
        assert_eq!(v[0].1, v[1].1);
        assert_eq!(v[0].1, v[5].1);
        for (i, selectivity) in [0., 0., 0.25, 0.5, 1., 0.].into_iter().enumerate() {
            let mut expected = base.clone();
            expected.color_field.selectivity = selectivity;
            assert_eq!(v[i].1, expected);
        }
    }
    #[test]
    fn rotation_and_metrics_use_the_same_native_pixels() {
        let a: Vec<_> = (0..4 * 3 * 3).map(|v| v as f32).collect();
        let mut b = a.clone();
        b[(1 * 4 + 2) * 3] += 2.;
        let view = View {
            crop: [1, 0, 4, 3],
            rotation: 90,
            extent: [3, 3],
        };
        assert_eq!(physical(view, 1, 1, 4), (1 * 4 + 2) * 3);
        let m = float_metrics(&a, &b, view, 4, [0, 0, 3, 3], &[0., 1.]);
        assert_eq!(m.pixels, 9);
        assert_eq!(m.maximum, 2.);
        assert_eq!(m.pixels_above, [1, 1]);
        assert_eq!(m.maximum_box, Some([1, 1, 2, 2]));
    }
}
