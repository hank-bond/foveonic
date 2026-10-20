//! Checks color-field shaders, settings, and identical-input numerical behavior.
use super::*;
#[path = "color_field/reference/mod.rs"]
mod reference;

#[test]
fn color_field_shaders_validate() {
    use wgpu::naga::{
        front::wgsl,
        valid::{Capabilities, ValidationFlags, Validator},
    };
    for code in [
        format!(
            "{}\n{}",
            include_str!("color_field_math.wgsl"),
            include_str!("color_field.wgsl")
        ),
        include_str!("color_field_solver.wgsl").into(),
    ] {
        let module =
            wgsl::parse_str(&code).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&code)));
        Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&code)));
        if module.entry_points.iter().any(|e| e.name == "initialize") {
            for entry in &module.entry_points {
                assert_eq!(
                    entry.workgroup_size,
                    [SOLVER_GROUP_SIZE, 1, 1],
                    "solver dispatch size drifted: {}",
                    entry.name
                );
            }
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_color_field_matches_pinned_cpu_equations() {
    let gpu = Gpu::new().unwrap();
    let width = 48usize;
    let height = 32usize;
    let p = [0.284206, 0.604436, 1.0];
    let y = [-2.6575486148565814, 3.313530567081604, -0.2476];
    let matrix = [
        4.45774, -1.90728, 0.581922, -4.22818, 4.30012, -0.701463, 3.1376, -5.81127, 3.31683,
    ];
    for scene in 0..8 {
        let mut pixels = Vec::<[f32; 3]>::new();
        let mut masks = Vec::new();
        let mut repaired = vec![false; width * height];
        for row in 0..height {
            for col in 0..width {
                let mut m = [0.27, 0.60, 0.94];
                if scene == 1 && col >= 16 {
                    let a = 1. + 0.0004 * (col + row) as f32;
                    m = [0.30 * a, 0.66 * a, 1.0];
                }
                if scene == 2 {
                    m = [1.0, 1.0, 1.0];
                }
                if scene == 3 && (16..40).contains(&col) {
                    m = [0.31, 0.67, 1.0];
                }
                if scene == 3 && col < 16 {
                    repaired[row * width + col] = true;
                }
                if scene == 4 && col >= 16 {
                    m = [0.30, 0.66, 0.951 + 0.039 * (col - 16) as f32 / 31.];
                }
                if scene == 5 {
                    m = [1.0, 0.4, 1.0];
                }
                if scene == 6 && col >= 16 {
                    m = [0.99, 0.4, 1.0];
                }
                // A repaired clipped sample can need brightness recovery while
                // every color cell stays fixed. Exercise the zero-unknown path.
                if scene == 7 && row == 16 && col == 24 {
                    m = [0.30, 0.66, 1.0];
                    repaired[row * width + col] = true;
                }
                pixels.push(m);
                masks.push(m.map(|v| {
                    let v = f64::from(v);
                    if v >= 0.99 {
                        0
                    } else {
                        let t = ((0.99 - v) / 0.04).clamp(0., 1.);
                        (t * t * (3. - 2. * t) * 255.).round().clamp(1., 255.) as u8
                    }
                }));
            }
        }
        let input = gpu.upload("identical normalized samples", &pixels).unwrap();
        let black = gpu.upload("zero black", &[0_f32; 3]).unwrap();
        let prepared = Prepared {
            params: Params {
                size: [
                    width as u32,
                    height as u32,
                    (width * height) as u32,
                    repaired.iter().filter(|v| **v).count() as u32,
                ],
                grid: [6, 4, 24, 8],
                crop: [0, 0, width as u32, height as u32],
                gain: [1.; 4],
                white: [1.; 4],
                neutral: [p[0] as f32, p[1] as f32, p[2] as f32, 0.],
                luminance: [y[0] as f32, y[1] as f32, y[2] as f32, 0.],
                metric: color_metric(p, matrix).unwrap(),
                tuning: [0.035, 0.75, 1., 1.],
                options: [1, 1, 1, 0],
            },
            repairs: repaired
                .iter()
                .enumerate()
                .filter_map(|(i, v)| v.then_some(i as u32))
                .collect(),
        };
        // The CPU sees precisely the float32 parameters uploaded to the shader.
        let p = std::array::from_fn(|i| f64::from(prepared.params.neutral[i]));
        let y = std::array::from_fn(|i| f64::from(prepared.params.luminance[i]));
        let expected = reference::run(
            width,
            height,
            &pixels.iter().map(|p| p.map(f64::from)).collect::<Vec<_>>(),
            &masks,
            &repaired,
            p,
            y,
            matrix,
        )
        .unwrap();
        let mut off_profile = Profile::new(crate::profile::Mode::Off);
        let output = apply(
            &gpu,
            Inputs {
                input: &input,
                original: &input,
                black: &black,
                unrepaired: &input,
            },
            &prepared,
            &mut off_profile,
            &mut Execution::default(),
        )
        .unwrap();
        assert!(off_profile.host.is_empty() && off_profile.gpu.is_empty());
        let bytes = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        let maximum = actual
            .iter()
            .zip(expected.iter().flatten())
            .map(|(a, b)| (f64::from(*a) - b).abs())
            .fold(0_f64, f64::max);
        let worst = actual
            .iter()
            .zip(expected.iter().flatten())
            .enumerate()
            .max_by(|(_, (a, b)), (_, (c, d))| {
                (f64::from(**a) - **b)
                    .abs()
                    .total_cmp(&(f64::from(**c) - **d).abs())
            })
            .unwrap()
            .0;
        eprintln!(
            "scene {scene}: max normalized error {maximum}, worst pixel {} GPU {:?} CPU {:?}",
            worst / 3,
            &actual[worst / 3 * 3..worst / 3 * 3 + 3],
            expected[worst / 3]
        );
        assert!(
            maximum < 0.0001,
            "scene {scene} max normalized error {maximum}"
        );
        if scene == 1 {
            {
                let mut experiment = Prepared {
                    params: prepared.params,
                    repairs: prepared.repairs.clone(),
                };
                experiment.params.options[0] = 2;
                let rendered = apply(
                    &gpu,
                    Inputs {
                        input: &input,
                        original: &input,
                        black: &black,
                        unrepaired: &input,
                    },
                    &experiment,
                    &mut Profile::default(),
                    &mut Execution::default(),
                )
                .unwrap();
                let bytes = gpu.readback(&rendered).unwrap();
                let values = bytemuck::cast_slice::<u8, f32>(&bytes);
                assert!(values.iter().all(|v| v.is_finite()), "Local");
                for (i, mask) in masks.iter().enumerate() {
                    if *mask == [255; 3] {
                        assert_eq!(
                            &values[i * 3..i * 3 + 3],
                            &pixels[i],
                            "Local changed healthy pixel"
                        );
                    }
                }
            }
        }
        if scene == 0 {
            assert_eq!(
                bytemuck::cast_slice::<u8, f32>(&gpu.readback(&input).unwrap()),
                actual
            );
        }
    }
}

#[test]
#[ignore = "requires GPU and FOVEONIC_HIGHLIGHT_DIR with the frozen highlight corpus"]
fn gpu_color_field_real_crops_match_identical_input_reference() {
    check_real_inputs(false);
}

#[test]
#[ignore = "requires GPU, FOVEONIC_HIGHLIGHT_DIR, and time/memory for the full-corpus double-precision reference solves"]
fn gpu_color_field_full_rasters_match_identical_input_reference() {
    check_real_inputs(true);
}

fn check_real_inputs(full: bool) {
    use crate::{
        inspection::Content,
        render::{Engine, Settings},
        source::Source,
    };
    let directory = std::path::PathBuf::from(
        std::env::var_os("FOVEONIC_HIGHLIGHT_DIR").expect("set FOVEONIC_HIGHLIGHT_DIR"),
    );
    let engine = Engine::new().unwrap();
    let gpu = Gpu::new().unwrap();
    let names: &[&str] = if full {
        &[
            "_P2M0009.X3F",
            "_P2M0010.X3F",
            "_P2M0035.X3F",
            "_P2M0077.X3F",
            "_P2M0089.X3F",
            "_P2M0095.X3F",
            "_P2M0170.X3F",
            "_P2M0195.X3F",
            "_P2M0286.X3F",
            "_P2M0313.X3F",
            "_P2M0322.X3F",
            "_P2M0331.X3F",
            "_P2M0352.X3F",
            "_P2M0358.X3F",
            "_P2M0373.X3F",
            "_P2M0399.X3F",
        ]
    } else {
        &["_P2M0095.X3F", "_P2M0286.X3F", "_P2M0313.X3F"]
    };
    let mut failures = Vec::new();
    for &name in names {
        let source = Source::open(directory.join(name)).unwrap();
        let settings = crate::sidecar::load(&directory.join(name))
            .unwrap()
            .unwrap_or_else(|| Settings::new("Sunlight", "Standard"));
        let metadata = &source.metadata;
        let calibration =
            super::super::calibration::prepare(metadata, &settings.white_balance).unwrap();
        let mut prepared = prepare(
            metadata,
            &settings.white_balance,
            calibration.params,
            true,
            Controls::default(),
        )
        .unwrap();
        let artifacts = engine
            .render_artifacts(
                &source,
                &settings,
                &["calibration.layers", "repair.layers", "black.levels"],
                &mut Profile::new(crate::profile::Mode::Off),
            )
            .unwrap();
        let floats = |name: &str| -> Vec<f32> {
            let artifact = artifacts.iter().find(|a| a.name == name).unwrap();
            match &artifact.content {
                Content::Image(image) => {
                    bytemuck::cast_slice::<u8, f32>(&engine.readback_image(image).unwrap()).to_vec()
                }
                Content::Table { values, .. } => values.clone(),
            }
        };
        let before = floats("calibration.layers");
        let after = floats("repair.layers");
        let black = floats("black.levels");
        let crop = prepared.params.crop;
        let side = 256usize;
        let mut chosen = (crop[0] as usize, crop[1] as usize);
        let mut best = 0usize;
        for top in (crop[1] as usize..crop[3] as usize - side).step_by(side) {
            for left in (crop[0] as usize..crop[2] as usize - side).step_by(side) {
                let mut clipped = 0;
                for row in top..top + side {
                    for col in left..left + side {
                        let off = (row * metadata.width + col) * 3;
                        let stressed = (0..3).any(|c| {
                            (source.pixels[off + c] - black[c])
                                / (prepared.params.white[c] - black[c])
                                > 0.95
                        });
                        clipped += usize::from(stressed);
                    }
                }
                let score = clipped * (side * side - clipped);
                if score > best {
                    best = score;
                    chosen = (left, top);
                }
            }
        }
        let (left, top, width, height) = if full {
            (0, 0, metadata.width, metadata.height)
        } else {
            (chosen.0, chosen.1, side, side)
        };
        let mut raw = Vec::new();
        let mut calibrated = Vec::new();
        let mut unrepaired = Vec::new();
        let mut measured = Vec::new();
        let mut masks = Vec::new();
        let mut repaired = Vec::new();
        let sites: std::collections::HashSet<_> = prepared.repairs.iter().copied().collect();
        let mut local_sites = Vec::new();
        for row in top..top + height {
            for col in left..left + width {
                let pixel = row * metadata.width + col;
                let off = pixel * 3;
                raw.extend_from_slice(&source.pixels[off..off + 3]);
                calibrated.extend_from_slice(&after[off..off + 3]);
                unrepaired.extend_from_slice(&before[off..off + 3]);
                measured.push(std::array::from_fn(|c| {
                    f64::from(
                        after[off + c]
                            / (prepared.params.gain[c] * (prepared.params.white[c] - black[c])),
                    )
                }));
                masks.push(std::array::from_fn(|c| {
                    let v =
                        (source.pixels[off + c] - black[c]) / (prepared.params.white[c] - black[c]);
                    if v >= 0.99 {
                        0
                    } else {
                        let t = ((0.99 - v) / 0.04).clamp(0., 1.);
                        (t * t * (3. - 2. * t) * 255.).round().clamp(1., 255.) as u8
                    }
                }));
                let marked = sites.contains(&(pixel as u32));
                if marked {
                    local_sites.push((measured.len() - 1) as u32);
                }
                repaired.push(marked || after[off..off + 3] != before[off..off + 3]);
            }
        }
        prepared.repairs = local_sites;
        prepared.params.size = [
            width as u32,
            height as u32,
            (width * height) as u32,
            prepared.repairs.len() as u32,
        ];
        prepared.params.crop = [0, 0, width as u32, height as u32];
        let gw = width.div_ceil(8) as u32;
        let gh = height.div_ceil(8) as u32;
        prepared.params.grid = [gw, gh, gw * gh, 8];
        let p = std::array::from_fn(|c| f64::from(prepared.params.neutral[c]));
        let y = std::array::from_fn(|c| f64::from(prepared.params.luminance[c]));
        let key = metadata
            .lists
            .get("WhiteBalanceColorCorrections")
            .unwrap()
            .get(&settings.white_balance)
            .unwrap();
        let cc = metadata.fixed::<9>(key).unwrap();
        let matrix = std::array::from_fn(|i| cc[i] * f64::from(prepared.params.gain[i % 3]) * 0.5);
        let raw = gpu.upload("real crop original", &raw).unwrap();
        let input = gpu.upload("real crop repaired", &calibrated).unwrap();
        let before = gpu.upload("real crop unrepaired", &unrepaired).unwrap();
        let blacks = gpu.upload("real crop black", &black).unwrap();
        // Float32 normalization may straddle a u8 rounding boundary. Check
        // quantization independently, then give both solvers identical evidence.
        let actual_masks =
            super::reliability_tests::from_buffers(&gpu, &prepared.params, &raw, &blacks, &input);
        let mut mask_differences = 0;
        for (cpu, &packed) in masks.iter_mut().zip(&actual_masks) {
            let actual = std::array::from_fn(|c| (packed >> (8 * c)) as u8);
            for c in 0..3 {
                assert!(
                    cpu[c].abs_diff(actual[c]) <= 1,
                    "raw reliability differs by more than one bin"
                );
            }
            mask_differences += usize::from(*cpu != actual);
            *cpu = actual;
        }
        eprintln!("{name}: {mask_differences} pixels straddle reliability quantization boundaries");
        let expected =
            reference::run(width, height, &measured, &masks, &repaired, p, y, matrix).unwrap();
        let output = apply(
            &gpu,
            Inputs {
                input: &input,
                original: &raw,
                black: &blacks,
                unrepaired: &before,
            },
            &prepared,
            &mut Profile::new(crate::profile::Mode::Off),
            &mut Execution::default(),
        )
        .unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        let mut maximum = 0_f64;
        let mut worst = 0;
        let mut sum = 0.;
        for (index, (actual, expected)) in actual.iter().zip(expected.iter().flatten()).enumerate()
        {
            let c = index % 3;
            let normalized = f64::from(
                *actual / (prepared.params.gain[c] * (prepared.params.white[c] - black[c])),
            );
            let error = (normalized - expected).abs();
            if error > maximum {
                maximum = error;
                worst = index;
            }
            sum += error;
        }
        eprintln!(
            "{name} area [{left},{top},{width},{height}]: normalized max {maximum} mean {}",
            sum / actual.len() as f64
        );
        eprintln!(
            "worst source pixel ({},{}), channel {}, measured {:?}, mask {:?}, expected {:?}",
            left + (worst / 3) % width,
            top + (worst / 3) / width,
            worst % 3,
            measured[worst / 3],
            masks[worst / 3],
            expected[worst / 3]
        );
        if maximum >= 0.0002 {
            failures.push((name, maximum));
        }
        assert_eq!(
            bytemuck::cast_slice::<u8, f32>(&gpu.readback(&input).unwrap()),
            calibrated
        );
    }
    assert!(
        failures.is_empty(),
        "identical-input maximum errors exceed unchanged 2e-4 limit: {failures:?}"
    );
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_color_field_reports_cancellation_and_invalid_preconditioners() {
    use bytemuck::Zeroable;
    let gpu = Gpu::new().unwrap();
    let input = gpu.upload("cancelled input", &[1_f32; 3]).unwrap();
    let prepared = Prepared {
        params: Params::zeroed(),
        repairs: Vec::new(),
    };
    let cancelled = || true;
    let mut execution = Execution {
        cancel: Some(&cancelled),
        ..Default::default()
    };
    let before = gpu.activity();
    assert!(apply(
        &gpu,
        Inputs {
            input: &input,
            original: &input,
            black: &input,
            unrepaired: &input
        },
        &prepared,
        &mut Profile::default(),
        &mut execution
    )
    .is_err());
    assert_eq!(gpu.activity().submissions, before.submissions);
    gpu.prepare_color_field().unwrap();
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct InvalidEquation {
        data: [f32; 4],
        neighbors: [u32; 4],
        weights: [f32; 4],
    }
    let equation = InvalidEquation {
        data: [0., 1., 0., 1.],
        neighbors: [u32::MAX; 4],
        weights: [0.; 4],
    };
    let equations = gpu
        .upload("invalid SPD preconditioner", &[equation])
        .unwrap();
    let mut x = gpu
        .upload("invalid solver initial state", &[0_f32; 4])
        .unwrap();
    let error = solve(
        &gpu,
        &equations,
        &mut x,
        Solve {
            count: 1,
            metric: [1., 0., 0., 0.],
            domain: Domain::Brightness,
            fixed_rhs_max: 0.,
        },
        &mut Profile::default(),
        &mut Execution::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("invalid preconditioner"));
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_color_field_checkpoints_preserve_conjugacy_and_validate_true_residual() {
    let gpu = Gpu::new().unwrap();
    gpu.prepare_color_field().unwrap();
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Equation {
        block: [f32; 4],
        rhs: [f32; 4],
        neighbors: [u32; 4],
        weights: [f32; 4],
        neutral: [f32; 4],
    }
    assert_eq!(std::mem::size_of::<Equation>(), 80);
    let equation = Equation {
        block: [1., 1., 0., -1.],
        rhs: [2., 0., 1., 1.],
        neighbors: [u32::MAX; 4],
        weights: [0.; 4],
        neutral: [0.; 4],
    };
    // Normal checkpoint, false recursive convergence, genuine convergence,
    // and an earlier breakdown that must survive a checkpoint.
    for (x, recursive_done, invalid, fixed_rhs_max) in [
        (0., 0., 0., 0.),
        (0., 1., 0., 0.),
        (2., 0., 0., 0.),
        (0., 1., 1., 0.),
        (0., 0., 0., 7.),
    ] {
        let mut b = vec![
            (
                0,
                gpu.uniform(&SolverParams {
                    size: [1, 1, 1, 0],
                    metric: [1., 1., 0., 0.],
                    tolerance: [1e-6, fixed_rhs_max, 0., 0.],
                }),
            ),
            (1, gpu.upload("checkpoint equation", &[equation]).unwrap()),
            (
                2,
                gpu.upload("checkpoint solution", &[[x, 0., 0., 0.]])
                    .unwrap(),
            ),
            (
                3,
                gpu.upload("checkpoint recursive residual", &[[3_f32, 4., 11., 12.]])
                    .unwrap(),
            ),
            (
                4,
                gpu.upload("checkpoint direction", &[[7_f32, 8.]]).unwrap(),
            ),
            (6, gpu.storage("checkpoint reduction", 16).unwrap()),
            (
                7,
                gpu.upload(
                    "checkpoint state",
                    &[17_f32, 5., 2., recursive_done, 99., 1e-6, invalid, 1.],
                )
                .unwrap(),
            ),
        ];
        let mut trace = gpu.trace(false, 2);
        dispatch(&gpu, Kernel::FieldSolveInit, &mut b, 1, &mut trace);
        gpu.field_dispatch(Kernel::FieldSolveState, &mut b, 1, &mut trace);
        let read = |slot| {
            let bytes = gpu.readback(buffer(&b, slot)).unwrap();
            bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()
        };
        let state = read(7);
        assert_eq!(state[3], if x == 2. { 1. } else { 0. });
        assert_eq!(state[4], (2_f32 - x).abs());
        assert_eq!(
            state[5],
            1e-6 * fixed_rhs_max.max(2.),
            "compaction changed the RHS-relative bound"
        );
        assert_eq!(
            state[6], invalid,
            "breakdown flags cannot be cleared by restart"
        );
        if recursive_done == 0. {
            assert_eq!(
                read(4),
                [7., 8.],
                "ordinary checkpoint reset search direction"
            );
            assert_eq!(
                read(3),
                [3., 4., 11., 12.],
                "ordinary checkpoint replaced recursive residual"
            );
            assert_eq!(&state[..3], &[17., 5., 2.]);
        } else {
            assert_eq!(
                read(4),
                [2., 0.],
                "false recursive convergence must restart from true residual"
            );
            assert_eq!(state[0], 4.);
            assert_eq!(
                read(3),
                [2., 0., 2., 0.],
                "restart must replace the cached preconditioned residual too"
            );
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_color_field_compensated_iterates_retain_small_color_updates() {
    let gpu = Gpu::new().unwrap();
    gpu.prepare_color_field().unwrap();
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Equation {
        block: [f32; 4],
        rhs: [f32; 4],
        neighbors: [u32; 4],
        weights: [f32; 4],
        neutral: [f32; 4],
    }
    assert_eq!(std::mem::size_of::<Equation>(), 80);
    // The large common color coordinate and small measured ratio cannot both
    // satisfy this residual bound when every iterate is rounded to one float.
    let rhs = [0.0024_f32, 0.0016_f32];
    {
        let equation = Equation {
            block: [1., 1., 0., -1.],
            rhs: [0.; 4],
            neighbors: [u32::MAX; 4],
            weights: [0.; 4],
            neutral: [0.; 4],
        };
        let mut b = vec![
            (
                0,
                gpu.uniform(&SolverParams {
                    size: [1, 1, 1, 0],
                    metric: [1., 1., 0., 1.],
                    tolerance: [2e-6, 0., 0., 0.],
                }),
            ),
            (
                1,
                gpu.upload("compensation probe equation", &[equation])
                    .unwrap(),
            ),
            (
                2,
                gpu.upload("compensation probe x", &[[20_f32, 20., 0., 0.]])
                    .unwrap(),
            ),
            (
                3,
                gpu.upload("compensation probe r", &[[0_f32; 4]]).unwrap(),
            ),
            (
                4,
                gpu.upload("compensation probe p", &[[1e-7_f32, -1e-7, 0., 0.]])
                    .unwrap(),
            ),
            (
                5,
                gpu.upload("compensation probe mapped", &[[0_f32; 4]])
                    .unwrap(),
            ),
            (6, gpu.storage("compensation probe partial", 16).unwrap()),
            (
                7,
                gpu.upload(
                    "compensation probe state",
                    &[1_f32, 1., 0., 0., 0., 0., 0., 1.],
                )
                .unwrap(),
            ),
        ];
        dispatch(
            &gpu,
            Kernel::FieldSolveStep,
            &mut b,
            1,
            &mut gpu.trace(false, 1),
        );
        let bytes = gpu.readback(buffer(&b, 2)).unwrap();
        let values = bytemuck::cast_slice::<u8, f32>(&bytes);
        eprintln!("compensated single-step probe: {values:?}");
        assert_eq!(
            &values[2..],
            &[1e-7, -1e-7],
            "GPU optimized away the rounding compensation"
        );
    }
    let equation = Equation {
        block: [4.0001, 4.0001, -4., 4.],
        rhs: [rhs[0], rhs[1], 0., 0.],
        neighbors: [u32::MAX; 4],
        weights: [0.; 4],
        neutral: [0.0001, 0., 0., 0.],
    };
    let equations = gpu
        .upload("compensated color equation", &[equation])
        .unwrap();
    let mut solution = gpu
        .upload("compensated color solution", &[[0_f32; 4]])
        .unwrap();
    solve(
        &gpu,
        &equations,
        &mut solution,
        Solve {
            count: 1,
            metric: [1., 1., 0., 1.],
            domain: Domain::Color,
            fixed_rhs_max: 0.,
        },
        &mut Profile::default(),
        &mut Execution::default(),
    )
    .unwrap();
    let bytes = gpu.readback(&solution).unwrap();
    let x = bytemuck::cast_slice::<u8, f32>(&bytes);
    assert!(x[2] != 0. || x[3] != 0., "compensation was lost: {x:?}");
    let a = f64::from(x[0]) + f64::from(x[2]);
    let b = f64::from(x[1]) + f64::from(x[3]);
    let residual = [
        4. * (a - b) + 0.0001 * a - f64::from(rhs[0]),
        4. * (b - a) + 0.0001 * b - f64::from(rhs[1]),
    ];
    assert!(
        residual.iter().all(|r| r.abs() <= 2e-6),
        "true residual {residual:?}, solution {x:?}"
    );
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_color_field_local_keeps_exactly_64_damped_sweeps_with_packed_scalars() {
    let gpu = Gpu::new().unwrap();
    gpu.prepare_color_field().unwrap();
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct ScalarEquation {
        data: [f32; 4],
        neighbors: [u32; 4],
        weights: [f32; 4],
    }
    let count = 9usize;
    let equations: Vec<_> = (0..count)
        .map(|i| {
            let neighbors = [
                if i > 0 { (i - 1) as u32 } else { u32::MAX },
                if i + 1 < count {
                    (i + 1) as u32
                } else {
                    u32::MAX
                },
                u32::MAX,
                u32::MAX,
            ];
            let weights = [
                if i > 0 { 1. } else { 0. },
                if i + 1 < count { 1. } else { 0. },
                0.,
                0.,
            ];
            ScalarEquation {
                data: [
                    1. / 64. + weights.iter().sum::<f32>(),
                    (i as f32 - 4.) * 0.002,
                    1. / 64.,
                    1.,
                ],
                neighbors,
                weights,
            }
        })
        .collect();
    let mut expected = vec![0_f64; count];
    for _ in 0..64 {
        expected = equations
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let sum = f64::from(e.data[1])
                    + e.neighbors
                        .iter()
                        .zip(e.weights)
                        .filter(|(j, _)| **j != u32::MAX)
                        .map(|(j, w)| f64::from(w) * expected[*j as usize])
                        .sum::<f64>();
                expected[i] + 0.75 * (sum / f64::from(e.data[0]) - expected[i])
            })
            .collect();
    }
    let equations = gpu
        .upload("packed scalar sweep equations", &equations)
        .unwrap();
    let mut x = gpu
        .upload("packed scalar sweep initial state", &vec![0_f32; count])
        .unwrap();
    relax(
        &gpu,
        &equations,
        &mut x,
        count as u32,
        &mut Profile::default(),
        &mut Execution::default(),
    )
    .unwrap();
    assert_eq!(x.size(), count as u64 * 4);
    let bytes = gpu.readback(&x).unwrap();
    let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
    for (i, (a, b)) in actual.iter().zip(expected).enumerate() {
        assert!(
            (f64::from(*a) - b).abs() < 1e-6,
            "Local sweep {i}: {a} / {b}"
        );
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_color_field_compaction_is_stable_across_groups_and_reused_domains() {
    let gpu = Gpu::new().unwrap();
    gpu.prepare_color_field().unwrap();
    let capacity = 257_u32 * 256 + 13;
    let mut resources = vec![
        (
            5,
            gpu.storage("stable positions", u64::from(capacity) * 4)
                .unwrap(),
        ),
        (
            6,
            gpu.storage("stable indices", u64::from(capacity) * 4)
                .unwrap(),
        ),
        (
            8,
            gpu.upload("stable counters", &[11_u32, 22, 33, 44])
                .unwrap(),
        ),
        (
            19,
            gpu.storage("stable block totals", u64::from(capacity.div_ceil(256)) * 4)
                .unwrap(),
        ),
        (20, gpu.uniform(&[capacity, 0, 0, 0])),
    ];
    for count in [capacity, 1, 255, 256, 257, 65535, 65536, 65537, capacity] {
        for pattern in 0..3 {
            let flags: Vec<u32> = (0..count)
                .map(|i| {
                    let active = match pattern {
                        0 => false,
                        1 => true,
                        _ => i % 7 == 0 || i % 257 == 255 || i + 1 == count,
                    };
                    if active {
                        0
                    } else {
                        u32::MAX
                    }
                })
                .collect();
            let expected: Vec<u32> = flags
                .iter()
                .enumerate()
                .filter_map(|(i, &flag)| (flag != u32::MAX).then_some(i as u32))
                .collect();
            let mut inverse = vec![u32::MAX; count as usize];
            for (rank, &position) in expected.iter().enumerate() {
                inverse[position as usize] = rank as u32;
            }
            for slot in [0, 2, 0] {
                let old = words(&gpu, buffer(&resources, 8)).unwrap();
                replace(
                    &mut resources,
                    6,
                    gpu.upload("stable eligibility", &flags).unwrap(),
                );
                replace(&mut resources, 20, gpu.uniform(&[count, slot, 0, 0]));
                let mut trace = gpu.trace(false, 3);
                gpu.field_batch(
                    &[
                        (Kernel::FieldRank, count.div_ceil(256)),
                        (Kernel::FieldPrefix, 1),
                        (Kernel::FieldScatter, count.div_ceil(256)),
                    ],
                    &mut resources,
                    &mut trace,
                );
                let current = words(&gpu, buffer(&resources, 8)).unwrap();
                assert_eq!(current[slot as usize], expected.len() as u32);
                for i in 0..4 {
                    if i != slot as usize {
                        assert_eq!(current[i], old[i]);
                    }
                }
                assert_eq!(
                    &words(&gpu, buffer(&resources, 5)).unwrap()[..expected.len()],
                    expected
                );
                assert_eq!(words(&gpu, buffer(&resources, 6)).unwrap(), inverse);
                let blocks = words(&gpu, buffer(&resources, 19)).unwrap();
                let mut offset = 0_u32;
                for (i, chunk) in flags.chunks(256).enumerate() {
                    assert_eq!(blocks[i], offset);
                    offset += chunk.iter().filter(|&&flag| flag != u32::MAX).count() as u32;
                }
            }
        }
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_color_field_cutoff_changes_only_mixed_data_weights_not_original_neutral_weights() {
    let gpu = Gpu::new().unwrap();
    gpu.prepare_color_field().unwrap();
    let pixels: Vec<[f32; 3]> = (0..128)
        .map(|i| {
            [
                if i % 16 >= 8 && i % 2 == 1 {
                    0.33
                } else {
                    0.32
                },
                0.66,
                1.,
            ]
        })
        .collect();
    let input = gpu.upload("two-cell selection source", &pixels).unwrap();
    let original = gpu.readback(&input).unwrap();
    let black = gpu.upload("two-cell black", &[0_f32; 3]).unwrap();
    for cutoff in [0., 0.02, 0.01, 0.005] {
        let params = Params {
            size: [16, 8, 128, 0],
            grid: [2, 1, 2, 8],
            crop: [0, 0, 16, 8],
            gain: [1.; 4],
            white: [1.; 4],
            neutral: [0.284206, 0.604436, 1., cutoff],
            luminance: [-2.6575486, 3.3135306, -0.2476, 0.],
            metric: [1., 1., 0., 0.],
            tuning: [0.035, 0.75, 1., 1.],
            options: [1, 1, 0, 0],
        };
        let mut resources = vec![
            (0, gpu.uniform(&params)),
            (1, input.clone()),
            (2, input.clone()),
            (3, black.clone()),
            (4, gpu.storage("selection masks", 128 * 4).unwrap()),
            (5, gpu.storage("selection positions", 128 * 4).unwrap()),
            (6, gpu.storage("selection indices", 128 * 4).unwrap()),
            (7, gpu.storage("selection targets", 128 * 4).unwrap()),
            (8, gpu.storage("selection counters", 16).unwrap()),
            (10, gpu.storage("selection equations", 2 * 80).unwrap()),
            (11, gpu.storage("selection solution", 2 * 16).unwrap()),
            (12, gpu.storage("selection cells", 2 * 64).unwrap()),
            (13, gpu.storage("selection edges", 2 * 8).unwrap()),
            (14, gpu.storage("selection roots", 2 * 4).unwrap()),
            (15, gpu.storage("selection peaks", 2 * 8).unwrap()),
            (16, gpu.storage("selection distances", 2 * 4).unwrap()),
            (18, input.clone()),
            (19, gpu.storage("selection row blocks", 4).unwrap()),
            (20, gpu.uniform(&[2_u32, 2, 0, 0])),
        ];
        let mut trace = gpu.trace(false, 8);
        dispatch(&gpu, Kernel::FieldPrepare, &mut resources, 128, &mut trace);
        dispatch(&gpu, Kernel::FieldGrid, &mut resources, 2, &mut trace);
        gpu.field_batch(
            &[
                (Kernel::FieldRank, 1),
                (Kernel::FieldPrefix, 1),
                (Kernel::FieldScatter, 1),
            ],
            &mut resources,
            &mut trace,
        );
        assert_eq!(words(&gpu, buffer(&resources, 8)).unwrap()[2], 2);
        dispatch(&gpu, Kernel::FieldColor, &mut resources, 2, &mut trace);
        let indices = words(&gpu, buffer(&resources, 6)).unwrap();
        let bytes = gpu.readback(buffer(&resources, 10)).unwrap();
        let rows = bytemuck::cast_slice::<u8, f32>(&bytes);
        let consistent = indices[0] as usize * 20;
        let mixed = indices[1] as usize * 20;
        assert_eq!(
            rows[consistent + 3],
            4.,
            "consistent evidence must keep its original weight"
        );
        assert_eq!(rows[consistent + 16], 0.0001);
        assert_eq!(
            rows[mixed + 16],
            0.0001,
            "rejecting evidence must not increase neutral weight"
        );
        if cutoff == 0. || cutoff == 0.02 {
            assert_eq!(rows[mixed + 3], 4.);
        } else if cutoff == 0.005 {
            assert_eq!(rows[mixed + 3], 0.);
        } else {
            assert!(
                rows[mixed + 3] > 0. && rows[mixed + 3] < 4.,
                "acceptance must taper smoothly"
            );
        }
        assert_eq!(gpu.readback(&input).unwrap(), original);
    }
}

#[test]
#[ignore = "requires a GPU"]
fn gpu_color_field_cutoff_preserves_healthy_samples_brightness_and_consistent_regions() {
    let gpu = Gpu::new().unwrap();
    let width = 40;
    let height = 16;
    let neutral = [0.284206, 0.604436, 1.0];
    let luminance = [-2.6575486148565814, 3.313530567081604, -0.2476];
    let matrix = [
        4.45774, -1.90728, 0.581922, -4.22818, 4.30012, -0.701463, 3.1376, -5.81127, 3.31683,
    ];
    let metric = color_metric(neutral, matrix).unwrap();
    let pixels: Vec<[f32; 3]> = (0..width * height)
        .map(|i| {
            let col = i % width;
            if (8..16).contains(&col) {
                [if col % 2 == 0 { 0.32 } else { 0.33 }, 0.66, 1.]
            } else if (24..32).contains(&col) {
                [0.32, 0.66, 1.]
            } else {
                [0.27, 0.60, 0.94]
            }
        })
        .collect();
    let input = gpu.upload("selectivity source", &pixels).unwrap();
    let black = gpu.upload("selectivity black", &[0_f32; 3]).unwrap();
    for brightness in [1, 2] {
        let mut baseline = Vec::<f32>::new();
        for amount in [0., 0.02, 0.01, 0.005] {
            let prepared = Prepared {
                params: Params {
                    size: [width, height, width * height, 0],
                    grid: [5, 2, 10, 8],
                    crop: [0, 0, width, height],
                    gain: [1.; 4],
                    white: [1.; 4],
                    neutral: [
                        neutral[0] as f32,
                        neutral[1] as f32,
                        neutral[2] as f32,
                        amount,
                    ],
                    luminance: [
                        luminance[0] as f32,
                        luminance[1] as f32,
                        luminance[2] as f32,
                        0.,
                    ],
                    metric,
                    tuning: [0.035, 0.75, 1., 1.],
                    options: [brightness, 1, 0, 0],
                },
                repairs: Vec::new(),
            };
            let output = apply(
                &gpu,
                Inputs {
                    input: &input,
                    original: &input,
                    black: &black,
                    unrepaired: &input,
                },
                &prepared,
                &mut Profile::default(),
                &mut Execution::default(),
            )
            .unwrap();
            let bytes = gpu.readback(&output).unwrap();
            let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
            if amount == 0. {
                baseline = actual.to_vec();
            }
            let mut maximum_mixed_change = 0_f32;
            for (i, p) in actual.as_chunks::<3>().0.iter().enumerate() {
                assert!(p.iter().all(|v| v.is_finite()));
                let col = i as u32 % width;
                if !(8..16).contains(&col) && !(24..32).contains(&col) {
                    assert_eq!(*p, pixels[i], "healthy pixel {i} changed");
                } else {
                    let reference = &baseline[3 * i..3 * i + 3];
                    let tone_difference: f64 = p
                        .iter()
                        .zip(reference)
                        .zip(luminance)
                        .map(|((a, b), y)| (f64::from(*a) - f64::from(*b)) * y)
                        .sum();
                    assert!(
                        tone_difference.abs() < 2e-6,
                        "brightness changed at {i}: {amount}"
                    );
                    let delta = p
                        .iter()
                        .zip(reference)
                        .map(|(a, b)| (a - b).abs())
                        .fold(0_f32, f32::max);
                    if (24..32).contains(&col) {
                        assert!(delta < 1e-5, "consistent region changed: {delta}");
                    } else {
                        maximum_mixed_change = maximum_mixed_change.max(delta);
                    }
                }
            }
            if amount == 0.005 {
                assert!(
                    maximum_mixed_change > 1e-5,
                    "mixed evidence must be selectable"
                );
            }
            if amount == 0.02 {
                assert!(
                    maximum_mixed_change < 1e-5,
                    "accepted evidence must retain the baseline"
                );
            }
        }
    }
}

#[test]
fn selection_strength_zero_is_original_and_increasing_strength_tightens_the_cutoff() {
    let controls = Controls::default();
    assert_eq!(controls.cutoff(), 0.);
    assert_eq!(controls.selectivity, 0.);
    for (strength, cutoff) in [(0.25, 0.02), (0.5, 0.01), (1., 0.005)] {
        assert_eq!(
            Controls {
                selectivity: strength,
                ..controls
            }
            .cutoff(),
            cutoff
        );
    }
    let mut previous = f32::INFINITY;
    for strength in [
        f32::from_bits(1),
        f32::MIN_POSITIVE,
        0.001,
        0.01,
        0.1,
        0.25,
        0.5,
        1.,
    ] {
        let current = Controls {
            selectivity: strength,
            ..controls
        };
        current.validate().unwrap();
        let cutoff = current.cutoff();
        assert!(cutoff.is_finite() && (2. * cutoff).is_finite());
        assert!(cutoff > 0. && cutoff <= previous);
        previous = cutoff;
    }
}

#[test]
fn color_field_defaults_and_bounds() {
    assert_eq!(HighlightMode::default(), HighlightMode::Classic);
    assert_eq!(
        serde_json::to_string(&HighlightMode::Classic).unwrap(),
        "\"classic\""
    );
    let controls = Controls::default();
    controls.validate().unwrap();
    assert_eq!(controls.approach, Approach::Faithful);
    assert_eq!(controls.selectivity, 0.);
    assert_eq!(serde_json::from_str::<Controls>("{}").unwrap(), controls);
    for selectivity in [0., 0.25, 0.5, 1.] {
        Controls {
            selectivity,
            ..controls
        }
        .validate()
        .unwrap();
    }
    for invalid in [f32::NAN, f32::INFINITY, -0.001, 1.001] {
        assert!(Controls {
            selectivity: invalid,
            ..controls
        }
        .validate()
        .is_err());
    }
}
