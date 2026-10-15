//! Checks layer-isolating mixtures and dedicated mono GPU arithmetic against independent scalar calculations.

use super::parameters::Parameters;
use super::*;
use crate::{gpu::Gpu, profile::Profile};

#[test]
fn mixing_controls_validate_and_keep_pure_layers() {
    for mix in [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.], [1. / 3.; 3]] {
        Controls {
            mix,
            ..Default::default()
        }
        .validate()
        .unwrap();
        assert_eq!(Controls::normalized_mix(mix).unwrap(), mix);
    }
    for mix in [[0.; 3], [-1., 1., 1.], [f32::NAN, 0., 1.]] {
        assert!(Controls::normalized_mix(mix).is_err());
        assert!(Controls {
            mix,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
    assert_eq!(
        Controls::normalized_mix([2., 1., 1.]).unwrap(),
        [0.5, 0.25, 0.25]
    );
}

pub(super) fn prepared(width: u32, height: u32) -> Prepared {
    Prepared {
        params: Parameters {
            source: [width, height, 15, 1],
            mix: [1. / 3., 1. / 3., 1. / 3., 2.],
            filter: [0.375, 0.3, 0.02, 0.0001],
            area: [0, 0, width, height],
            detail: [100., 500., 0., 0.],
        },
    }
}
#[test]
fn mono_shaders_declare_only_their_guarded_output_writable() {
    use wgpu::naga::{
        front::wgsl,
        valid::{Capabilities, GlobalUse, ValidationFlags, Validator},
    };
    let module = wgsl::parse_str(include_str!("mono.wgsl")).unwrap();
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .unwrap();
    assert_eq!(module.entry_points.len(), 7);
    for (i, entry) in module.entry_points.iter().enumerate() {
        let mut writes = Vec::new();
        for (handle, var) in module
            .global_variables
            .iter()
            .filter(|(_, var)| var.binding.is_some())
        {
            if info.get_entry_point(i)[handle].intersects(GlobalUse::WRITE | GlobalUse::ATOMIC) {
                writes.push(var.binding.as_ref().unwrap().binding);
            }
        }
        let expected: &[u32] = if entry.name == "histogram" {
            &[3, 6]
        } else {
            &[3]
        };
        assert_eq!(writes, expected, "{}", entry.name);
    }
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn dedicated_filters_match_reference_arithmetic_fixture() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/data/monochrome-reference.json"
    ))
    .unwrap();
    let values = |key: &str| {
        fixture[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect::<Vec<_>>()
    };
    let gpu = Gpu::new().unwrap();
    let mut prepared = prepared(48, 48);
    prepared.params.filter[3] = (1_f32 / 32.).powi(3);
    let raw = gpu
        .upload("Reference mono input", &values("input"))
        .unwrap();
    let mut profile = Profile::default();
    let directional = apply(&gpu, Stage::Directional, &[&raw], &prepared, &mut profile).unwrap();
    let column = apply(
        &gpu,
        Stage::Column,
        &[&raw, &directional],
        &prepared,
        &mut profile,
    )
    .unwrap();
    let histogram = apply(
        &gpu,
        Stage::Histogram,
        &[&column, &directional],
        &prepared,
        &mut profile,
    )
    .unwrap();
    let despeckle = apply(
        &gpu,
        Stage::Despeckle,
        &[&histogram, &histogram],
        &prepared,
        &mut profile,
    )
    .unwrap();
    for (name, result) in [
        ("directional", &directional),
        ("column", &column),
        ("histogram", &histogram),
        ("despeckle", &despeckle),
    ] {
        let bytes = gpu.readback(result).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        let expected = values(name);
        let max = actual
            .iter()
            .zip(&expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0_f32, f32::max);
        eprintln!("{name}: maximum reference code difference {max}");
        assert!(actual.iter().all(|v| v.is_finite()));
        assert!(max < 0.003, "{name}: {max}");
    }
    let signal = apply(&gpu, Stage::Signal, &[&despeckle], &prepared, &mut profile).unwrap();
    let expanded = apply(&gpu, Stage::Expand, &[&despeckle], &prepared, &mut profile).unwrap();
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct SharpenParams {
        source: [u32; 4],
        value: [f32; 4],
    }
    let uniform = gpu.uniform(&SharpenParams {
        source: [48, 48, 0, 0],
        value: [0.75, 0., 0., 0.],
    });
    let mut result = gpu
        .storage("Reference mono sharpen result", 48 * 48 * 12)
        .unwrap();
    let mut trace = gpu.trace(false, 1);
    gpu.dispatch(
        crate::gpu::Kernel::MonoSharpen,
        &mut [
            (0, crate::gpu::Binding::Read(&expanded)),
            (1, crate::gpu::Binding::Read(&signal)),
            (2, crate::gpu::Binding::Read(&uniform)),
            (3, crate::gpu::Binding::Write(result.write())),
        ],
        [6, 6],
        &mut trace,
    );
    let bytes = gpu.readback(&result).unwrap();
    let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
    let corrected = values("despeckle");
    let sharp = values("signed_detail");
    for (p, triple) in actual.as_chunks::<3>().0.iter().enumerate() {
        assert_eq!(triple[0], triple[1]);
        assert_eq!(triple[1], triple[2]);
        let expected = (corrected[p] * 2. + sharp[p] * 0.25 * 0.75).clamp(0., 32767.);
        assert!(
            (triple[0] - expected).abs() < 0.2,
            "pixel {p}: {} vs {expected}",
            triple[0]
        );
    }
}

/// Independent rolling-bin oracle keeps the old column/strip accumulation order.
fn rolling_histogram(source: &[f32], borders: &[f32], prepared: &Prepared) -> Vec<f32> {
    let [width, height, _, strips] = prepared.params.source.map(|v| v as usize);
    let (indexes, weights) = prepared.table(false);
    let sample_bin = |v: f32| (v as i32).clamp(0, 4095) as usize >> 2;
    let mut output = borders.to_vec();
    for strip in 0..strips {
        let first = (strip * height / strips).max(9);
        let last = ((strip + 1) * height / strips - 1).min(height - 10);
        for x in 9..width - 9 {
            let down = (x - 9) % 2 == 0;
            let mut y = if down { first } else { last };
            let mut counts = [0_u32; 1024];
            let mut sums = [0_f32; 1024];
            for yy in y - 9..=y + 9 {
                for xx in x - 9..=x + 9 {
                    let value = source[yy * width + xx];
                    let bin = sample_bin(value);
                    sums[bin] += value;
                    counts[bin] += 1;
                }
            }
            loop {
                let center = sample_bin(source[y * width + x]);
                let offset = indexes[center * 3] as usize;
                let first_bin = indexes[center * 3 + 1] as usize;
                let count = indexes[center * 3 + 2] as usize;
                let mut sum = 0_f32;
                let mut total = 0_f32;
                for k in 0..count {
                    let bin = first_bin + k;
                    if counts[bin] != 0 {
                        let weight = weights[offset + k];
                        sum = weight.mul_add(sums[bin], sum);
                        total = weight.mul_add(counts[bin] as f32, total);
                    }
                }
                output[y * width + x] = sum / total;
                if y == if down { last } else { first } {
                    break;
                }
                let leaving = if down { y - 9 } else { y + 9 };
                let entering = if down { y + 10 } else { y - 10 };
                for xx in x - 9..=x + 9 {
                    let value = source[leaving * width + xx];
                    let bin = sample_bin(value);
                    sums[bin] -= value;
                    counts[bin] -= 1;
                }
                for xx in x - 9..=x + 9 {
                    let value = source[entering * width + xx];
                    let bin = sample_bin(value);
                    sums[bin] += value;
                    counts[bin] += 1;
                }
                y = if down { y + 1 } else { y - 1 };
            }
        }
    }
    output
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn histogram_reuses_bins_across_strips_with_partial_groups_and_immutable_inputs() {
    let gpu = Gpu::new().unwrap();
    let mut prepared = prepared(83, 344);
    prepared.params.source[3] = 8;
    let source: Vec<f32> = (0..83 * 344)
        .map(|p| {
            if p % 127 == 0 {
                32766.
            } else if p % 73 == 0 {
                0.
            } else {
                100. + ((p * 17 + p / 83 * 73) % 521) as f32 / 7.
            }
        })
        .collect();
    let borders: Vec<f32> = (0..source.len()).map(|p| -123. - (p % 31) as f32).collect();
    let input = gpu.upload("Strip-reset histogram source", &source).unwrap();
    let neighbors = gpu.upload("Distinct histogram borders", &borders).unwrap();
    for divisor in [0.5, 0.3, 0.05] {
        prepared.params.filter[1] = divisor;
        let expected = rolling_histogram(&source, &borders, &prepared);
        let mut previous = None;
        for _ in 0..3 {
            let output = apply(
                &gpu,
                Stage::Histogram,
                &[&input, &neighbors],
                &prepared,
                &mut Profile::default(),
            )
            .unwrap();
            let bytes = gpu.readback(&output).unwrap();
            let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
            let max = actual
                .iter()
                .zip(&expected)
                .map(|(a, b)| (a - b).abs())
                .fold(0_f32, f32::max);
            assert!(actual.iter().all(|v| v.is_finite()));
            assert!(max < 0.005, "divisor {divisor}: {max}");
            for y in 0..344 {
                for x in 0..83 {
                    let p = y * 83 + x;
                    if !(9..74).contains(&x) || !(9..335).contains(&y) {
                        assert_eq!(actual[p], borders[p]);
                    }
                }
            }
            if let Some(previous) = &previous {
                assert!(previous == &bytes, "histogram output changed on repeat");
            } else {
                previous = Some(bytes);
            }
        }
    }
    assert_eq!(
        gpu.readback(&input).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&source)
    );
    assert_eq!(
        gpu.readback(&neighbors).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&borders)
    );
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn pure_layer_mixing_precedes_noise_and_color_conversion() {
    let gpu = Gpu::new().unwrap();
    let mut prepared = prepared(64, 80);
    let source: Vec<f32> = (0..64 * 80)
        .flat_map(|p| [10. + p as f32, 100., 700.])
        .collect();
    let input = gpu.upload("Layer isolation fixture", &source).unwrap();
    for c in 0..3 {
        prepared.params.mix[..3].fill(0.);
        prepared.params.mix[c] = 1.;
        let result = apply(
            &gpu,
            Stage::Mix,
            &[&input],
            &prepared,
            &mut Profile::default(),
        )
        .unwrap();
        let result = gpu.readback(&result).unwrap();
        let values = bytemuck::cast_slice::<u8, f32>(&result);
        for (p, value) in values.iter().enumerate() {
            assert_eq!(*value, source[p * 3 + c]);
        }
    }
    assert_eq!(
        gpu.readback(&input).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&source)
    );
}
