//! Checks manual GPU capture without gate waits and preserves explicit synchronized measurements.

use super::*;
use crate::{
    background,
    diagnostics::{CaptureConfig, Recorder},
    profile::{Mode, Profile},
};
use std::{path::PathBuf, sync::mpsc, time::Duration};

fn encode(gpu: &Gpu, trace: &mut GpuTrace, pipeline: &wgpu::ComputePipeline) {
    let first = trace.len();
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Diagnostic test pass"),
            timestamp_writes: trace.writes(super::super::Kernel::Color),
        });
        pass.set_pipeline(pipeline);
        pass.dispatch_workgroups(1, 1, 1);
    }
    let submission = gpu.submit_buffer(encoder, false);
    trace.submitted(first, submission);
}

#[test]
#[ignore = "requires a GPU with timestamps, run serialized"]
fn manual_capture_resolves_on_normal_poll_without_a_foreground_gate_wait() {
    let gpu = Gpu::new().unwrap();
    assert!(gpu
        .device
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY));
    let shader = gpu
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Diagnostic test shader"),
            source: wgpu::ShaderSource::Wgsl("@compute @workgroup_size(1) fn main() {}".into()),
        });
    let pipeline = gpu
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Diagnostic test pipeline"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
    let temporary = tempfile::tempdir().unwrap();
    let recorder = Recorder::new();
    recorder
        .start(CaptureConfig {
            mode: Mode::Gpu,
            ..Default::default()
        })
        .unwrap()
        .recv()
        .unwrap()
        .unwrap();
    let mut trace = gpu.trace(false, 1);
    {
        let _node = diag::enter(Context {
            task: 7,
            revision: 11,
            photo: 3,
            purpose: "current",
            node: Some(42),
        });
        encode(&gpu, &mut trace, &pipeline);
    }
    assert_eq!(trace.passes[0].context.unwrap().node, Some(42));
    let executor = gpu.executor();
    assert!(Arc::ptr_eq(&gpu.diagnostics, &executor.diagnostics));
    let gate = Arc::new(background::Gate::default());
    let foreground = gate.foreground();
    let (send, receive) = mpsc::channel();
    let (trace, profile) = std::thread::scope(|scope| {
        let job = scope.spawn(move || {
            background::run(Some(gate), || {
                let mut profile = Profile::default();
                executor.finish_trace(&trace, &mut profile).unwrap();
                send.send(()).unwrap();
                (trace, profile)
            })
        });
        let completed = receive.recv_timeout(Duration::from_secs(5));
        // Always release the gate before joining if a regression reintroduces blocking.
        drop(foreground);
        let result = job.join().unwrap();
        completed.expect("capture timestamp resolution waited behind the foreground gate");
        result
    });
    assert!(
        profile.gpu.is_empty(),
        "manual capture must not select the synchronous profiler"
    );
    // This represents the application's existing completion/presentation polling;
    // finish_capture_trace itself must never poll or wait for its readback.
    gpu.device.poll(wgpu::Maintain::Wait);
    assert!(trace
        .capture
        .as_ref()
        .unwrap()
        .complete
        .load(Ordering::Acquire));
    drop(trace);
    assert_eq!(gpu.diagnostics.pending.load(Ordering::Acquire), 0);

    let mut legacy = gpu.trace(true, 1);
    encode(&gpu, &mut legacy, &pipeline);
    let mut profile = Profile::new(Mode::Gpu);
    gpu.finish_trace(&legacy, &mut profile).unwrap();
    assert_eq!(
        profile.gpu.len(),
        1,
        "explicit scientific GPU profiling retains its result"
    );
    drop(legacy);
    assert_eq!(gpu.diagnostics.pending.load(Ordering::Acquire), 0);
    let directory = std::env::var_os("FOVEONIC_GPU_DIAGNOSTIC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().join("gpu"));
    recorder
        .stop_save(
            directory,
            serde_json::json!({"scenario":"async-gpu-timestamp-test","gpu_observations":"device"}),
        )
        .unwrap()
        .recv()
        .unwrap()
        .unwrap();
}

#[test]
#[ignore = "requires a GPU with timestamps, run serialized"]
fn executor_clones_share_one_bounded_capture_query_budget() {
    let gpu = Gpu::new().unwrap();
    assert!(gpu
        .device
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY));
    let recorder = Recorder::new();
    let temporary = tempfile::tempdir().unwrap();
    recorder
        .start(CaptureConfig {
            mode: Mode::Gpu,
            ..Default::default()
        })
        .unwrap()
        .recv()
        .unwrap()
        .unwrap();
    let executor = gpu.executor();
    // The observed native Quattro burst needs 23 tiny traces before a normal
    // poll services callbacks. Both executor handles use the same admission cap.
    let mut traces: Vec<_> = (0..23)
        .map(|index| {
            let trace = if index % 2 == 0 {
                gpu.trace(false, 1)
            } else {
                executor.trace(false, 1)
            };
            assert!(trace.query.is_some(), "normal 23-trace burst was dropped");
            trace
        })
        .collect();
    assert_eq!(executor.diagnostics.pending.load(Ordering::Acquire), 23);
    for _ in traces.len()..MAX_PENDING {
        let trace = executor.trace(false, 1);
        assert!(trace.query.is_some());
        traces.push(trace);
    }
    assert_eq!(
        executor.diagnostics.pending.load(Ordering::Acquire),
        MAX_PENDING
    );
    assert!(gpu.trace(false, 1).query.is_none());
    assert!(executor.trace(false, 1).query.is_none());
    drop(traces);
    assert_eq!(gpu.diagnostics.pending.load(Ordering::Acquire), 0);
    assert!(executor.trace(false, 1).query.is_some());
    recorder
        .stop_save(
            temporary.path().join("budget"),
            serde_json::json!({"scenario":"gpu-query-budget"}),
        )
        .unwrap()
        .recv()
        .unwrap()
        .unwrap();
}
