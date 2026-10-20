//! Checks original-sample reliability quantization separately from the floating field equations.
use super::*;

pub(super) fn masks(gpu: &Gpu, params: &Params, original: &[f32], black: &[f32]) -> Vec<u32> {
    let n = (original.len() / 3) as u32;
    let mut params = *params;
    params.size = [n, 1, n, 0];
    params.crop = [0, 0, n, 1];
    let empty = gpu
        .upload("mask test calibrated", &vec![0_f32; original.len()])
        .unwrap();
    from_buffers(
        gpu,
        &params,
        &gpu.upload("mask test original", original).unwrap(),
        &gpu.upload("mask test black", black).unwrap(),
        &empty,
    )
}

pub(super) fn from_buffers(
    gpu: &Gpu,
    params: &Params,
    original: &ReadGuard,
    black: &ReadGuard,
    input: &ReadGuard,
) -> Vec<u32> {
    let mut params = *params;
    params.size[3] = 0;
    params.options[2] = 0;
    let n = params.size[2];
    gpu.prepare_color_field().unwrap();
    let mut resources = vec![
        (0, gpu.uniform(&params)),
        (1, input.clone()),
        (2, original.clone()),
        (3, black.clone()),
        (
            4,
            gpu.storage("mask test reliability", u64::from(n) * 4)
                .unwrap(),
        ),
        (
            5,
            gpu.storage("mask test positions", u64::from(n) * 4)
                .unwrap(),
        ),
        (
            6,
            gpu.storage("mask test indices", u64::from(n) * 4).unwrap(),
        ),
        (
            7,
            gpu.storage("mask test targets", u64::from(n) * 4).unwrap(),
        ),
        (8, gpu.storage("mask test counts", 16).unwrap()),
        (18, input.clone()),
    ];
    dispatch(
        gpu,
        Kernel::FieldPrepare,
        &mut resources,
        n,
        &mut gpu.trace(false, 2),
    );
    words(gpu, buffer(&resources, 4)).unwrap()
}

#[test]
#[ignore = "requires GPU and FOVEONIC_HIGHLIGHT_DIR"]
fn gpu_color_field_reliability_quantization_matches_original_sensor_codes() {
    use crate::{
        inspection::Content,
        render::{Engine, Settings},
        source::Source,
    };
    let path = std::path::PathBuf::from(std::env::var_os("FOVEONIC_HIGHLIGHT_DIR").unwrap())
        .join("_P2M0170.X3F");
    let source = Source::open(&path).unwrap();
    let settings = crate::sidecar::load(&path)
        .unwrap()
        .unwrap_or_else(|| Settings::new("Sunlight", "Standard"));
    let engine = Engine::new().unwrap();
    let values = engine
        .render_artifacts(
            &source,
            &settings,
            &["black.levels"],
            &mut Profile::default(),
        )
        .unwrap();
    let Content::Table { values: black, .. } = &values[0].content else {
        panic!("black table")
    };
    let calibration =
        super::super::calibration::prepare(&source.metadata, &settings.white_balance).unwrap();
    let prepared = prepare(
        &source.metadata,
        &settings.white_balance,
        calibration.params,
        true,
        Controls::default(),
    )
    .unwrap();
    let original = (0..4096).flat_map(|i| [i as f32; 3]).collect::<Vec<_>>();
    let gpu = Gpu::new().unwrap();
    let actual = masks(&gpu, &prepared.params, &original, black);
    let mut differences = 0;
    let mut maximum = 0_u8;
    for (code, &mask) in actual.iter().enumerate() {
        for (c, &level) in black.iter().enumerate().take(3) {
            let v = (code as f32 - level) / (prepared.params.white[c] - level);
            let t = ((0.99 - v) / 0.04).clamp(0., 1.);
            let expected = if v >= 0.99 {
                0
            } else {
                (t * t * (3. - 2. * t) * 255.).round().clamp(1., 255.) as u8
            };
            let q = (mask >> (c * 8)) as u8;
            let delta = q.abs_diff(expected);
            maximum = maximum.max(delta);
            if delta != 0 {
                differences += 1;
                eprintln!("reliability code {code} channel {c}: GPU {q} CPU {expected}");
            }
        }
    }
    let pixel = 2170 * source.metadata.width + 3127;
    eprintln!(
        "0170 worst pixel original {:?}, black {:?}, GPU mask {:?}",
        &source.pixels[pixel * 3..pixel * 3 + 3],
        black,
        masks(
            &gpu,
            &prepared.params,
            &source.pixels[pixel * 3..pixel * 3 + 3],
            black
        )
    );
    eprintln!("sensor-code reliability mismatches {differences}, max {maximum}");
    assert!(
        maximum <= 1,
        "float32 reliability error exceeds one quantization bin"
    );
}
