//! Checks first-presentation admission, background preparation, progress and failure settlement.

use super::*;
use eframe::Storage as _;

#[test]
fn preparation_waits_for_presentation_and_recorder_without_blocking_updates() {
    let context = egui::Context::default();
    let (began, started) = mpsc::channel();
    let (finish, released) = mpsc::channel();
    let mut preparation = Preparation::new(move |progress| {
        *progress.lock().unwrap() = Progress {
            completed: 2,
            total: 4,
        };
        began
            .send(std::thread::current().name().unwrap().to_owned())
            .unwrap();
        released.recv().unwrap();
        Ok(42)
    });
    preparation.start_if_ready(false, true, &context).unwrap();
    preparation.start_if_ready(true, false, &context).unwrap();
    assert!(preparation.work.is_some());
    assert!(started.try_recv().is_err());
    preparation.start_if_ready(true, true, &context).unwrap();
    assert_eq!(
        started.recv_timeout(Duration::from_secs(2)).unwrap(),
        "graphics-startup"
    );
    assert!(preparation.receive().is_none());
    let progress = *preparation.progress.lock().unwrap();
    assert_eq!((progress.completed, progress.total), (2, 4));
    for _ in 0..3 {
        let output = context.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("Responsive while preparing");
            });
        });
        assert!(!output.shapes.is_empty());
        preparation.start_if_ready(true, true, &context).unwrap();
    }
    finish.send(()).unwrap();
    assert_eq!(receive(&mut preparation).unwrap(), 42);
    assert!(preparation.receive().is_none());
}

fn receive<T: Send + 'static>(preparation: &mut Preparation<T>) -> Result<T, String> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(result) = preparation.receive() {
            return result;
        }
        assert!(Instant::now() < deadline, "preparation did not settle");
        std::thread::yield_now();
    }
}

#[test]
fn failures_and_worker_disconnects_settle_once() {
    let context = egui::Context::default();
    let mut failed = Preparation::<()>::new(|_| Err("unsupported graphics device".into()));
    failed.start_if_ready(true, true, &context).unwrap();
    assert_eq!(
        receive(&mut failed).unwrap_err(),
        "unsupported graphics device"
    );
    assert!(failed.receive().is_none());
    let mut panicked = Preparation::<()>::new(|_| panic!("injected startup failure"));
    panicked.start_if_ready(true, true, &context).unwrap();
    assert!(receive(&mut panicked)
        .unwrap_err()
        .contains("stopped unexpectedly"));
    assert!(panicked.receive().is_none());
}

#[test]
fn dropping_preparation_never_joins_the_worker() {
    let context = egui::Context::default();
    let (began, started) = mpsc::channel();
    let (finish, released) = mpsc::channel();
    let (ended, ended_rx) = mpsc::channel();
    let mut preparation = Preparation::new(move |_| {
        began.send(()).unwrap();
        released.recv().unwrap();
        ended.send(()).unwrap();
        Ok(())
    });
    preparation.start_if_ready(true, true, &context).unwrap();
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    drop(preparation);
    finish.send(()).unwrap();
    ended_rx.recv_timeout(Duration::from_secs(2)).unwrap();
}

#[test]
fn first_present_is_retained_once_even_without_recording() {
    let observations = Observations::new(Instant::now());
    let paint = egui_wgpu::PaintTiming {
        stage: egui_wgpu::PaintStage::QueueSubmit,
        viewport: egui::ViewportId::ROOT,
        pass: 0,
        started: Instant::now(),
        finished: Instant::now(),
    };
    observations.paint(paint);
    assert!(!observations.presented.load(Ordering::Acquire));
    observations.paint(egui_wgpu::PaintTiming {
        stage: egui_wgpu::PaintStage::Present,
        ..paint
    });
    assert!(observations.presented.load(Ordering::Acquire));
    assert!(observations.pending.lock().unwrap().is_empty());
}

#[test]
fn application_state_can_be_prepared_off_the_native_ui_thread() {
    fn assert_send<T: Send>() {}
    assert_send::<App>();
    assert_send::<Inputs>();
}

#[test]
#[ignore = "owns global SDK capture state; run alone with --exact"]
fn early_setup_intervals_survive_asynchronous_recorder_start() {
    let temporary = tempfile::tempdir().unwrap();
    let began = Instant::now();
    let mut observations = Observations::new(began);
    Arc::get_mut(&mut observations).unwrap().requested = true;
    observations.record("startup.early-proof", began, Instant::now());
    observations.flush();
    assert_eq!(observations.pending.lock().unwrap().len(), 1);
    let mut controller = diagnostics::Controller::default();
    controller
        .start(
            trace::Mode::Wall,
            serde_json::json!({"entry":"startup-test"}),
        )
        .unwrap();
    controller.wait_for_start().unwrap();
    observations.flush();
    assert!(observations.pending.lock().unwrap().is_empty());
    controller
        .stop_to(temporary.path().join("capture"))
        .unwrap();
    controller.save_on_exit().unwrap();
    let bytes = std::fs::read(&controller.last_saved().unwrap().trace).unwrap();
    for marker in ["startup.early-proof", "host_elapsed", "duration_ns"] {
        assert!(
            bytes
                .windows(marker.len())
                .any(|bytes| bytes == marker.as_bytes()),
            "missing {marker}"
        );
    }
}

#[test]
fn compact_geometry_preserves_the_workspace_and_restores_its_modes() {
    let workspace = egui::ViewportBuilder::default()
        .with_inner_size([1240., 820.])
        .with_min_inner_size([1000., 600.])
        .with_position([130., 75.])
        .with_fullscreen(true)
        .with_maximized(true);
    let compact = compact_window(workspace.clone());
    assert_eq!(compact.inner_size, Some(WINDOW_SIZE));
    assert_eq!(compact.min_inner_size, Some(WINDOW_SIZE));
    assert_eq!(compact.position, workspace.position);
    assert_eq!(compact.fullscreen, Some(false));
    assert_eq!(compact.maximized, Some(false));
    assert_eq!(compact.resizable, Some(false));
    let window = Window::new(workspace, None);
    let commands = window.commands();
    use egui::ViewportCommand as Command;
    assert!(commands.contains(&Command::InnerSize(egui::vec2(1240., 820.))));
    assert!(commands.contains(&Command::MinInnerSize(egui::vec2(1000., 600.))));
    assert!(commands.contains(&Command::Fullscreen(true)));
    assert!(commands.contains(&Command::Maximized(true)));
    assert!(commands.contains(&Command::Resizable(true)));
    // The native window already has its restored position. Replaying the builder
    // position as an outer coordinate would move it on macOS.
    assert!(!commands
        .iter()
        .any(|command| matches!(command, Command::OuterPosition(_))));
}

#[derive(Default)]
struct Storage(std::collections::HashMap<String, String>);
impl eframe::Storage for Storage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.into(), value);
    }
    fn flush(&mut self) {}
}

#[test]
fn loading_saves_preserve_opaque_geometry_including_absent_and_malformed_settings() {
    for original in [
        None,
        Some("(inner_size_points:Some((x:1240.0,y:820.0)))"),
        Some("older unsupported settings"),
    ] {
        let mut storage = Storage::default();
        if let Some(original) = original {
            storage.set_string(WINDOW_STORAGE_KEY, original.into());
        }
        let window = Window::new(egui::ViewportBuilder::default(), Some(&storage));
        // Eframe's pinned save order: native geometry first, then App::save.
        for _ in 0..2 {
            storage.set_string(WINDOW_STORAGE_KEY, "temporary compact geometry".into());
            window.preserve(&mut storage);
            assert_eq!(
                storage.get_string(WINDOW_STORAGE_KEY).as_deref(),
                Some(original.unwrap_or_default())
            );
        }
        if original.is_none() {
            assert!(eframe::get_value::<serde_json::Value>(&storage, WINDOW_STORAGE_KEY).is_none());
        }
    }
}

#[test]
fn geometry_stays_protected_when_resize_is_delayed_ignored_or_observed() {
    let mut window = Window::new(egui::ViewportBuilder::default(), None);
    window.resizing_from = Some(WINDOW_SIZE);
    window.observe_size(None);
    for _ in 0..10 {
        window.observe_size(Some(WINDOW_SIZE));
    }
    assert!(window.resizing_from.is_some());
    window.observe_size(Some(egui::vec2(1240., 820.)));
    assert!(window.resizing_from.is_none());
    window.observe_size(Some(WINDOW_SIZE));
    assert!(window.resizing_from.is_none()); // Later user resizing is ordinary workspace use.
}

#[test]
fn compact_panel_uses_the_effective_appearance_and_fits_both_progress_and_errors() {
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let context = egui::Context::default();
        widgets::install_style(&context);
        context.set_theme(theme);
        let icons = crate::icon::startup_icons(&context);
        assert!(icons.iter().all(|icon| icon.size() == [256, 256]));
        let long_error = "An example graphics preparation failure. ".repeat(60);
        for (progress, error) in [
            (Progress::default(), None),
            (
                Progress {
                    completed: 75,
                    total: 303,
                },
                None,
            ),
            (
                Progress {
                    completed: 303,
                    total: 303,
                },
                None,
            ),
            (Progress::default(), Some(long_error.as_str())),
        ] {
            let output = context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW_SIZE)),
                    ..Default::default()
                },
                |context| {
                    loading_panel(context, &icons, progress, error);
                },
            );
            let chosen = icons[usize::from(theme == egui::Theme::Dark)].id();
            let primitives = context.tessellate(output.shapes.clone(), output.pixels_per_point);
            assert!(primitives
                .iter()
                .any(|primitive| matches!(&primitive.primitive,
                egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == chosen)));
            for shape in &output.shapes {
                if let egui::Shape::Text(text) = &shape.shape {
                    let visible = text.visual_bounding_rect().intersect(shape.clip_rect);
                    assert!(
                        visible.min.x >= 0. && visible.max.x <= WINDOW_SIZE.x + 1.,
                        "text outside compact window: {visible:?}"
                    );
                    assert!(
                        visible.min.y >= 0. && visible.max.y <= WINDOW_SIZE.y + 1.,
                        "text outside compact window: {visible:?}"
                    );
                }
            }
        }
    }
}

fn placement_input(outer: egui::Pos2) -> egui::RawInput {
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, WINDOW_SIZE)),
        ..Default::default()
    };
    let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
    viewport.monitor_size = Some(egui::vec2(2048., 1152.));
    viewport.outer_rect = Some(egui::Rect::from_min_size(
        outer,
        WINDOW_SIZE + egui::vec2(0., 32.),
    ));
    viewport.inner_rect = Some(egui::Rect::from_min_size(
        outer + egui::vec2(0., 32.),
        WINDOW_SIZE,
    ));
    input
}

#[test]
fn loading_centers_once_and_restores_the_actual_remembered_outer_position() {
    let context = egui::Context::default();
    let workspace = egui::ViewportBuilder::default()
        .with_inner_size([1240., 820.])
        .with_position([130., 104.]);
    let mut window = Window::new(workspace, None);
    let output = context.run(placement_input(egui::pos2(130., 72.)), |context| {
        window.place(context);
        window.place(context);
        window.restore(context);
    });
    let positions: Vec<_> = output.viewport_output[&egui::ViewportId::ROOT]
        .commands
        .iter()
        .filter_map(|command| {
            if let egui::ViewportCommand::OuterPosition(position) = command {
                Some(*position)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(positions, [egui::pos2(744., 460.), egui::pos2(130., 72.)]);
}

#[test]
fn first_launch_workspace_expands_around_the_center_and_unavailable_geometry_is_optional() {
    let context = egui::Context::default();
    let mut window = Window::new(
        egui::ViewportBuilder::default().with_inner_size([1440., 1000.]),
        None,
    );
    let _ = context.run(placement_input(egui::pos2(130., 72.)), |context| {
        window.place(context)
    });
    let output = context.run(placement_input(egui::pos2(744., 460.)), |context| {
        window.restore(context)
    });
    assert!(output.viewport_output[&egui::ViewportId::ROOT]
        .commands
        .contains(&egui::ViewportCommand::OuterPosition(egui::pos2(304., 60.))));
    let mut unavailable = Window::new(egui::ViewportBuilder::default(), None);
    let _ = egui::Context::default().run(Default::default(), |context| unavailable.place(context));
    assert!(unavailable.positioned);
    assert!(unavailable.workspace_position.is_none());
}

fn fitted(
    position: egui::Pos2,
    inner_size: egui::Vec2,
    area: egui::Rect,
) -> (egui::Pos2, egui::Vec2) {
    let context = egui::Context::default();
    let mut window = Window::new(
        egui::ViewportBuilder::default().with_inner_size(inner_size),
        None,
    );
    window.workspace_position = Some(position);
    let _ = context.run(placement_input(egui::Pos2::ZERO), |context| {
        window.fit(context, |_| Some(area))
    });
    (
        window.workspace_position.unwrap(),
        window.workspace.inner_size.unwrap(),
    )
}

#[test]
fn workspace_moves_clear_of_a_taskbar_on_any_edge() {
    let size = egui::vec2(1440., 1000.);
    let top = egui::Rect::from_min_max(egui::pos2(0., 48.), egui::pos2(1920., 1080.));
    assert_eq!(
        fitted(egui::pos2(240., 20.), size, top),
        (egui::pos2(240., 48.), size)
    );
    // The fixture's title bar adds 32 points to the outer height.
    let bottom = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1920., 1032.));
    assert_eq!(
        fitted(egui::pos2(240., 20.), size, bottom),
        (egui::pos2(240., 0.), size)
    );
    let left = egui::Rect::from_min_max(egui::pos2(62., 0.), egui::pos2(1920., 1080.));
    assert_eq!(
        fitted(egui::pos2(20., 20.), size, left),
        (egui::pos2(62., 20.), size)
    );
    // The fixture's frame adds nothing to the outer width.
    let right = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1858., 1080.));
    assert_eq!(
        fitted(egui::pos2(460., 20.), size, right),
        (egui::pos2(418., 20.), size)
    );
}

#[test]
fn workspace_fits_a_monitor_left_of_the_primary_one() {
    let size = egui::vec2(1440., 1000.);
    let area = egui::Rect::from_min_max(egui::pos2(-1920., 48.), egui::pos2(0., 1080.));
    assert_eq!(
        fitted(egui::pos2(-1500., 0.), size, area),
        (egui::pos2(-1500., 48.), size)
    );
}

#[test]
fn workspace_larger_than_the_free_area_shrinks_to_fill_it() {
    // 1920x1080 at 150% with a top taskbar.
    let area = egui::Rect::from_min_max(egui::pos2(0., 32.), egui::pos2(1280., 720.));
    assert_eq!(
        fitted(egui::pos2(-80., -156.), egui::vec2(1440., 1000.), area),
        (egui::pos2(0., 32.), egui::vec2(1280., 656.))
    );
}

#[test]
fn a_free_area_smaller_than_the_minimum_window_lowers_the_minimum() {
    // 1366x768 at 125% with a bottom taskbar leaves 1092.8 x 576 points.
    let area = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1092.8, 576.));
    let context = egui::Context::default();
    let mut window = Window::new(
        egui::ViewportBuilder::default()
            .with_inner_size([1440., 1000.])
            .with_min_inner_size([1000., 600.]),
        None,
    );
    window.workspace_position = Some(egui::pos2(-80., -100.));
    let _ = context.run(placement_input(egui::Pos2::ZERO), |context| {
        window.fit(context, |_| Some(area))
    });
    // The fixture's title bar adds 32 points to the outer height.
    assert_eq!(window.workspace.inner_size, Some(egui::vec2(1092.8, 544.)));
    assert_eq!(
        window.workspace.min_inner_size,
        Some(egui::vec2(1000., 544.))
    );
    let roomy = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1920., 1032.));
    let mut window = Window::new(
        egui::ViewportBuilder::default()
            .with_inner_size([1440., 1000.])
            .with_min_inner_size([1000., 600.]),
        None,
    );
    window.workspace_position = Some(egui::pos2(0., 0.));
    let _ = context.run(placement_input(egui::Pos2::ZERO), |context| {
        window.fit(context, |_| Some(roomy))
    });
    assert_eq!(
        window.workspace.min_inner_size,
        Some(egui::vec2(1000., 600.))
    );
}

#[test]
fn workspace_inside_the_free_area_and_unknown_areas_are_unchanged() {
    let size = egui::vec2(1240., 820.);
    let area = egui::Rect::from_min_max(egui::pos2(0., 48.), egui::pos2(1920., 1080.));
    assert_eq!(
        fitted(egui::pos2(130., 72.), size, area),
        (egui::pos2(130., 72.), size)
    );
    let context = egui::Context::default();
    let mut window = Window::new(egui::ViewportBuilder::default().with_inner_size(size), None);
    window.workspace_position = Some(egui::pos2(130., -40.));
    let _ = context.run(placement_input(egui::Pos2::ZERO), |context| {
        window.fit(context, |_| None)
    });
    assert_eq!(window.workspace_position, Some(egui::pos2(130., -40.)));
}
