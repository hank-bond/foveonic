//! Executes delayed five-row highlight neutralization with explicit threshold controls.
use crate::{
    gpu::{
        Binding::{Read, Write},
        Gpu, Kernel, ReadGuard,
    },
    profile::Profile,
    source::Result,
};
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Prepared {
    pub extent: [u32; 2],
    pub fix: [i32; 3],
    pub apriori: [i32; 3],
    pub scaled: [i32; 3],
    pub bulb: bool,
}
impl Prepared {
    /// Prepare functional neutralization thresholds. The recipe supplies its
    /// calibrated saturation levels and selects exposure-dependent controls.
    pub(crate) fn new(
        extent: [u32; 2],
        neutral: [f64; 3],
        saturation: [i32; 3],
        shutter: f32,
        gain: f32,
        bulb: bool,
    ) -> Result<Self> {
        if neutral.iter().any(|v| !v.is_finite() || *v <= 0.)
            || saturation.iter().any(|v| *v <= 0)
            || !shutter.is_finite()
            || !gain.is_finite()
        {
            return Err("invalid highlight neutralization controls".into());
        }
        let minimum = (0..3)
            .map(|c| f64::from(saturation[c] + (saturation[c] >> 3)) / neutral[c])
            .fold(f64::INFINITY, f64::min);
        let apriori = neutral.map(|v| (v * minimum + 0.5) as i32);
        let total: i32 = apriori.iter().sum();
        let scaled =
            apriori.map(|v| (f64::from(v.wrapping_shl(12)) / f64::from(total) + 0.5) as i32);
        let half = *apriori.iter().min().unwrap() / 2;
        let mut fix = apriori
            .map(|v| (f64::from(v - half) * if gain >= 1. { 1.8 } else { 1.6 } + 0.5) as i32);
        if shutter > 2. {
            fix = fix.map(|v| (f64::from(v) * 0.5 + 0.5) as i32);
        } else if shutter > 1. {
            let factor = (shutter - 1.) * -0.5 + 1.;
            fix = fix.map(|v| (v as f32 * factor + 0.5) as i32);
        }
        Ok(Self {
            extent,
            fix,
            apriori,
            scaled,
            bulb,
        })
    }
}

pub(crate) fn apply(
    gpu: &Gpu,
    input: &ReadGuard,
    p: &Prepared,
    profile: &mut Profile,
) -> Result<ReadGuard> {
    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Params {
        size: [u32; 4],
        fix: [i32; 4],
        apriori: [i32; 4],
        scaled: [i32; 4],
    }
    let [w, h] = p.extent;
    if w == 0 || h == 0 || input.size() != u64::from(w) * u64::from(h) * 12 {
        return Err("highlight stream requires matching signed RGB geometry".into());
    }
    // Keep this matched to WORKGROUP_THREADS in the adjacent shader.
    const WORKGROUP_THREADS: u32 = 256;
    const INITIALIZE: u32 = 0;
    const ORDERED_SWEEP: u32 = 1;
    let pad = |v: [i32; 3]| [v[0], v[1], v[2], 0];
    let params = Params {
        size: [w, h, u32::from(p.bulb), INITIALIZE],
        fix: pad(p.fix),
        apriori: pad(p.apriori),
        scaled: pad(p.scaled),
    };
    let initialize = gpu.uniform(&params);
    let mut ordered_params = params;
    ordered_params.size[3] = ORDERED_SWEEP;
    let ordered = gpu.uniform(&ordered_params);
    let mut output = gpu.storage("Highlight neutralized signed RGB", input.size())?;
    let mut history = gpu.storage(
        "Highlight flag bits and row bounds",
        (2 * u64::from(h) + (u64::from(w) * u64::from(h)).div_ceil(32)) * 4,
    )?;
    let mut scratch = gpu.storage("Highlight three-row RGB", u64::from(w) * 3 * 3 * 4)?;
    let mut trace = gpu.trace(profile.mode.gpu(), 2);
    gpu.dispatch(
        Kernel::HighlightStream,
        &mut [
            (0, Read(&initialize)),
            (1, Read(input)),
            (2, Write(output.write())),
            (3, Write(history.write())),
            (4, Write(scratch.write())),
        ],
        [w.div_ceil(WORKGROUP_THREADS), h],
        &mut trace,
    );
    gpu.dispatch(
        Kernel::HighlightStream,
        &mut [
            (0, Read(&ordered)),
            (1, Read(input)),
            (2, Write(output.write())),
            (3, Write(history.write())),
            (4, Write(scratch.write())),
        ],
        [1, 1],
        &mut trace,
    );
    gpu.finish_trace(&trace, profile)?;
    Ok(output)
}

#[cfg(test)]
#[test]
#[ignore = "requires a working GPU adapter"]
fn gpu_stream_matches_independent_stateful_fixtures() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/data/highlight-stream-reference.json"
    ))
    .unwrap();
    let gpu = Gpu::new().unwrap();
    for case in cases.as_array().unwrap() {
        let p = Prepared {
            extent: std::array::from_fn(|i| case["extent"][i].as_u64().unwrap() as u32),
            fix: [100, 200, 50],
            apriori: [300, 400, 250],
            scaled: [1024, 2048, 1024],
            bulb: case["bulb"].as_bool().unwrap(),
        };
        let values = |name: &str| -> Vec<f32> {
            case[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap() as f32)
                .collect()
        };
        let source = values("source");
        let input = gpu.upload("Highlight stream fixture", &source).unwrap();
        let output = apply(&gpu, &input, &p, &mut Profile::default()).unwrap();
        let bytes = gpu.readback(&output).unwrap();
        let expected = values("expected");
        let actual = bytemuck::cast_slice::<u8, f32>(&bytes);
        let differences: Vec<_> = actual
            .iter()
            .zip(&expected)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .take(10)
            .collect();
        assert!(
            differences.is_empty(),
            "extent {:?} bulb {}: {:?}",
            p.extent,
            p.bulb,
            differences
        );
        assert_eq!(
            gpu.readback(&input).unwrap(),
            bytemuck::cast_slice::<f32, u8>(&source)
        );
    }
}

#[cfg(test)]
#[test]
fn actual_sd15_thresholds_match_captured_caller() {
    let p = Prepared::new(
        [2652, 1760],
        [1.; 3],
        [5491, 4362, 4556],
        0.003_01,
        0.772_003_6,
        false,
    )
    .unwrap();
    assert_eq!(p.fix, [3926; 3]);
    assert_eq!(p.apriori, [4907; 3]);
    assert_eq!(p.scaled, [1365; 3]);
}

#[cfg(test)]
#[test]
#[ignore = "requires a GPU and CLASSIC_HIGHLIGHT_REFERENCE_DIR with captured SD15 stage buffers"]
fn gpu_stream_matches_full_sd15_stage() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("CLASSIC_HIGHLIGHT_REFERENCE_DIR")
            .expect("set CLASSIC_HIGHLIGHT_REFERENCE_DIR to captured SD15 stage buffers"),
    );
    let source = std::fs::read(directory.join("repair.layers.f32")).unwrap();
    let expected = std::fs::read(directory.join("highlight.layers.f32")).unwrap();
    let gpu = Gpu::new().unwrap();
    let input = gpu
        .upload(
            "Captured SD15 repaired layers",
            bytemuck::cast_slice::<u8, f32>(&source),
        )
        .unwrap();
    let p = Prepared {
        extent: [2688, 1792],
        fix: [3926; 3],
        apriori: [4907; 3],
        scaled: [1365; 3],
        bulb: false,
    };
    for iteration in 0..4 {
        let mut profile = Profile::new(crate::profile::Mode::Gpu);
        let started = std::time::Instant::now();
        let output = apply(&gpu, &input, &p, &mut profile).unwrap();
        let actual = gpu.readback(&output).unwrap();
        assert_eq!(actual.len(), expected.len());
        let differences = actual
            .as_chunks::<4>()
            .0
            .iter()
            .zip(expected.as_chunks::<4>().0.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            differences, 0,
            "full SD15 highlight stage must remain exact"
        );
        eprintln!(
            "highlight iteration {iteration}: {:.3} ms, {:?}",
            started.elapsed().as_secs_f64() * 1000.,
            profile
        );
    }
    assert_eq!(gpu.readback(&input).unwrap(), source);
}
