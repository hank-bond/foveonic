//! Checks independent noise endpoints, texture frequency bands and finite neutral gain on GPU.

use super::*;
use crate::noise::TextureControls;

#[test]
fn texture_color_coordinates_validate_neutrality_and_cache_identity() {
    let matrix = [
        [1.8, -0.3, 0.1, 0.],
        [-0.2, 1.3, -0.1, 0.],
        [0.05, -0.2, 1.4, 0.],
    ];
    let color = Color::new(matrix, ColorSpace::Srgb, [2, 3, 240, 78]).unwrap();
    let rgb: [f32; 3] =
        std::array::from_fn(|r| (0..3).map(|c| matrix[r][c] * color.neutral_y[c]).sum());
    assert!(rgb.iter().all(|v| (*v - 1.).abs() < 0.00001));
    assert!(
        ((0..3)
            .map(|c| color.rgb_y[c] * color.neutral_y[c])
            .sum::<f32>()
            - 1.)
            .abs()
            < 0.000001
    );
    assert!(Color::new([[0.; 4]; 3], ColorSpace::Srgb, [0, 0, 1, 1]).is_err());
    assert!(Color::new(matrix, ColorSpace::Srgb, [0, 0, 0, 1]).is_err());
    let first = Prepared::new(Controls::default(), 1., color).unwrap();
    assert!(
        first.key()
            != Prepared::new(Controls::default(), 1., Color::default())
                .unwrap()
                .key()
    );
    let p = parameters();
    let mut other = p;
    other.model[0][0] = 1.;
    assert!(
        p.key(crate::noise::RecipeStage::Texture) != other.key(crate::noise::RecipeStage::Texture)
    );
}

#[test]
#[ignore = "requires a GPU"]
fn weak_color_safeguard_repairs_false_color_and_preserves_supported_texture() {
    let gpu = Gpu::new().unwrap();
    let mut p = parameters();
    p.model[0] = [16., 16., 16., 0.];
    let n = p.atlas_pixels() as usize;
    let mut raw = vec![20_f32; n * 3];
    let mut base = vec![1_f32; n * 3];
    let bad = 40 * 256 + 128;
    let noisy = 40 * 256 + 140;
    let supported = 40 * 256 + 152;
    base[bad * 3..bad * 3 + 3].copy_from_slice(&[0.5, 1.7, 0.8]);
    raw[bad * 3..bad * 3 + 3].copy_from_slice(&[30., 8., 25.]);
    for i in [noisy, supported] {
        base[i * 3..i * 3 + 3].copy_from_slice(&[0.7, 1.6, 0.7]);
    }
    raw[noisy * 3..noisy * 3 + 3].copy_from_slice(&[12., 12., 12.]);
    raw[supported * 3..supported * 3 + 3].copy_from_slice(&[14., 32., 14.]);
    let input = gpu.upload("Weak-color conditioned input", &raw).unwrap();
    let reference = gpu.upload("Weak-color reference", &base).unwrap();
    let ratio = gpu
        .upload("Unused neutral luma ratio", &vec![1_f32; n])
        .unwrap();
    let color = Color {
        active: [0, 0, 256, 80],
        rgb_y: p.weights,
        neutral_y: [1., 1., 1., 0.],
    };
    let render = |c: Controls, color: Color| {
        let out = apply(
            &gpu,
            Inputs {
                raw: &input,
                base: Some(&reference),
                ratio: Some(&ratio),
            },
            p,
            Prepared::new(c, 1., color).unwrap(),
            Requested {
                layers: true,
                change: true,
            },
            &mut Profile::default(),
        )
        .unwrap();
        let read =
            |v: &ReadGuard| bytemuck::cast_slice::<u8, f32>(&gpu.readback(v).unwrap()).to_vec();
        (
            read(out.layers.as_ref().unwrap()),
            read(out.change.as_ref().unwrap()),
        )
    };
    let mut c = Controls {
        chroma: 4,
        luminance: 0,
        ..Default::default()
    };
    let (pixels, _) = render(c, color);
    let chroma = |v: &[f32]| {
        let mean = v.iter().sum::<f32>() / 3.;
        v.iter().map(|x| (x - mean).powi(2)).sum::<f32>().sqrt()
    };
    let expected = |i: usize| {
        let y = raw[i * 3..i * 3 + 3].iter().sum::<f32>() / 3.;
        base[i * 3..i * 3 + 3]
            .iter()
            .map(|x| x * y)
            .collect::<Vec<_>>()
    };
    assert!(chroma(&pixels[bad * 3..bad * 3 + 3]) < chroma(&expected(bad)) * 0.1);
    let retained = chroma(&pixels[noisy * 3..noisy * 3 + 3]) / chroma(&expected(noisy));
    assert!(
        (0.90..0.94).contains(&retained),
        "noisy real color retained: {retained}"
    );
    for i in 0..n {
        if i != bad && i != noisy {
            for (a, b) in pixels[i * 3..i * 3 + 3].iter().zip(&expected(i)) {
                assert!((a - b).abs() < 0.00001);
            }
        }
    }
    let limited = Color {
        active: [0, 0, 100, 80],
        ..color
    };
    let (outside, _) = render(c, limited);
    assert!(outside[bad * 3..bad * 3 + 3]
        .iter()
        .zip(expected(bad))
        .all(|(a, b)| (a - b).abs() < 0.00001));
    c.texture.fine = 0.5;
    c.texture.coarse = 1.5;
    let (detail, change) = render(c, color);
    for i in [bad, noisy, supported] {
        let y = detail[i * 3..i * 3 + 3].iter().sum::<f32>() / 3.;
        let original = raw[i * 3..i * 3 + 3].iter().sum::<f32>() / 3.;
        assert!(
            (y / original - change[i]).abs() < 0.00001,
            "luminance changed at {i}"
        );
    }
    c.texture = Default::default();
    c.chroma = 0;
    let (identity, _) = render(c, color);
    assert_eq!(identity, raw);
    assert_eq!(
        gpu.readback(&input).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&raw)
    );
    assert_eq!(
        gpu.readback(&reference).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&base)
    );
}

#[test]
fn texture_controls_start_neutral_and_validate() {
    let legacy: Controls =
        serde_json::from_str(r#"{"chroma":3,"luminance":1,"detail":0.4}"#).unwrap();
    assert_eq!(legacy.texture, TextureControls::default());
    assert_eq!(
        Prepared::new(Controls::default(), 1., Default::default())
            .unwrap()
            .uniforms
            .strength,
        [1., 1., 0.5, 0.75]
    );
    let mut selected = Controls {
        chroma: 0,
        luminance: 4,
        detail: 0.2,
        banding: 3,
        ..Default::default()
    };
    assert_eq!(
        selected.base_controls(),
        Controls {
            banding: 3,
            ..Default::default()
        }
    );
    let restored: Controls =
        serde_json::from_value(serde_json::to_value(selected).unwrap()).unwrap();
    assert_eq!(restored, selected);
    assert_eq!(
        Prepared::new(selected, 0.5, Default::default())
            .unwrap()
            .uniforms
            .steps,
        [1., 1., 2., 4.]
    );
    selected.texture.fine = 3.;
    selected.texture.coarse = 3.;
    selected.validate().unwrap();
    assert_eq!(
        serde_json::from_value::<Controls>(serde_json::to_value(selected).unwrap()).unwrap(),
        selected
    );
    for value in [f32::NAN, f32::INFINITY, -0.01, 3.01] {
        selected.texture.fine = value;
        assert!(selected.validate().is_err());
    }
}

#[test]
fn group_switches_resolve_independently_without_discarding_settings() {
    let mut selected = Controls {
        texture: TextureControls {
            fine: 1.4,
            coarse: 0.3,
            fine_size: 0.75,
            coarse_size: 1.6,
        },
        ..Default::default()
    };
    let original = selected;
    selected.detail_enabled = false;
    assert_eq!(
        Prepared::new(selected, 1., Default::default())
            .unwrap()
            .uniforms
            .strength,
        [1., 1., 0.5, 0.75]
    );
    assert_eq!(selected.texture, original.texture);
    selected.noise_enabled = false;
    assert!(!selected.merrill_effects().needs_tiles());
    assert_eq!(
        Prepared::new(selected, 1., Default::default())
            .unwrap()
            .uniforms
            .strength,
        [1., 1., 0., 0.]
    );
    selected.detail_enabled = true;
    assert_eq!(
        Prepared::new(selected, 1., Default::default())
            .unwrap()
            .uniforms
            .strength,
        [1.4, 0.3, 0., 0.]
    );
    assert_eq!(selected.texture, original.texture);
    let saved = serde_json::to_value(selected).unwrap();
    assert_eq!(serde_json::from_value::<Controls>(saved).unwrap(), selected);
    let older: Controls = serde_json::from_str(r#"{"texture":{"fine":0.8}}"#).unwrap();
    assert!(older.noise_enabled && older.detail_enabled);
}

fn parameters() -> Parameters {
    let mut p: Parameters = bytemuck::Zeroable::zeroed();
    p.source = [256, 80, 1, 1];
    p.tile = [256, 80, 0, 11];
    p.window = [0, 0, 1, 1];
    p.region = [0, 0, 256, 80];
    p.weights = [1. / 3., 1. / 3., 1. / 3., 0.];
    p.edge[2] = 5.;
    p
}

#[test]
#[ignore = "requires a GPU"]
fn signed_layer_cancellation_cannot_explode_restored_color() {
    let gpu = Gpu::new().unwrap();
    let params = parameters();
    let count = params.atlas_pixels() as usize;
    let cases = [
        [-5., 2., 3.],
        [-5., 2., 3.00001],
        [-5., -2., -3.],
        [1., 2., 3.],
    ];
    let raw: Vec<f32> = (0..count).flat_map(|i| cases[i % cases.len()]).collect();
    let original = gpu.upload("Signed near-black cancellation", &raw).unwrap();
    let base = gpu
        .upload("Positive filtered signal", &vec![2_f32; count * 3])
        .unwrap();
    let ratio = gpu
        .upload("Positive restored brightness", &vec![1_f32; count])
        .unwrap();
    let result = apply(
        &gpu,
        Inputs {
            raw: &original,
            base: Some(&base),
            ratio: Some(&ratio),
        },
        params,
        Prepared::new(
            Controls {
                chroma: 3,
                ..Default::default()
            },
            1.,
            Default::default(),
        )
        .unwrap(),
        Requested {
            layers: true,
            ..Default::default()
        },
        &mut Profile::default(),
    )
    .unwrap()
    .layers
    .unwrap();
    let bytes = gpu.readback(&result).unwrap();
    let values: &[f32] = bytemuck::cast_slice(&bytes);
    for (index, pixel) in values.as_chunks::<3>().0.iter().enumerate() {
        assert!(
            pixel
                .iter()
                .all(|v| v.is_finite() && (0. ..=3.).contains(v)),
            "unbounded color at {index}: {pixel:?}"
        );
        if index % 4 == 0 {
            assert!((pixel.iter().sum::<f32>() / 3. - 1.).abs() < 0.00001);
        }
        if index % 4 == 3 {
            for (actual, expected) in pixel.iter().zip([1.9375, 2., 2.0625]) {
                assert!(
                    (actual - expected).abs() < 0.00001,
                    "positive sample changed"
                );
            }
        }
    }
    assert_eq!(
        gpu.readback(&original).unwrap(),
        bytemuck::cast_slice::<f32, u8>(&raw)
    );
}

#[test]
#[ignore = "requires a GPU"]
fn texture_preserves_shading_and_separates_noise_and_spatial_bands() {
    let gpu = Gpu::new().unwrap();
    let p = parameters();
    let n = p.atlas_pixels() as usize;
    let base = vec![100_f32; n * 3];
    let ratio = vec![1_f32; n];
    let render = |raw: &[f32], ratio: &[f32], c: Controls| {
        let raw = gpu.upload("Texture fixture raw", raw).unwrap();
        let base = gpu.upload("Texture fixture base", &base).unwrap();
        let ratio = gpu.upload("Texture fixture cleaned ratio", ratio).unwrap();
        let output = apply(
            &gpu,
            Inputs {
                raw: &raw,
                base: Some(&base),
                ratio: Some(&ratio),
            },
            p,
            Prepared::new(c, 1., Default::default()).unwrap(),
            Requested {
                layers: true,
                ..Default::default()
            },
            &mut Profile::default(),
        )
        .unwrap()
        .layers
        .unwrap();
        let bytes = gpu.readback(&output).unwrap();
        bytemuck::cast_slice::<u8, f32>(&bytes)
            .iter()
            .map(|v| v / 100.)
            .collect::<Vec<_>>()
    };
    let c = Controls::default();
    let mut identity = c;
    identity.chroma = 0;
    identity.luminance = 0;
    let signed = [-3_f32, 0., 4.].repeat(n);
    let same = render(&signed, &ratio, identity);
    assert!(same
        .iter()
        .zip(&signed)
        .all(|(a, b)| a.to_bits() == (b / 100.).to_bits()));
    assert!(render(&base, &ratio, c)
        .iter()
        .all(|v| (*v - 1.).abs() < 1e-5));
    let mut disabled = c;
    disabled.detail_enabled = false;
    disabled.texture.fine = 1.8;
    disabled.texture.coarse = 0.2;
    disabled.texture.coarse_size = 2.;
    assert_eq!(render(&base, &ratio, disabled), render(&base, &ratio, c));
    disabled.noise_enabled = false;
    assert_eq!(render(&signed, &ratio, disabled), same);
    let signal = |x: usize| {
        0.2 + 0.1 * if x.is_multiple_of(2) { 1. } else { -1. }
            + 0.15 * (x as f32 * std::f32::consts::TAU / 32.).cos()
    };
    let raw: Vec<f32> = (0..n)
        .flat_map(|i| [100. * signal(i % 256).exp2(); 3])
        .collect();
    let mut c = c;
    c.luminance = 0;
    c.chroma = 4;
    let all = render(&raw, &ratio, c);
    for i in 0..n {
        assert!((all[i * 3].log2() - signal(i % 256)).abs() < 0.00001);
    }
    c.texture.coarse = 0.;
    let fine = render(&raw, &ratio, c);
    c.texture.coarse = 1.;
    c.texture.fine = 0.;
    let coarse = render(&raw, &ratio, c);
    let amplitude = |values: &[f32], period: f32| -> f32 {
        (64..192)
            .map(|x| {
                values[(40 * 256 + x) * 3].log2()
                    * (x as f32 * std::f32::consts::TAU / period).cos()
            })
            .sum::<f32>()
            / 64.
    };
    assert!(amplitude(&fine, 2.) > 0.18);
    assert!(amplitude(&coarse, 2.).abs() < 0.001);
    assert!(amplitude(&fine, 32.).abs() < 0.04);
    assert!(amplitude(&coarse, 32.) > 0.12);
    c.texture.fine = 1.;
    let mut previous = f32::INFINITY;
    for choice in 0..=4 {
        c.luminance = choice;
        let image = render(&raw, &ratio, c);
        let energy = image.iter().map(|v| (v - 1.).powi(2)).sum::<f32>();
        assert!(energy <= previous);
        previous = energy;
    }
    c.luminance = 4;
    assert!(render(&raw, &ratio, c)
        .iter()
        .all(|v| (*v - 1.).abs() < 0.00001));

    // Opposite red/green grain has no brightness signal. Chroma cleanup must
    // remove it while preserving the same weighted brightness at both ends.
    let colored: Vec<f32> = (0..n)
        .flat_map(|i| {
            if i.is_multiple_of(2) {
                [130., 70., 100.]
            } else {
                [70., 130., 100.]
            }
        })
        .collect();
    let mut previous = f32::INFINITY;
    for choice in 0..=4 {
        c.chroma = choice;
        let image = render(&colored, &ratio, c);
        let energy = image.iter().map(|v| (v - 1.).powi(2)).sum::<f32>();
        assert!(energy <= previous);
        previous = energy;
    }
    c.chroma = 0;
    let low = render(&colored, &ratio, c);
    c.chroma = 4;
    let high = render(&colored, &ratio, c);
    assert!(low[0] > 1.2 && low[1] < 0.8);
    for pixel in low.as_chunks::<3>().0 {
        assert!((pixel.iter().sum::<f32>() / 3. - 1.).abs() < 1e-5);
    }
    assert!(high.iter().all(|v| (*v - 1.).abs() < 1e-5));
}

#[test]
fn texture_sizes_persist_and_fit_processing_context() {
    let partial: TextureControls = serde_json::from_str(r#"{"fine":0.8,"coarse":0.2}"#).unwrap();
    assert_eq!([partial.fine_size, partial.coarse_size], [1., 1.]);
    let mut c = Controls::default();
    c.texture.fine = 0.8;
    c.texture.fine_size = 0.75;
    c.texture.coarse_size = 1.8;
    let roundtrip: Controls = serde_json::from_value(serde_json::to_value(c).unwrap()).unwrap();
    assert_eq!(roundtrip, c);
    assert_eq!(
        Prepared::new(c, 1., Default::default())
            .unwrap()
            .uniforms
            .steps,
        [0.75, 1.5, 7.2, 14.4]
    );
    assert!(
        Prepared::new(c, 1., Default::default()).unwrap().key()
            != Prepared::new(Controls::default(), 1., Default::default())
                .unwrap()
                .key()
    );
    for scale in [0.5, 1.] {
        c.texture.fine_size = 2.;
        c.texture.coarse_size = 2.;
        let p = Prepared::new(c, scale, Default::default()).unwrap();
        let support: f32 = p.uniforms.steps.iter().map(|s| (2. * s).ceil()).sum();
        assert!(support <= 64. * scale);
    }
    for value in [f32::NAN, f32::INFINITY, 0.49, 2.01] {
        c.texture.fine_size = value;
        assert!(c.validate().is_err());
        c.texture.fine_size = 1.;
        c.texture.coarse_size = value;
        assert!(c.validate().is_err());
    }
}

#[test]
#[ignore = "requires a GPU"]
fn texture_change_tracks_sizes_without_changing_layers() {
    let gpu = Gpu::new().unwrap();
    let p = parameters();
    let n = p.atlas_pixels() as usize;
    let raw: Vec<f32> = (0..n)
        .flat_map(|i| {
            let x = (i % 256) as f32;
            let signal = 0.12 * (x * std::f32::consts::TAU / 12.).cos()
                + 0.18 * (x * std::f32::consts::TAU / 48.).cos();
            [100. * signal.exp2(); 3]
        })
        .collect();
    let raw = gpu.upload("Band fixture raw", &raw).unwrap();
    let base = gpu
        .upload("Band fixture base", &vec![100_f32; n * 3])
        .unwrap();
    let ratio = gpu.upload("Band fixture ratio", &vec![1_f32; n]).unwrap();
    let render = |c: Controls, needs: Requested| {
        apply(
            &gpu,
            Inputs {
                raw: &raw,
                base: Some(&base),
                ratio: Some(&ratio),
            },
            p,
            Prepared::new(c, 1., Default::default()).unwrap(),
            needs,
            &mut Profile::new(crate::profile::Mode::Gpu),
        )
        .unwrap()
    };
    let read = |value: &ReadGuard| {
        let bytes = gpu.readback(value).unwrap();
        bytemuck::cast_slice::<u8, f32>(&bytes).to_vec()
    };
    let raw_pixels = read(&raw);
    let mut c = Controls {
        chroma: 0,
        luminance: 0,
        ..Default::default()
    };
    let neutral = render(
        c,
        Requested {
            layers: true,
            change: true,
        },
    );
    assert_eq!(read(neutral.layers.as_ref().unwrap()), read(&raw));
    assert!(read(neutral.change.as_ref().unwrap())
        .iter()
        .all(|v| *v == 1.));
    let mut previous_change = Vec::new();
    // Include fractional taps and both radius limits. The diagnostic must
    // predict the actual brightness edit, without changing its retained image.
    for (fine_size, coarse_size) in [(0.5, 0.5), (0.83, 1.37), (2., 2.)] {
        c.texture.fine_size = fine_size;
        c.texture.coarse_size = coarse_size;
        c.texture.fine = 0.25;
        c.texture.coarse = 1.75;
        let result = render(
            c,
            Requested {
                layers: true,
                change: true,
            },
        );
        let layers = read(result.layers.as_ref().unwrap());
        let change = read(result.change.as_ref().unwrap());
        let ordinary = render(
            c,
            Requested {
                layers: true,
                ..Default::default()
            },
        );
        assert_eq!(layers, read(ordinary.layers.as_ref().unwrap()));
        let diagnostic = render(
            c,
            Requested {
                change: true,
                ..Default::default()
            },
        );
        assert!(diagnostic.layers.is_none());
        assert_eq!(change, read(diagnostic.change.as_ref().unwrap()));
        for x in 64..192 {
            let i = 40 * 256 + x;
            let raw_value = raw_pixels[i * 3];
            assert!((layers[i * 3] / raw_value - change[i]).abs() < 1e-5);
        }
        if !previous_change.is_empty() {
            assert_ne!(previous_change, change);
        }
        previous_change = change;
    }
}

#[test]
fn disabled_and_neutral_detail_share_one_effective_recipe() {
    let mut c = Controls {
        noise_enabled: false,
        ..Default::default()
    };
    c.texture.fine_size = 2.;
    c.texture.coarse_size = 0.5;
    assert!(!c.merrill_effects().needs_tiles());
    let neutral = Prepared::new(c, 1., Default::default()).unwrap();
    assert!(!neutral.needs_base());
    assert!(!neutral.needs_ratio());
    c.detail_enabled = false;
    c.texture.fine = 0.2;
    assert!(!c.merrill_effects().needs_tiles());
    assert!(neutral.key() == Prepared::new(c, 1., Default::default()).unwrap().key());
    c.detail_enabled = true;
    assert!(c.merrill_effects().needs_tiles());
    assert!(Prepared::new(c, 1., Default::default())
        .unwrap()
        .needs_base());
    assert!(!Prepared::new(c, 1., Default::default())
        .unwrap()
        .needs_ratio());
    c.noise_enabled = true;
    assert!(Prepared::new(c, 1., Default::default())
        .unwrap()
        .needs_ratio());
    c.chroma = 0;
    c.luminance = 0;
    c.detail_enabled = false;
    assert!(
        c.merrill_effects().needs_tiles(),
        "column cleanup remains selected"
    );
    assert!(!Prepared::new(c, 1., Default::default())
        .unwrap()
        .needs_base());
}
