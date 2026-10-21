//! Checks signed photographic ordering and adjusted responses against scalar code-domain evaluation.

use super::*;
use crate::{color_space::TransferFunction, gpu::Gpu, profile::Profile};

const ROOT_SAMPLES: [[i32; 3]; 13] = [
    [-4096, -1024, -1],
    [0, 1, 31],
    [32, 815, 816],
    [817, 818, 819],
    [820, 1024, 2048],
    [2048, 4096, 6144],
    [4096, 2048, 1024],
    [6144, 4096, 2048],
    [8128, 8159, 8160],
    [8161, 8192, 12288],
    [16384, 24576, 32767],
    [4096, 4096, 4096],
    [800, 4096, 7000],
];

fn lookup(prepared: &Prepared, table: usize, code: i32) -> i32 {
    prepared.tables[table * LUT_LEN + (code as u16) as usize]
}

fn scalar_tone(prepared: &Prepared, root: [i32; 3], detail: i32) -> [i32; 3] {
    let addition = (i64::from(detail) * i64::from(prepared.detail_q14)) >> 14;
    std::array::from_fn(|channel| {
        let code = i64::from(root[channel]) + addition;
        let linear = lookup(prepared, 0, code as i32).min(32767);
        let root = lookup(prepared, 1, linear);
        lookup(prepared, 2 + channel, root)
    })
}

fn scalar_saturate(prepared: &Prepared, rgb: [i32; 3]) -> [i32; 3] {
    std::array::from_fn(|channel| {
        let sum: i64 = rgb
            .iter()
            .enumerate()
            .map(|(j, value)| i64::from(prepared.saturation[channel * 3 + j]) * i64::from(*value))
            .sum();
        ((sum + 2048) >> 12) as i32
    })
}

fn scalar_response(prepared: &Prepared, codes: [i32; 3]) -> [f32; 3] {
    codes.map(|code| f32::from_bits(lookup(prepared, 5, code) as u32))
}

#[test]
fn saturation_uses_equal_channel_weights_and_reference_integer_rounding() {
    for saturation in [-1., -0.65, 0., 0.8, 1.] {
        let p = prepare(
            Controls {
                saturation,
                ..Controls::default()
            },
            [1.; 3],
            ToneAdjustments::default(),
            TransferFunction::Power(461. / 256.),
        )
        .unwrap();
        // A unit control changes saturation by one half. The neutral axis is
        // the arithmetic mean, unlike the weighted-luminance floating node.
        let off = (-saturation / 6. * 4096. + 0.5) as i32;
        let diagonal = ((1. + saturation / 3.) * 4096. + 0.5) as i32;
        for row in 0..3 {
            for column in 0..3 {
                assert_eq!(
                    p.saturation[row * 3 + column],
                    if row == column { diagonal } else { off }
                );
            }
        }
        let rgb = [300, 1800, 7200];
        let expected = std::array::from_fn(|channel| {
            let other = rgb[(channel + 1) % 3] + rgb[(channel + 2) % 3];
            (diagonal * rgb[channel] + off * other + 2048) >> 12
        });
        assert_eq!(scalar_saturate(&p, rgb), expected);
        // Integer coefficient rounding can slightly shift even a neutral input.
        let neutral = scalar_saturate(&p, [4096; 3]);
        assert_eq!(neutral, [diagonal + 2 * off; 3]);
    }
}

#[test]
#[ignore = "requires a working GPU adapter"]
fn gpu_signed_response_matches_scalar_with_adjusted_tone_saturation_and_detail() {
    let gpu = Gpu::new().unwrap();
    let source: Vec<f32> = ROOT_SAMPLES
        .into_iter()
        .flatten()
        .map(|v| v as f32)
        .collect();
    let detail: Vec<f32> = (0..ROOT_SAMPLES.len())
        .map(|i| [-31., 0., 47.][i % 3])
        .collect();
    let source_gpu = gpu
        .upload("Signed response boundary roots", &source)
        .unwrap();
    let detail_gpu = gpu
        .upload("Signed response signed detail", &detail)
        .unwrap();
    let controls = [
        Controls::default(),
        Controls {
            exposure: 0.7,
            shadows: -0.4,
            contrast: 0.3,
            highlights: -0.2,
            saturation: 0.8,
            detail: 0.5,
        },
        Controls {
            exposure: -0.8,
            shadows: 0.6,
            contrast: -0.5,
            highlights: 0.4,
            saturation: -0.65,
            detail: -1.,
        },
    ];
    let mut ordering_discriminators = 0;
    let mut control_outputs = Vec::new();
    for c in controls {
        let prepared = prepare(
            c,
            [0.9, 1.1, 0.8],
            ToneAdjustments {
                exposure: 0.6,
                shadows: -0.2,
                contrast: 0.4,
                highlights: 0.2,
            },
            TransferFunction::Power(461. / 256.),
        )
        .unwrap();
        let output = apply(
            &gpu,
            &source_gpu,
            &detail_gpu,
            [ROOT_SAMPLES.len() as u32, 1],
            &prepared,
            &mut Profile::default(),
        )
        .unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        for (pixel, root) in ROOT_SAMPLES.into_iter().enumerate() {
            let toned = scalar_tone(&prepared, root, detail[pixel] as i32);
            let expected = scalar_response(&prepared, scalar_saturate(&prepared, toned));
            for channel in 0..3 {
                assert_eq!(
                    actual[pixel * 3 + channel].to_bits(),
                    expected[channel].to_bits(),
                    "pixel {pixel}, channel {channel}, controls {c:?}"
                );
            }
            // Guard the fixture itself: moving saturation before channel tone
            // must produce a detectably different result for these colors.
            let reordered = scalar_tone(
                &prepared,
                scalar_saturate(&prepared, root),
                detail[pixel] as i32,
            );
            ordering_discriminators +=
                usize::from(scalar_response(&prepared, reordered) != expected);
        }
        control_outputs.push(bytes);
    }
    assert!(
        ordering_discriminators > 10,
        "samples must detect a saturation/tone reordering"
    );
    assert!(control_outputs.windows(2).all(|pair| pair[0] != pair[1]));
    assert_eq!(
        gpu.readback(&source_gpu).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&source)
    );
}
