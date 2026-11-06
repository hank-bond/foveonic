//! Checks fused Huffman differences against category decoding and measures exact original-file loading.

use super::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

// Only the explicitly ignored, serial original-file comparison enables this.
static CATEGORY_ONLY: AtomicBool = AtomicBool::new(false);

pub(super) fn dispatch(
    table: &Table,
    input: &[u8],
    width: usize,
    height: usize,
    seed: u16,
    maximum: u16,
) -> Option<Result<Vec<u16>>> {
    CATEGORY_ONLY
        .load(Ordering::Relaxed)
        .then(|| category_plane(table, input, width, height, seed, maximum))
}

fn category_plane(
    table: &Table,
    input: &[u8],
    width: usize,
    height: usize,
    seed: u16,
    maximum: u16,
) -> Result<Vec<u16>> {
    let mut output = Vec::with_capacity(width * height);
    let mut bits = Bits::new(input)?;
    let mut vertical = [[seed; 2]; 2];
    let mut horizontal = [seed; 2];
    for y in 0..height {
        crate::background::checkpoint_or_cancel()?;
        for x in 0..width {
            let predictor = if x < 2 {
                if y < 2 {
                    seed
                } else {
                    vertical[y & 1][x & 1]
                }
            } else {
                horizontal[x & 1]
            };
            let value = (predictor as i16).wrapping_add(table.difference(&mut bits)?) as u16;
            if value > maximum {
                return Err(damaged("decoded sample outside declared range"));
            }
            horizontal[x & 1] = value;
            if x < 2 {
                vertical[y & 1][x & 1] = value;
            }
            output.push(value);
        }
    }
    Ok(output)
}

fn assert_same<T: std::fmt::Debug + PartialEq>(expected: Result<T>, actual: Result<T>) {
    assert_eq!(expected.as_ref().ok(), actual.as_ref().ok());
    assert_eq!(
        expected.as_ref().err().map(ToString::to_string),
        actual.as_ref().err().map(ToString::to_string)
    );
}

#[test]
fn fused_differences_preserve_payloads_and_every_short_bit_boundary() {
    let mut long = vec![0; 18];
    long[0] = 1;
    long[17] = 0x8001;
    for table in [
        Table::new(&[(0, 1)]).unwrap(),
        Table::new(&[(0, 1), (2048, 2), (3072, 2)]).unwrap(),
        Table::new(&(0..13).map(|i| (i << 8, 4)).collect::<Vec<_>>()).unwrap(),
        Table::packed(&long).unwrap(),
    ] {
        let lookup = table.differences();
        for prefix in 0..4096u16 {
            let input = [((prefix << 4) >> 8) as u8, (prefix << 4) as u8, 0x95];
            for available in 0..=24 {
                let mut category = Bits::new(&input).unwrap();
                category.total_bits = available;
                let mut fused = Bits::new(&input).unwrap();
                fused.total_bits = available;
                let expected = table.difference(&mut category);
                let actual = table.looked_up_difference(&lookup, &mut fused);
                if expected.is_ok() {
                    assert_eq!(category.position, fused.position);
                }
                assert_same(expected, actual);
            }
        }
    }
}

#[test]
fn fused_planes_preserve_predictors_damage_and_geometry_limits() {
    let table = Table::new(&[(0, 1), (2048, 2), (3072, 2)]).unwrap();
    let input: Vec<_> = (0..83).map(|i| (i * 73 + 19) as u8).collect();
    for width in [1, 2, 3, 7, 8, 17] {
        for height in [1, 2, 3, 9] {
            for length in [0, 1, 2, 7, input.len()] {
                for seed in [0, 512, 4095] {
                    assert_same(
                        category_plane(&table, &input[..length], width, height, seed, 4095),
                        table.decode(&input[..length], width, height, seed),
                    );
                }
            }
        }
    }
    for (width, height, seed) in [
        (0, 1, 0),
        (1, 0, 0),
        (1, 1, 4096),
        (65536, 1, 0),
        (8192, 8192, 0),
    ] {
        assert!(table.decode(&[], width, height, seed).is_err());
    }
    let gate = std::sync::Arc::new(crate::background::Gate::default());
    crate::background::interruptible(gate.clone(), || {
        gate.interrupt_prefetch();
        let error = table.decode(&[0], 4, 2, 512).unwrap_err();
        assert!(error.is::<crate::background::Interrupted>());
    });
}

struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        CATEGORY_ONLY.store(false, Ordering::Relaxed);
    }
}

#[test]
#[ignore = "requires DECODER_RESEARCH_FILES, optional DECODER_RESEARCH_REPEATS, serial test execution"]
fn profile_fused_original_loading() {
    let paths: Vec<PathBuf> =
        std::env::split_paths(&std::env::var_os("DECODER_RESEARCH_FILES").unwrap()).collect();
    let repeats: usize = std::env::var("DECODER_RESEARCH_REPEATS")
        .ok()
        .map_or(5, |v| v.parse().unwrap());
    for path in paths {
        let expected = crate::source::Source::open(&path).unwrap();
        for repeat in 0..repeats {
            for category_only in [true, false] {
                CATEGORY_ONLY.store(category_only, Ordering::Relaxed);
                let reset = Reset;
                let began = Instant::now();
                let source = crate::source::Source::open(&path).unwrap();
                let milliseconds = began.elapsed().as_secs_f64() * 1000.;
                drop(reset);
                assert_eq!(source.samples(), expected.samples());
                println!(
                    "{}",
                    crate::profile::report(
                        "decoder_fused_loading",
                        crate::profile::Mode::Off,
                        serde_json::json!({"source":path,"category_only":category_only,"repeat":repeat,"open_ms":milliseconds,"stages":source.load_timings(),"exact":true})
                    )
                );
            }
        }
    }
}
