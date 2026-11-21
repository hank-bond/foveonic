//! Measures requested and actual grids, cache execution and completed quality switches.

use super::*;
use crate::{
    inspection::Content,
    profile::{Mode, Profile},
    render::{BaseResponse, ControlCapabilities, Engine, PhotographicMethod, Scale},
};

#[test]
#[ignore = "requires GPU and SESSION_SCALE_X3F"]
fn session_scale_switch_diagnostic() {
    let source = Arc::new(Source::open(std::env::var_os("SESSION_SCALE_X3F").unwrap()).unwrap());
    let capabilities = ControlCapabilities::from_metadata(&source.metadata).unwrap();
    let engine = Engine::new().unwrap();
    for look in ["camera", "camera-auto-haze", "natural-auto-haze"] {
        let adaptive = look == "natural-auto-haze";
        let mut settings = Settings::new("Auto", "Standard");
        capabilities
            .apply_capture_defaults(&mut settings, &source.metadata)
            .unwrap();
        if look != "camera" {
            settings.dehaze.shadow = None;
            settings.dehaze_estimate = true;
        }
        if adaptive {
            settings.photographic.tone_method = PhotographicMethod::Universal;
            settings.photographic.base_response = BaseResponse::Natural;
        }
        let mut session = NativePhoto::new(source.clone());
        for (step, (quality, exposure)) in [
            (Scale::new(2).unwrap(), 0.),
            (Scale::new(2).unwrap(), 0.25),
            (Scale::new(2).unwrap(), 0.5),
            (Scale::new(4).unwrap(), 0.5),
            (Scale::new(4).unwrap(), 0.75),
            (Scale::new(4).unwrap(), 1.),
            (Scale::new(8).unwrap(), 1.),
            (Scale::new(8).unwrap(), 1.25),
            (Scale::new(8).unwrap(), 1.5),
            (Scale::new(4).unwrap(), 1.5),
            (Scale::new(8).unwrap(), 1.5),
            (Scale::new(2).unwrap(), 1.5),
        ]
        .into_iter()
        .enumerate()
        {
            settings.working_scale = quality;
            settings.exposure = exposure;
            settings.exposure_auto = false;
            let mut profile = Profile::new(Mode::Wall);
            let before = engine.gpu_activity();
            let started = std::time::Instant::now();
            let artifact = engine
                .render_native_photo(&mut session, &settings, &["final"], || false, &mut profile)
                .unwrap()
                .unwrap()
                .pop()
                .unwrap();
            let Content::Image(image) = artifact.content else {
                panic!("photo")
            };
            let texture = engine
                .view_texture(&image, None, &mut Profile::default())
                .unwrap();
            engine
                .device()
                .poll(wgpu::PollType::Wait)
                .expect("GPU polling failed");
            let completed_ms = started.elapsed().as_secs_f64() * 1000.;
            let activity = engine.gpu_activity().since(before);
            if step > 0 && ![3, 6].contains(&step) {
                assert!(profile.nodes.iter().all(|node| !node.executed || !matches!(node.operation, "Source" | "SourcePhaseCells" | "PoolSignal")), "warm edits and recovered scale histories must reuse their exact sensor data: look={look} step={step}");
            }
            let executed: Vec<_> = profile.nodes.iter().filter(|node| node.executed).map(|node| serde_json::json!({"operation":node.operation,"outputs":node.outputs.iter().map(|o|o.name.as_str()).collect::<Vec<_>>(),"activity":node.activity,"host_ms":node.host_milliseconds})).collect();
            println!(
                "{}",
                serde_json::json!({"event":"session_scale","look":look,"adaptive":adaptive,"step":step,"requested":quality,"effective":artifact.settings.working_scale,"extent":image.description().extent,"view_extent":image.view().extent,"texture_extent":texture.description().extent,"completed_ms":completed_ms,"cache_bytes":session.cached_bytes(),"activity":activity,"executed":executed,"host":profile.host})
            );
            let bytes = engine.readback_image(&image).unwrap();
            let fresh = engine
                .render(&source, &settings, &mut Profile::default())
                .unwrap();
            assert_eq!(image.description(), fresh.description());
            assert_eq!(image.view(), fresh.view());
            assert!(
                bytes == engine.readback_image(&fresh).unwrap(),
                "cached/fresh mismatch adaptive={adaptive} step={step}"
            );
        }
    }
}
