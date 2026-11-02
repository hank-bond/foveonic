//! Measures actual shadow-neutralization inputs, regional guides and output at reported green outliers.

use super::*;

#[test]
#[ignore = "requires GPU, explicit original and new shadow diagnostic JSON path"]
fn inspect_shadow_protection() -> Result<()> {
    let source = Arc::new(Source::open(
        std::env::var_os("FOVEONIC_CHROMA_SOURCE").ok_or("source")?,
    )?);
    let output = PathBuf::from(std::env::var_os("FOVEONIC_SHADOW_DIAGNOSIS").ok_or("output")?);
    if output.exists() {
        return Err("choose a new diagnostic output".into());
    }
    let gpu = Gpu::new()?;
    let engine = Engine::from_device(
        gpu.device.clone(),
        gpu.queue.clone(),
        gpu.adapter_name.clone(),
    )?;
    let mut settings = Settings::new(source.metadata.recorded_white_balance()?, "Standard");
    settings.merrill_noise_method = MerrillNoiseMethod::MultiscaleRowsAndColumns;
    settings.merrill_banding.method = MerrillBandingMethod::Directional;
    settings.reconstruction = Some(crate::noise::Controls {
        chroma: 4,
        luminance: 2,
        ..Default::default()
    });
    settings.neutralize_shadows.strength = 1.;
    let points = [[539, 76], [583, 85], [595, 98], [509, 245], [597, 234]];
    let mut session = RenderSession::new(source);
    let mut runs = Vec::new();
    for (id, tolerance, width) in [
        ("default-selectivity", 0.25, 35.),
        ("wider-color-tolerance", 1., 35.),
        ("wider-hue", 0.25, 75.),
        ("widest-selectivity", 1., 75.),
    ] {
        settings.neutralize_shadows.color_tolerance = tolerance;
        settings.neutralize_shadows.hue_width = width;
        let artifacts = engine
            .render_session(
                &mut session,
                &settings,
                &["shadow-neutralize.input", "shadow-neutralize.rgb"],
                || false,
                &mut Profile::default(),
            )?
            .ok_or("cancelled")?;
        let images: Vec<_> = artifacts
            .iter()
            .map(|a| match &a.content {
                Content::Image(i) => i.as_ref(),
                _ => panic!("image required"),
            })
            .collect();
        let indices: Vec<_> = points
            .iter()
            .map(|p| {
                let q = storage(*p, images[0].view());
                (q[1] * images[0].description().extent[0] + q[0]) as usize
            })
            .collect();
        // The session releases this transient guide. Recreate it with the same
        // production kernel rather than retaining extra image state in the app.
        let guide = crate::nodes::finish::shadow_neutralize::prepare_guide(
            &gpu,
            images[0].buffer(),
            crate::nodes::finish::shadow_neutralize::Params::new(
                settings.neutralize_shadows,
                ColorSpace::from_metadata(images[0].source_metadata())?,
                1.,
            ),
            images[0].description().extent,
            &mut Profile::default(),
        )?;
        let guide = read_points(&gpu, &guide, &indices, 4)?;
        runs.push(json!({"id":id,"controls":settings.neutralize_shadows,"points":points,"before_linear_srgb":capture(&gpu,images[0],&points)?,"after_linear_srgb":capture(&gpu,images[1],&points)?,"regional_oklab_and_y":guide.as_chunks::<4>().0}));
    }
    std::fs::write(output, serde_json::to_vec_pretty(&json!({"runs":runs}))?)?;
    Ok(())
}
