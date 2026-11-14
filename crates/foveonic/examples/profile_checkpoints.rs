//! Measures independent checkpoint edit sequences with optional exact replay.

use foveonic::{
    image::RenderedImage,
    inspection::Content,
    profile::{self, Mode, Profile, RenderSettingsSnapshot},
    render::{ControlCapabilities, Engine, RenderSession, Scale, Settings},
    source::Source,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    label: String,
    source: PathBuf,
    #[serde(default = "empty_object")]
    settings: Value,
    native_region: Option<[u32; 4]>,
    edits: Vec<Edit>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Edit {
    label: String,
    a: Value,
    b: Value,
}
fn empty_object() -> Value {
    json!({})
}

// Edit patches change authored settings. Scale and region belong to the run.
fn merge(target: &mut Value, patch: &Value) -> Result<()> {
    let target = target
        .as_object_mut()
        .ok_or("patch target is not an object")?;
    let patch = patch
        .as_object()
        .ok_or("settings patch must be an object")?;
    for (key, value) in patch {
        let slot = target
            .get_mut(key)
            .ok_or_else(|| format!("unknown settings key: {key}"))?;
        if value.is_object() && slot.is_object() {
            merge(slot, value)?;
        } else if value.is_object() && slot.is_null() {
            return Err(format!("cannot validate object patch for unset setting: {key}").into());
        } else {
            *slot = value.clone();
        }
    }
    Ok(())
}
fn patch_settings(settings: &Settings, patch: &Value) -> Result<Settings> {
    let mut snapshot = serde_json::to_value(RenderSettingsSnapshot::from(settings))?;
    merge(&mut snapshot["settings"], patch)?;
    Ok(serde_json::from_value::<RenderSettingsSnapshot>(snapshot)?.into_settings())
}
fn request_settings(
    mut settings: Settings,
    source: &Source,
    scale: Scale,
    region: Option<[u32; 4]>,
) -> Result<Settings> {
    // Validate authored choices before applying the transient view request.
    settings.validate_for_camera(&source.metadata)?;
    settings.working_scale = scale;
    settings.preview_region = region;
    Ok(settings)
}
fn image_data(image: &RenderedImage) -> Value {
    let d = image.description();
    let v = image.view();
    json!({"extent":d.extent,"layout":format!("{:?}",d.layout),"space":format!("{:?}",d.space),"linear_white":d.linear_white,"view_baked":d.view_baked,"byte_len":d.byte_len(),"view_extent":v.extent,"view_crop":v.crop,"view_rotation":v.rotation,"actual_region":image.settings().preview_region})
}
fn render(
    engine: &Engine,
    session: &mut RenderSession,
    settings: &Settings,
    profile: &mut Profile,
) -> Result<RenderedImage> {
    let mut artifacts = engine
        .render_session(session, settings, &["final"], || false, profile)?
        .ok_or("unexpected cancelled render")?;
    let Content::Image(image) = artifacts.remove(0).content else {
        return Err("final output is not an image".into());
    };
    Ok(*image)
}
fn emit(out: &mut impl Write, event: &str, mode: Mode, data: Value) -> Result<()> {
    writeln!(out, "{}", profile::report(event, mode, data))?;
    Ok(())
}
fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let cases_path = PathBuf::from(
        args.next()
            .ok_or("usage: profile_checkpoints CASES.json OUTPUT.jsonl [options]")?,
    );
    let output = PathBuf::from(args.next().ok_or("missing output")?);
    let (mut mode, mut repeats, mut scale, mut regional, mut verify) =
        (Mode::Off, 1usize, Scale::NATIVE, false, false);
    let (mut case_filter, mut control_filter) = (None, None);
    let mut processing_quality = foveonic::render::ProcessingQuality::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mode" => mode = args.next().ok_or("missing mode")?.parse()?,
            "--processing-quality" => {
                processing_quality = serde_json::from_value(Value::String(
                    args.next().ok_or("missing processing quality")?,
                ))?
            }
            "--repeats" => repeats = args.next().ok_or("missing repeats")?.parse()?,
            "--scale-divisor" => scale = Scale::new(args.next().ok_or("missing scale")?.parse()?)?,
            "--regional" => regional = true,
            "--verify" => verify = true,
            "--case" => case_filter = Some(args.next().ok_or("missing case label")?),
            "--control" => control_filter = Some(args.next().ok_or("missing control label")?),
            _ => return Err(format!("unknown option: {arg}").into()),
        }
    }
    if repeats == 0 {
        return Err("repeats must be positive".into());
    }
    if regional && scale != Scale::NATIVE {
        return Err("regional rendering requires native scale".into());
    }
    let raw: Value = serde_json::from_slice(&std::fs::read(cases_path)?)?;
    let cases: Vec<Case> = serde_json::from_value(if raw.is_array() {
        raw
    } else {
        raw.get("cases")
            .cloned()
            .ok_or("cases must be an array or contain cases")?
    })?;
    let mut out = BufWriter::new(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)?,
    );
    let mut selected = 0;
    for case in cases
        .iter()
        .filter(|c| case_filter.as_ref().is_none_or(|s| s == &c.label))
    {
        let edits: Vec<_> = case
            .edits
            .iter()
            .filter(|e| control_filter.as_ref().is_none_or(|s| s == &e.label))
            .collect();
        if edits.is_empty() {
            continue;
        }
        selected += 1;
        let source = Arc::new(Source::open(&case.source)?);
        let mut initial = Settings::new("Sunlight", "Standard");
        initial.processing_quality = processing_quality;
        ControlCapabilities::from_metadata(&source.metadata)?
            .apply_capture_defaults(&mut initial, &source.metadata)?;
        let authored = patch_settings(&initial, &case.settings)?;
        let region = if regional {
            Some(
                case.native_region
                    .ok_or("regional case missing native_region")?,
            )
        } else {
            None
        };
        let initial = request_settings(authored.clone(), &source, scale, region)?;
        let edits: Vec<_> = edits
            .iter()
            .map(|edit| -> Result<_> {
                Ok((
                    edit.label.as_str(),
                    request_settings(patch_settings(&authored, &edit.a)?, &source, scale, region)?,
                    request_settings(patch_settings(&authored, &edit.b)?, &source, scale, region)?,
                ))
            })
            .collect::<Result<_>>()?;
        let controls: Vec<_> = edits.iter().map(|(label, _, _)| *label).collect();
        // Compile the selected shader variants before independent measured sessions.
        let warm = Engine::new()?;
        mode.check_gpu_features(warm.device().features())?;
        for (_, a, b) in &edits {
            let mut session = RenderSession::new(source.clone());
            for settings in [&initial, a, b] {
                drop(render(
                    &warm,
                    &mut session,
                    settings,
                    &mut Profile::default(),
                )?);
            }
        }
        warm.device().poll(wgpu::Maintain::Wait);
        warm.clear_preparation();
        let source_identity = match source.origin() {
            foveonic::source::SourceOrigin::File(identity) => Some(identity),
            foveonic::source::SourceOrigin::Memory(_) => None,
        };
        emit(
            &mut out,
            "checkpoint_session",
            mode,
            json!({"case":case.label,"source":case.source,"source_identity":source_identity,"settings":RenderSettingsSnapshot::from(&initial),"processing_quality":processing_quality,"scale_divisor":scale.divisor(),"region":initial.preview_region,"adapter":warm.adapter_name(),"repeats":repeats,"verify":verify,"timing_usable":!verify && mode == Mode::Off,"source_loaded_outside_timing":true,"snapshots_outside_timing":true,"controls":controls}),
        )?;
        for (control, a, b) in edits {
            for trial in 0..repeats {
                let engine = warm.executor();
                mode.check_gpu_features(engine.device().features())?;
                let mut session = RenderSession::new(source.clone());
                for (phase, settings) in [
                    ("base", &initial),
                    ("unchanged", &initial),
                    ("edit-a", &a),
                    ("edit-b", &b),
                    ("return-a", &a),
                ] {
                    let mut profile = Profile::new(mode);
                    let before = engine.gpu_activity();
                    let started = Instant::now();
                    let image = render(&engine, &mut session, settings, &mut profile)?;
                    let render_call_ms = started.elapsed().as_secs_f64() * 1000.;
                    engine.device().poll(wgpu::Maintain::Wait);
                    let completed_ms = started.elapsed().as_secs_f64() * 1000.;
                    let activity = engine.gpu_activity().since(before);
                    let geometry = image_data(&image);
                    emit(
                        &mut out,
                        "checkpoint_render",
                        mode,
                        json!({"case":case.label,"trial":trial,"control":control,"phase":phase,"completed_ms":completed_ms,"render_call_ms":render_call_ms,"profile":profile,"activity":activity,"checkpoints":session.checkpoints(),"resources":session.resources(),"engine_preparation_bytes":engine.cached_preparation_bytes(),"settings":RenderSettingsSnapshot::from(settings),"rendered_image":geometry}),
                    )?;
                    if verify {
                        let fresh = warm.executor();
                        let mut fresh_session = RenderSession::new(source.clone());
                        let expected = render(
                            &fresh,
                            &mut fresh_session,
                            settings,
                            &mut Profile::default(),
                        )?;
                        fresh.device().poll(wgpu::Maintain::Wait);
                        let equal = geometry == image_data(&expected)
                            && engine.readback_image(&image)? == fresh.readback_image(&expected)?;
                        emit(
                            &mut out,
                            "checkpoint_verify",
                            mode,
                            json!({"case":case.label,"trial":trial,"control":control,"phase":phase,"equal":equal,"outside_timing":true}),
                        )?;
                        if !equal {
                            return Err(format!(
                                "verification failed: {} {} {phase}",
                                case.label, control
                            )
                            .into());
                        }
                    }
                }
            }
        }
    }
    if selected == 0 {
        return Err("no matching case and control".into());
    }
    out.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn patch_preserves_transients_and_rejects_unknown_keys() {
        let mut settings = Settings::new("Sunlight", "Standard");
        settings.working_scale = Scale::new(4).unwrap();
        settings.processing_quality = foveonic::render::ProcessingQuality::Speed;
        settings.preview_region = Some([1, 2, 10, 12]);
        let changed = patch_settings(&settings, &json!({"exposure":0.25})).unwrap();
        assert_eq!(changed.exposure, 0.25);
        assert_eq!(changed.working_scale, settings.working_scale);
        assert_eq!(changed.processing_quality, settings.processing_quality);
        assert_eq!(changed.preview_region, settings.preview_region);
        assert!(patch_settings(&settings, &json!({"exposuer":1})).is_err());
        assert!(patch_settings(&settings, &json!({"tone":{"unknown":1}})).is_err());
        assert!(patch_settings(&settings, &json!({"preview_region":null})).is_err());
    }
}
