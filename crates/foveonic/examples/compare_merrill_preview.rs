//! Compares completed Merrill reduced-preview variants and writes a color-managed local review.

use foveonic::{
    image::RenderedImage,
    inspection::Content,
    profile::{Mode, Profile},
    render::{Engine, PreviewQuality, RenderSession, Settings, TiffColorSpace},
    source::{Result, Source},
};
use image::ImageEncoder;
use serde_json::{json, Value};
use std::{fs, io::BufReader, path::Path, sync::Arc, time::Instant};

const VARIANTS: [&str; 3] = ["full", "reduced", "skip-speckle"];
const SCENES: [&str; 6] = [
    "blue-parking-sign",
    "sunlit-red-flower",
    "paper-and-recycling-can",
    "glossy-leaves-and-buds",
    "turquoise-still-life",
    "rocks-and-tree-shadows",
];
// One discarded warmup per variant, followed by three alternating-order trials.
const ROUNDS: usize = 4;
const JPEG_QUALITY: u8 = 100;

fn settings(variant: &str) -> Settings {
    let mut settings = Settings::new("Sunlight", "Standard");
    settings.reconstruction = Some(Default::default());
    settings.tiff_color_space = Some(TiffColorSpace::Srgb);
    if variant != "full" {
        settings.preview_quality = PreviewQuality::Fast;
    }
    settings.despeckle = variant != "skip-speckle";
    settings
}

fn complete(
    engine: &Engine,
    render: impl FnOnce() -> Result<RenderedImage>,
) -> Result<(RenderedImage, f64)> {
    let began = Instant::now();
    let image = render()?;
    let _texture = engine.view_texture(&image, None, &mut Profile::default())?;
    engine.device().poll(wgpu::Maintain::Wait);
    Ok((image, began.elapsed().as_secs_f64() * 1000.))
}

fn session_image(
    engine: &Engine,
    session: &mut RenderSession,
    settings: &Settings,
) -> Result<RenderedImage> {
    let artifact = engine
        .render_session(
            session,
            settings,
            &["final"],
            || false,
            &mut Profile::default(),
        )?
        .ok_or("comparison render superseded")?
        .pop()
        .ok_or("missing comparison image")?;
    let Content::Image(image) = artifact.content else {
        return Err("expected comparison image".into());
    };
    Ok(*image)
}

fn asset(engine: &Engine, image: &RenderedImage, directory: &Path, variant: &str) -> Result<Value> {
    let temporary = directory.join(format!("{variant}.tiff"));
    let extent = engine.write_tiff(image, &temporary, &mut Profile::default())?;
    // Only repack already GPU-encoded display bytes. Camera processing, crop,
    // orientation and sRGB conversion have completed before this transfer.
    let mut decoder = tiff::decoder::Decoder::new(BufReader::new(fs::File::open(&temporary)?))?;
    let icc = decoder.get_tag_u8_vec(tiff::tags::Tag::Unknown(34675))?;
    let tiff::decoder::DecodingResult::U16(pixels) = decoder.read_image()? else {
        return Err("expected RGB16 comparison TIFF".into());
    };
    let bytes: Vec<_> = pixels.into_iter().map(|v| (v >> 8) as u8).collect();
    let file = format!("{variant}.jpg");
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
        fs::File::create(directory.join(&file))?,
        JPEG_QUALITY,
    );
    encoder.set_icc_profile(icc)?;
    encoder.encode(&bytes, extent[0], extent[1], image::ExtendedColorType::Rgb8)?;
    drop(decoder);
    fs::remove_file(temporary)?;
    Ok(json!({"file":file, "extent":extent}))
}

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let root = std::path::PathBuf::from(
        args.next()
            .ok_or("compare_merrill_preview ORIGINALS NEW_REPORT_DIR")?,
    );
    let output = std::path::PathBuf::from(args.next().ok_or("missing new report directory")?);
    if args.next().is_some() || output.exists() {
        return Err("provide an originals directory and a new report directory".into());
    }
    fs::create_dir_all(&output)?;
    let engine = Engine::new()?;
    let mut scenes = Vec::new();
    for scene in SCENES {
        let source = Arc::new(Source::open(root.join(format!("{scene}.x3f")))?);
        let directory = output.join(scene);
        fs::create_dir(&directory)?;
        let mut runs = Vec::new();
        let mut assets = serde_json::Map::new();
        for round in 0..ROUNDS {
            for step in 0..VARIANTS.len() {
                // Rotate the order so one variant does not always follow the same work.
                let variant = VARIANTS[(step + round) % VARIANTS.len()];
                let settings = settings(variant);
                let mut profile = Profile::new(Mode::Wall);
                let before = engine.gpu_activity();
                let (image, milliseconds) =
                    complete(&engine, || engine.render(&source, &settings, &mut profile))?;
                runs.push(json!({"variant":variant,"round":round,"warmup":round==0,"milliseconds":milliseconds,"profile":profile,"activity":engine.gpu_activity().since(before),"settings":settings}));
                if round == ROUNDS - 1 {
                    assets.insert(variant.into(), asset(&engine, &image, &directory, variant)?);
                }
                eprintln!("{scene} {variant} round {round}: {milliseconds:.1} ms");
            }
        }
        let mut sessions = Vec::new();
        for variant in ["reduced", "skip-speckle"] {
            let mut session = RenderSession::new(source.clone());
            let mut selected = settings(variant);
            let (_, first) = complete(&engine, || session_image(&engine, &mut session, &selected))?;
            selected.exposure = 0.5;
            let (_, edit) = complete(&engine, || session_image(&engine, &mut session, &selected))?;
            let mut full = settings("full");
            full.exposure = selected.exposure;
            let (_, refinement) =
                complete(&engine, || session_image(&engine, &mut session, &full))?;
            sessions.push(json!({"variant":variant,"first_ms":first,"exposure_edit_ms":edit,"full_refinement_ms":refinement}));
        }
        scenes.push(json!({"name":scene,"load":source.load_timings(),"runs":runs,"assets":assets,"sessions":sessions}));
    }
    let report = json!({"adapter":engine.adapter_name(),"method":"Completed render and view texture, decode/export/JPEG excluded. One warmup and three fresh trials per variant. Explicit Sunlight/Standard, Noise and Detail requested on and bypassed in Fast, no sidecars. JPEG assets are sRGB with embedded ICC at quality 100.","scenes":scenes});
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    // Embed the data so the report also opens directly from local storage.
    let data = serde_json::to_string(&report)?.replace('<', "\\u003c");
    fs::write(
        output.join("index.html"),
        include_str!("compare_merrill_preview.html").replace("REPORT_DATA", &data),
    )?;
    println!("{}", output.join("index.html").display());
    Ok(())
}
