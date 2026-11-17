//! Verifies a finite photo queue with one device and fresh source preparation and graphs.

use foveonic::{
    profile::{self, Mode, Profile, RenderSettingsSnapshot},
    render::{Engine, Settings},
    source::Source,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    error::Error,
    fs::OpenOptions,
    hash::Hasher,
    io::Write,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    id: String,
    requests: Vec<Request>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    id: String,
    source: PathBuf,
    settings_snapshot: PathBuf,
    golden: PathBuf,
    events: PathBuf,
    terminal: PathBuf,
}

fn simple_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
}

fn validate(manifest: &Manifest) -> Result<Vec<Settings>> {
    if manifest.schema_version != 1 || !simple_id(&manifest.id) || manifest.requests.is_empty() {
        return Err(
            "verification batch needs schema 1, a simple identity and a finite nonempty queue"
                .into(),
        );
    }
    let mut ids = HashSet::new();
    let mut outputs = HashSet::<PathBuf>::new();
    let mut settings = Vec::new();
    for request in &manifest.requests {
        if !simple_id(&request.id) || !ids.insert(&request.id) {
            return Err("verification request identities must be unique simple names".into());
        }
        for path in [
            &request.source,
            &request.settings_snapshot,
            &request.golden,
            &request.events,
            &request.terminal,
        ] {
            if !path.is_absolute()
                || path
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
            {
                return Err("verification paths must be absolute without parent traversal".into());
            }
        }
        for path in [&request.golden, &request.events, &request.terminal] {
            if path.exists() || !outputs.insert(path.to_path_buf()) {
                return Err("verification output paths must be distinct and new".into());
            }
        }
        let snapshot: RenderSettingsSnapshot =
            serde_json::from_slice(&std::fs::read(&request.settings_snapshot)?)?;
        settings.push(snapshot.into_settings());
    }
    if manifest
        .requests
        .iter()
        .any(|r| outputs.contains(&r.source) || outputs.contains(&r.settings_snapshot))
    {
        return Err("verification outputs must not alias inputs".into());
    }
    Ok(settings)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::create_dir_all(path.parent().ok_or("verification output has no parent")?)?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(bytes)?;
    Ok(())
}

fn terminal(
    manifest: &Manifest,
    request: &Request,
    index: usize,
    outcome: &str,
    failure: Option<&str>,
) -> Result<()> {
    let value = json!({"schema_version":1,"batch_id":manifest.id,"request_id":request.id,
        "index":index,"count":manifest.requests.len(),"source":request.source,
        "settings_snapshot":request.settings_snapshot,"golden":request.golden,"events":request.events,
        "pid":std::process::id(),"outcome":outcome,"passed":outcome=="passed","failure":failure,
        "execution":"fresh","source_reopened":outcome=="passed","fresh_executor":outcome=="passed","timing_valid":false});
    write_new(&request.terminal, &serde_json::to_vec_pretty(&value)?)
}

fn render(engine: &Engine, request: &Request, settings: &Settings) -> Result<()> {
    write_new(
        &request.events,
        format!(
            "{}\n",
            json!({"event":"verification_request_started",
        "data":{"id":request.id,"pid":std::process::id()}})
        )
        .as_bytes(),
    )?;
    let source = Source::open(&request.source)?;
    let image = engine.render(&source, settings, &mut Profile::default())?;
    engine.device().poll(wgpu::PollType::Wait)?;
    let bytes = engine.readback_image(&image)?;
    let mut hasher = std::hash::DefaultHasher::new();
    hasher.write(&bytes);
    write_new(&request.golden, &bytes)?;
    let description = image.description();
    let view = image.view();
    let records: [(&str, Value); 3] = [
        (
            "session",
            json!({"frontend":"independent_verification","source":request.source,
            "adapter":engine.adapter_name(),"settings":settings,"processing_quality":settings.processing_quality}),
        ),
        (
            "processing",
            json!({"source":request.source,"settings":settings,"execution":"fresh",
            "effective_scale_divisor":image.settings().working_scale.divisor()}),
        ),
        (
            "validation",
            json!({"source":request.source,"digest":format!("{:016x}",hasher.finish()),
            "extent":description.extent,"layout":format!("{:?}",description.layout),
            "space":format!("{:?}",description.space),"linear_white":description.linear_white,
            "view_baked":description.view_baked,"endianness":if cfg!(target_endian="little") {"little"} else {"big"},
            "view_extent":view.extent,"view_crop":view.crop,"view_rotation":view.rotation,"outside_timing":true}),
        ),
    ];
    let events = records
        .into_iter()
        .map(|(name, data)| format!("{}\n", profile::report(name, Mode::Off, data)))
        .collect::<String>();
    OpenOptions::new()
        .append(true)
        .open(&request.events)?
        .write_all(events.as_bytes())?;
    Ok(())
}

pub fn run(path: &Path) -> Result<()> {
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(path)?)?;
    let settings = validate(&manifest)?;
    let engine = Engine::new();
    let mut failure = engine.as_ref().err().map(ToString::to_string);
    for (index, (request, settings)) in manifest.requests.iter().zip(&settings).enumerate() {
        if failure.is_some() && index > 0 {
            terminal(&manifest, request, index, "unrun", failure.as_deref())?;
            continue;
        }
        let outcome = match &engine {
            Ok(engine) => {
                // A new executor owns empty preparation and statistics, sharing
                // only device/kernel resources with the batch's seed engine.
                render(&engine.executor(), request, settings)
            }
            Err(error) => Err(error.to_string().into()),
        };
        match outcome {
            Ok(()) => terminal(&manifest, request, index, "passed", None)?,
            Err(error) => {
                failure = Some(error.to_string());
                terminal(&manifest, request, index, "failed", failure.as_deref())?;
            }
        }
    }
    if let Some(failure) = failure {
        return Err(failure.into());
    }
    println!(
        "{}",
        json!({"event":"verification_batch","data":{"id":manifest.id,
        "requests":manifest.requests.len(),"pid":std::process::id(),"passed":true,"timing_valid":false}})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_and_manifest_shape_are_strict() {
        assert!(simple_id("photo-00_A"));
        assert!(!simple_id("../photo"));
        assert!(!simple_id(""));
        assert!(serde_json::from_value::<Manifest>(
            json!({"schema_version":1,"id":"a","requests":[],"extra":true})
        )
        .is_err());
        let empty = Manifest {
            schema_version: 1,
            id: "a".into(),
            requests: vec![],
        };
        assert!(validate(&empty).is_err());
    }
    fn fixture(root: &Path, id: &str, exposure: f32) -> Request {
        let settings = Settings {
            exposure,
            ..Settings::default()
        };
        let snapshot = root.join(format!("{id}-settings.json"));
        std::fs::write(
            &snapshot,
            serde_json::to_vec(&RenderSettingsSnapshot::from(&settings)).unwrap(),
        )
        .unwrap();
        Request {
            id: id.into(),
            source: root.join("source.x3f"),
            settings_snapshot: snapshot,
            golden: root.join(format!("{id}.f32")),
            events: root.join(format!("{id}.jsonl")),
            terminal: root.join(format!("{id}-terminal.json")),
        }
    }

    #[test]
    fn complete_queue_is_validated_before_any_output_or_device() {
        let root = tempfile::tempdir().unwrap();
        let mut manifest = Manifest {
            schema_version: 1,
            id: "batch".into(),
            requests: vec![
                fixture(root.path(), "a", 0.25),
                fixture(root.path(), "b", -0.5),
                fixture(root.path(), "repeat-a", 0.25),
            ],
        };
        let settings = validate(&manifest).unwrap();
        assert_eq!(
            settings.iter().map(|s| s.exposure).collect::<Vec<_>>(),
            [0.25, -0.5, 0.25]
        );
        manifest.requests[2].id = "a".into();
        assert!(validate(&manifest).is_err());
        assert!(!manifest.requests[0].golden.exists());
        manifest.requests[2].id = "repeat-a".into();
        std::fs::write(&manifest.requests[2].settings_snapshot, b"invalid JSON").unwrap();
        assert!(validate(&manifest).is_err());
        assert!(!manifest.requests[0].events.exists());
    }

    #[test]
    fn aliased_existing_and_relative_outputs_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let mut manifest = Manifest {
            schema_version: 1,
            id: "batch".into(),
            requests: vec![fixture(root.path(), "a", 0.)],
        };
        manifest.requests[0].events = manifest.requests[0].golden.clone();
        assert!(validate(&manifest).is_err());
        manifest.requests[0] = fixture(root.path(), "a", 0.);
        std::fs::write(&manifest.requests[0].golden, b"keep").unwrap();
        assert!(validate(&manifest).is_err());
        assert_eq!(
            std::fs::read(&manifest.requests[0].golden).unwrap(),
            b"keep"
        );
        manifest.requests[0].golden = PathBuf::from("relative.f32");
        assert!(validate(&manifest).is_err());
    }
}
